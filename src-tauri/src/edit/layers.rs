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

/// Copie pour l'impression : le contenu des calques masqués est retiré des
/// pages (sections de contenu marqué /OC, objets de formulaire et annotations
/// rattachés à ces calques) et la notion de calque disparaît, si bien que le
/// résultat ne dépend plus d'aucune configuration de visibilité.
pub fn print_copy(bytes: &[u8], password: Option<&str>, hidden: Option<&[String]>) -> Result<Vec<u8>, String> {
    let mut doc = match password {
        Some(p) => Document::load_mem_with_options(bytes, lopdf::LoadOptions::with_password(p)),
        None => Document::load_mem(bytes),
    }
    .map_err(|e| format!("lecture du PDF : {e}"))?;
    doc.encryption_state = None;
    crate::security::strip_xref_streams(&mut doc);
    let layers = list(&doc);
    let off: HashSet<String> = match hidden {
        Some(h) => h.iter().cloned().collect(),
        None => layers.iter().filter(|l| !l.visible).map(|l| l.id.clone()).collect(),
    };
    let hidden_ids: HashSet<ObjectId> = layers
        .iter()
        .filter(|l| off.contains(&l.id))
        .filter_map(|l| {
            let mut it = l.id.split('-');
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
        })
        .collect();
    let mut streams: Vec<(ObjectId, Option<ObjectId>)> = Vec::new();
    for (_, pid) in doc.get_pages() {
        let page = doc.get_dictionary(pid).map_err(|e| e.to_string())?;
        let contents: Vec<ObjectId> = match page.get(b"Contents") {
            Ok(Object::Reference(r)) => vec![*r],
            Ok(Object::Array(a)) => a.iter().filter_map(|o| o.as_reference().ok()).collect(),
            _ => Vec::new(),
        };
        if contents.len() > 1 {
            // Plusieurs flux : réunis pour que les sections marquées restent entières.
            let mut all = Vec::new();
            for c in &contents {
                if let Ok(s) = doc.get_object(*c).and_then(|o| o.as_stream()) {
                    all.extend(s.decompressed_content().unwrap_or_else(|_| s.content.clone()));
                    all.push(b'\n');
                }
            }
            let id = doc.add_object(lopdf::Stream::new(lopdf::Dictionary::new(), all));
            doc.get_dictionary_mut(pid).map_err(|e| e.to_string())?.set("Contents", id);
            streams.push((id, Some(pid)));
        } else if let Some(c) = contents.first() {
            streams.push((*c, Some(pid)));
        }
    }
    let forms: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter(|(_, o)| o.as_stream().map(|s| matches!(s.dict.get(b"Subtype"), Ok(Object::Name(n)) if n == b"Form")).unwrap_or(false))
        .map(|(id, _)| *id)
        .collect();
    streams.extend(forms.into_iter().map(|f| (f, None)));
    for (sid, page) in streams {
        let res = match page {
            Some(pid) => doc.get_dictionary(pid).ok().and_then(|p| crate::annot::pdf::inherited(&doc, p, b"Resources")).and_then(|r| r.as_dict().ok()).cloned(),
            None => doc.get_object(sid).ok().and_then(|o| o.as_stream().ok()).and_then(|s| get(&doc, &s.dict, b"Resources")).and_then(|r| r.as_dict().ok()).cloned(),
        }
        .unwrap_or_default();
        let props = get(&doc, &res, b"Properties").and_then(|p| p.as_dict().ok()).cloned().unwrap_or_default();
        let xobjects = get(&doc, &res, b"XObject").and_then(|p| p.as_dict().ok()).cloned().unwrap_or_default();
        let Ok(stream) = doc.get_object(sid).and_then(|o| o.as_stream()) else { continue };
        let data = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
        let Ok(content) = lopdf::content::Content::decode(&data) else { continue };
        let mut out = Vec::new();
        let mut stack: Vec<bool> = Vec::new();
        let mut changed = false;
        for op in content.operations {
            let skipping = stack.iter().any(|h| *h);
            match op.operator.as_str() {
                "BDC" => {
                    let hid = matches!(op.operands.first(), Some(Object::Name(n)) if n == b"OC")
                        && match op.operands.get(1) {
                            Some(Object::Name(n)) => props.get(n).map(|o| oc_hidden(&doc, &hidden_ids, o)).unwrap_or(false),
                            Some(o) => oc_hidden(&doc, &hidden_ids, o),
                            None => false,
                        };
                    stack.push(hid);
                    if skipping || hid {
                        changed = true;
                        continue;
                    }
                }
                "BMC" => {
                    stack.push(false);
                    if skipping {
                        continue;
                    }
                }
                "EMC" => {
                    let was = stack.pop().unwrap_or(false);
                    if skipping || was {
                        continue;
                    }
                }
                "Do" if !skipping => {
                    let hid = match op.operands.first() {
                        Some(Object::Name(n)) => xobjects
                            .get(n)
                            .ok()
                            .and_then(|x| x.as_reference().ok())
                            .and_then(|x| doc.get_object(x).ok())
                            .and_then(|o| o.as_stream().ok())
                            .and_then(|s| s.dict.get(b"OC").ok())
                            .map(|oc| oc_hidden(&doc, &hidden_ids, oc))
                            .unwrap_or(false),
                        _ => false,
                    };
                    if hid {
                        changed = true;
                        continue;
                    }
                }
                _ if skipping => {
                    changed = true;
                    continue;
                }
                _ => {}
            }
            out.push(op);
        }
        if changed {
            let bytes = lopdf::content::Content { operations: out }.encode().map_err(|e| e.to_string())?;
            if let Ok(s) = doc.get_object_mut(sid).and_then(|o| o.as_stream_mut()) {
                s.set_plain_content(bytes);
            }
        }
    }
    // Annotations rattachées à un calque masqué.
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for pid in pages {
        let annots: Vec<Object> = doc.get_dictionary(pid).ok().and_then(|p| get(&doc, p, b"Annots")).and_then(|a| a.as_array().ok()).cloned().unwrap_or_default();
        let keep: Vec<Object> = annots
            .iter()
            .filter(|a| !deref(&doc, a).as_dict().ok().and_then(|d| d.get(b"OC").ok()).map(|oc| oc_hidden(&doc, &hidden_ids, oc)).unwrap_or(false))
            .cloned()
            .collect();
        if keep.len() != annots.len() {
            doc.get_dictionary_mut(pid).map_err(|e| e.to_string())?.set("Annots", keep);
        }
    }
    // Plus de calques : tout ce qui reste est visible.
    let root = doc.trailer.get(b"Root").and_then(|o| o.as_reference()).map_err(|e| e.to_string())?;
    if let Ok(c) = doc.get_dictionary_mut(root) {
        c.remove(b"OCProperties");
    }
    for o in doc.objects.values_mut() {
        if let Object::Stream(s) = o {
            s.dict.remove(b"OC");
        }
    }
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| format!("écriture du PDF : {e}"))?;
    Ok(out)
}

/// Groupe (OCG) masqué, ou appartenance (OCMD, « au moins un visible ») dont tous les groupes le sont.
fn oc_hidden(doc: &Document, hidden: &HashSet<ObjectId>, o: &Object) -> bool {
    let Ok(id) = o.as_reference() else { return false };
    if hidden.contains(&id) {
        return true;
    }
    let Ok(d) = doc.get_dictionary(id) else { return false };
    if !d.has_type(b"OCMD") {
        return false;
    }
    let groups: Vec<ObjectId> = match d.get(b"OCGs") {
        Ok(Object::Array(a)) => a.iter().filter_map(|x| x.as_reference().ok()).collect(),
        Ok(Object::Reference(r)) => vec![*r],
        _ => Vec::new(),
    };
    !groups.is_empty() && groups.iter().all(|g| hidden.contains(g))
}
