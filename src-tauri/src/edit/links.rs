//! Liens (annotations /Link) : liste, ajout, modification de l'adresse, suppression.

use lopdf::{dictionary, Document, Object, ObjectId, StringFormat};
use serde::Serialize;

use super::LinkEdit;
use crate::annot::geom::Rect;
use crate::annot::pdf::{deref, get, num, page_geom, text};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkInfo {
    /// Index dans /Annots.
    pub index: u32,
    /// Rectangle en points d'affichage.
    pub rect: Rect,
    pub uri: Option<String>,
    /// Page de destination (index), pour les liens internes.
    pub page: Option<u32>,
}

/// Liens d'une page.
pub fn list(doc: &Document, page_index: u32) -> Vec<LinkInfo> {
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let Some(&pid) = pages.get(page_index as usize) else { return Vec::new() };
    let Ok(pd) = doc.get_dictionary(pid) else { return Vec::new() };
    let g = page_geom(doc, pd);
    let Some(annots) = get(doc, pd, b"Annots").and_then(|a| a.as_array().ok()) else { return Vec::new() };
    let mut out = Vec::new();
    for (i, o) in annots.iter().enumerate() {
        let Ok(d) = deref(doc, o).as_dict() else { continue };
        if d.get(b"Subtype").and_then(|s| s.as_name()).ok() != Some(b"Link".as_slice()) {
            continue;
        }
        let Some(r) = get(doc, d, b"Rect").and_then(|r| r.as_array().ok()).filter(|a| a.len() == 4) else { continue };
        let v: Vec<f32> = r.iter().map(|x| num(deref(doc, x)).unwrap_or(0.0)).collect();
        let user = Rect::from_corners(v[0], v[1], v[2], v[3]);
        let action = get(doc, d, b"A").and_then(|a| a.as_dict().ok());
        let uri = action
            .filter(|a| a.get(b"S").and_then(|s| s.as_name()).ok() == Some(b"URI".as_slice()))
            .and_then(|a| get(doc, a, b"URI"))
            .map(text);
        let dest = get(doc, d, b"Dest").or_else(|| action.and_then(|a| get(doc, a, b"D")));
        let page = dest
            .and_then(|o| o.as_array().ok())
            .and_then(|a| a.first())
            .and_then(|p| p.as_reference().ok())
            .and_then(|r| pages.iter().position(|&x| x == r))
            .map(|p| p as u32);
        out.push(LinkInfo { index: i as u32, rect: g.rect_to_display(&user), uri, page });
    }
    out
}

pub fn apply(doc: &mut Document, page: ObjectId, edits: &[LinkEdit]) -> Result<(), String> {
    let pd = doc.get_dictionary(page).map_err(|_| "page introuvable")?.clone();
    let g = page_geom(doc, &pd);
    let mut annots: Vec<Object> = get(doc, &pd, b"Annots").and_then(|a| a.as_array().ok()).cloned().unwrap_or_default();
    let mut deleted = vec![false; annots.len()];
    let mut added = Vec::new();
    for e in edits {
        match e {
            LinkEdit::Delete { index } => {
                if let Some(d) = deleted.get_mut(*index as usize) {
                    *d = true;
                }
            }
            LinkEdit::Update { index, uri } => {
                let Some(o) = annots.get(*index as usize).cloned() else { continue };
                let action = uri_action(uri);
                match o {
                    Object::Reference(id) => {
                        if let Ok(d) = doc.get_dictionary_mut(id) {
                            d.remove(b"Dest");
                            d.set("A", action);
                        }
                    }
                    Object::Dictionary(mut d) => {
                        d.remove(b"Dest");
                        d.set("A", action);
                        annots[*index as usize] = Object::Dictionary(d);
                    }
                    _ => {}
                }
            }
            LinkEdit::Add { rect, uri } => {
                let u = g.rect_to_user(rect);
                let id = doc.add_object(dictionary! {
                    "Type" => "Annot",
                    "Subtype" => "Link",
                    "Rect" => vec![Object::Real(u.x), Object::Real(u.y), Object::Real(u.x + u.w), Object::Real(u.y + u.h)],
                    "Border" => vec![0.into(), 0.into(), 0.into()],
                    "F" => 4,
                    "P" => page,
                    "A" => uri_action(uri),
                });
                added.push(Object::Reference(id));
            }
        }
    }
    let mut list: Vec<Object> = annots.into_iter().zip(deleted).filter(|(_, d)| !d).map(|(o, _)| o).collect();
    list.extend(added);
    if let Ok(d) = doc.get_dictionary_mut(page) {
        d.set("Annots", list);
    }
    Ok(())
}

fn uri_action(uri: &str) -> Object {
    let u = normalize(uri);
    Object::Dictionary(dictionary! { "S" => "URI", "URI" => Object::String(u.into_bytes(), StringFormat::Literal) })
}

/// Complète une adresse saisie sans schéma (« exemple.fr » → « https://exemple.fr »).
pub fn normalize(uri: &str) -> String {
    let t = uri.trim();
    if t.contains(':') {
        return t.to_string();
    }
    if t.contains('@') && !t.contains('/') {
        return format!("mailto:{t}");
    }
    format!("https://{t}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn adresses() {
        assert_eq!(super::normalize("exemple.fr/q4"), "https://exemple.fr/q4");
        assert_eq!(super::normalize("audit@exemple.fr"), "mailto:audit@exemple.fr");
        assert_eq!(super::normalize("https://a.b"), "https://a.b");
    }
}
