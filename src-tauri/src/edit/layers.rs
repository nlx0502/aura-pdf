//! Calques (contenu optionnel, /OCProperties) : liste et visibilité.
//!
//! PDFium dessine les calques selon la configuration par défaut du document
//! (/OCProperties /D). Pour masquer ou afficher un calque, on produit une copie
//! du fichier dont seule cette configuration change (mise à jour incrémentale :
//! quelques centaines d'octets ajoutés), que le moteur affiche à la place de
//! l'original. Le fichier de l'utilisateur n'est jamais modifié.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, IncrementalDocument, Object, ObjectId};
use serde::Serialize;

use crate::annot::pdf::{deref, get, text};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    /// Identifiant de l'objet OCG (« 12-0 »).
    pub id: String,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub depth: u32,
}

fn key(id: ObjectId) -> String {
    format!("{}-{}", id.0, id.1)
}

fn refs(doc: &Document, d: &Dictionary, k: &[u8]) -> HashSet<ObjectId> {
    get(doc, d, k).and_then(|a| a.as_array().ok()).map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect()).unwrap_or_default()
}

pub fn list(doc: &Document) -> Vec<Layer> {
    let Some(props) = doc.catalog().ok().and_then(|c| get(doc, c, b"OCProperties")).and_then(|o| o.as_dict().ok()) else { return Vec::new() };
    let ocgs: Vec<ObjectId> = get(doc, props, b"OCGs").and_then(|a| a.as_array().ok()).map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect()).unwrap_or_default();
    if ocgs.is_empty() {
        return Vec::new();
    }
    let config = get(doc, props, b"D").and_then(|o| o.as_dict().ok());
    let base_off = config.and_then(|c| get(doc, c, b"BaseState")).and_then(|o| o.as_name().ok()) == Some(b"OFF".as_slice());
    let on = config.map(|c| refs(doc, c, b"ON")).unwrap_or_default();
    let off = config.map(|c| refs(doc, c, b"OFF")).unwrap_or_default();
    let locked = config.map(|c| refs(doc, c, b"Locked")).unwrap_or_default();
    let visible = |id: &ObjectId| if base_off { on.contains(id) } else { !off.contains(id) };
    let name_of = |id: &ObjectId| doc.get_dictionary(*id).ok().and_then(|d| get(doc, d, b"Name")).map(text).unwrap_or_else(|| "Calque".into());

    let mut out = Vec::new();
    let mut seen: HashSet<ObjectId> = HashSet::new();
    // Ordre de présentation (/Order, éventuellement imbriqué), sinon ordre des OCGs.
    fn walk(doc: &Document, arr: &[Object], depth: u32, out: &mut Vec<(ObjectId, u32)>) {
        for o in arr {
            match o {
                Object::Reference(r) => {
                    if let Ok(Object::Array(sub)) = doc.get_object(*r) {
                        walk(doc, sub, depth + 1, out);
                    } else {
                        out.push((*r, depth));
                    }
                }
                Object::Array(sub) if depth < 8 => walk(doc, sub, depth + 1, out),
                _ => {}
            }
        }
    }
    let mut ordered: Vec<(ObjectId, u32)> = Vec::new();
    if let Some(order) = config.and_then(|c| get(doc, c, b"Order")).and_then(|o| o.as_array().ok()) {
        walk(doc, order, 0, &mut ordered);
    }
    let all: HashSet<ObjectId> = ocgs.iter().copied().collect();
    for (id, depth) in ordered {
        if all.contains(&id) && seen.insert(id) {
            out.push(Layer { id: key(id), name: name_of(&id), visible: visible(&id), locked: locked.contains(&id), depth });
        }
    }
    for id in ocgs {
        if seen.insert(id) {
            out.push(Layer { id: key(id), name: name_of(&id), visible: visible(&id), locked: locked.contains(&id), depth: 0 });
        }
    }
    out
}

/// Copie de `bytes` où les calques `hidden` sont masqués et les autres affichés.
pub fn with_visibility(bytes: Vec<u8>, password: Option<&str>, hidden: &[String]) -> Result<Vec<u8>, String> {
    let prev = match password {
        Some(p) => Document::load_mem_with_password(&bytes, p),
        None => Document::load_mem(&bytes),
    }
    .map_err(|e| format!("lecture du PDF : {e}"))?;
    let catalog_id = prev.trailer.get(b"Root").and_then(|o| o.as_reference()).map_err(|_| "catalogue introuvable")?;
    let catalog = prev.get_dictionary(catalog_id).map_err(|e| e.to_string())?.clone();
    let props_obj = catalog.get(b"OCProperties").map_err(|_| "aucun calque")?.clone();
    let mut props = deref(&prev, &props_obj).as_dict().map_err(|_| "calques illisibles")?.clone();
    let ocgs: Vec<ObjectId> = get(&prev, &props, b"OCGs").and_then(|a| a.as_array().ok()).map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect()).unwrap_or_default();
    let hidden: HashSet<&str> = hidden.iter().map(String::as_str).collect();
    let mut config = get(&prev, &props, b"D").and_then(|o| o.as_dict().ok()).cloned().unwrap_or_default();
    let (mut on, mut off) = (Vec::new(), Vec::new());
    for id in ocgs {
        if hidden.contains(key(id).as_str()) {
            off.push(Object::Reference(id));
        } else {
            on.push(Object::Reference(id));
        }
    }
    config.set("BaseState", Object::Name(b"ON".to_vec()));
    config.set("ON", on);
    config.set("OFF", off);
    // Les règles d'exclusion (/RBGroups) n'ont pas lieu d'être pour un choix explicite.
    config.remove(b"RBGroups");
    props.set("D", config);

    let mut inc = IncrementalDocument::create_from(bytes, prev);
    match props_obj {
        Object::Reference(id) => {
            inc.new_document.set_object(id, Object::Dictionary(props));
        }
        _ => {
            let mut c = catalog;
            c.set("OCProperties", props);
            inc.new_document.set_object(catalog_id, Object::Dictionary(c));
        }
    }
    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| format!("écriture du PDF : {e}"))?;
    Ok(out)
}
