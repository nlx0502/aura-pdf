//! Écriture des annotations par mise à jour incrémentale (lopdf).
//!
//! Le fichier d'origine n'est pas réécrit : les objets nouveaux ou modifiés sont
//! ajoutés à la fin. Les signatures existantes restent donc vérifiables, et
//! l'enregistrement reste rapide même pour de gros fichiers.

use std::collections::{HashMap, HashSet};

use lopdf::{dictionary, Dictionary, Document, IncrementalDocument, Object, ObjectId, Stream};

use super::ap::build;
use super::geom::PageGeom;
use super::model::{rgb, Annot, Kind};
use super::pdf::{classify, deref, get, now_pdf_date, page_annots, page_geom, text_object, Role};

/// Annotation (nouvelle ou régénérée) → dictionnaire complet et flux d'apparence.
fn annot_dict(a: &Annot, g: &PageGeom, page_ref: ObjectId, ap_ref: ObjectId, date: &str) -> (Dictionary, Stream) {
    let b = build(a, g);
    let (r, gc, bl) = rgb(&a.color);
    let mut d = dictionary! {
        "Type" => "Annot",
        "Subtype" => Object::Name(b.subtype.as_bytes().to_vec()),
        "Rect" => Object::Array(vec![b.rect.x.into(), b.rect.y.into(), (b.rect.x + b.rect.w).into(), (b.rect.y + b.rect.h).into()]),
        "P" => Object::Reference(page_ref),
        "NM" => text_object(&a.id),
        "M" => Object::string_literal(date),
        "F" => 4,
        "C" => Object::Array(vec![r.into(), gc.into(), bl.into()]),
        "CA" => Object::Real(a.opacity),
        "AP" => dictionary! { "N" => Object::Reference(ap_ref) },
    };
    if !a.author.is_empty() {
        d.set("T", text_object(&a.author));
    }
    // Zone de texte : le texte affiché est /Contents ; ailleurs, c'est le commentaire.
    let contents = if a.kind == Kind::Text { &a.text } else { &a.contents };
    if !contents.is_empty() {
        d.set("Contents", text_object(contents));
    }
    if a.kind == Kind::Stamp || a.kind == Kind::Note || a.kind == Kind::Text {
        d.set("CreationDate", Object::string_literal(date));
    }
    for (k, v) in b.dict.iter() {
        d.set(k.clone(), v.clone());
    }
    // Zone de texte : /C désigne la bordure (la couleur du texte est dans /DA).
    if a.kind == Kind::Text {
        match a.border_color.as_ref().filter(|_| a.width > 0.0) {
            Some(bc) => {
                let (r, g, b) = rgb(bc);
                d.set("C", Object::Array(vec![r.into(), g.into(), b.into()]));
            }
            None => {
                d.remove(b"C");
            }
        }
    }
    let bbox = Object::Array(vec![b.rect.x.into(), b.rect.y.into(), (b.rect.x + b.rect.w).into(), (b.rect.y + b.rect.h).into()]);
    let stream = Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => bbox, "Resources" => Object::Dictionary(b.resources) },
        b.content,
    );
    (d, stream)
}

/// Réponse ou changement d'état (/IRT) rattaché à une annotation.
fn reply_dict(parent: ObjectId, page_ref: ObjectId, rect: &Object, author: &str, contents: &str, state: Option<&str>, date: &str) -> Dictionary {
    let mut d = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Text",
        "IRT" => Object::Reference(parent),
        "RT" => "R",
        "P" => Object::Reference(page_ref),
        "Rect" => rect.clone(),
        "M" => Object::string_literal(date),
        // Non affichée sur la page : visible dans la liste des commentaires.
        "F" => 32,
        "Open" => false,
        "Name" => "Comment",
        "T" => text_object(author),
        "Contents" => text_object(contents),
    };
    if let Some(s) = state {
        d.set("StateModel", Object::string_literal("Review"));
        d.set("State", Object::string_literal(s));
    }
    d
}

struct Original {
    item: Object,
    oid: Option<ObjectId>,
    role: Role,
    popup_parent: Option<ObjectId>,
    irt: Option<ObjectId>,
    popup: Option<ObjectId>,
    rect: Object,
}

/// Applique `annots` (état complet des annotations modifiables) au fichier `bytes`.
pub fn save(bytes: Vec<u8>, password: Option<&str>, annots: &[Annot]) -> Result<Vec<u8>, String> {
    let prev = match password {
        Some(p) => Document::load_mem_with_password(&bytes, p),
        None => Document::load_mem(&bytes),
    }
    .map_err(|e| format!("lecture du PDF : {e}"))?;
    let date = now_pdf_date();

    // ---- Analyse des pages (avant de céder le document à la mise à jour incrémentale)
    let mut by_page: HashMap<u32, Vec<&Annot>> = HashMap::new();
    for a in annots {
        by_page.entry(a.page).or_default().push(a);
    }
    struct PagePlan {
        page_id: ObjectId,
        geom: PageGeom,
        originals: Vec<Original>,
    }
    let mut plans: Vec<(u32, PagePlan)> = Vec::new();
    for (pno, page_id) in prev.get_pages() {
        let page_idx = pno - 1;
        let Ok(page) = prev.get_dictionary(page_id) else { continue };
        let originals: Vec<Original> = {
            let raw = get(&prev, page, b"Annots").and_then(|a| a.as_array().ok()).cloned().unwrap_or_default();
            let dicts = page_annots(&prev, page);
            raw.into_iter()
                .zip(dicts)
                .map(|(item, (oid, d))| Original {
                    oid,
                    role: classify(&prev, d),
                    popup_parent: d.get(b"Parent").ok().and_then(|o| o.as_reference().ok()),
                    irt: d.get(b"IRT").ok().and_then(|o| o.as_reference().ok()),
                    popup: d.get(b"Popup").ok().and_then(|o| o.as_reference().ok()),
                    rect: d.get(b"Rect").ok().map(|o| deref(&prev, o).clone()).unwrap_or(Object::Array(vec![0.into(), 0.into(), 0.into(), 0.into()])),
                    item,
                })
                .collect()
        };
        let model = by_page.get(&page_idx).map(|v| v.as_slice()).unwrap_or(&[]);
        let kept: HashSet<u32> = model.iter().filter_map(|a| a.source).collect();
        let has_deleted = originals.iter().enumerate().any(|(i, o)| matches!(o.role, Role::Editable(_)) && !kept.contains(&(i as u32)));
        let has_changes = model.iter().any(|a| {
            a.source.is_none() || a.geom_dirty || a.meta_dirty || a.resolved != a.saved_resolved || a.replies.iter().any(|r| !r.saved)
        });
        if has_deleted || has_changes {
            plans.push((page_idx, PagePlan { page_id, geom: page_geom(&prev, page), originals }));
        }
    }
    if plans.is_empty() {
        return Ok(bytes);
    }

    // ---- Mise à jour incrémentale
    let mut inc = IncrementalDocument::create_from(bytes, prev);
    for (page_idx, plan) in plans {
        let model = by_page.get(&page_idx).cloned().unwrap_or_default();
        let by_source: HashMap<u32, &Annot> = model.iter().filter_map(|a| a.source.map(|s| (s, *a))).collect();

        // Annotations supprimées : elles, leur fenêtre contextuelle et leurs réponses.
        let mut removed: HashSet<ObjectId> = HashSet::new();
        for (i, o) in plan.originals.iter().enumerate() {
            if matches!(o.role, Role::Editable(_)) && !by_source.contains_key(&(i as u32)) {
                if let Some(id) = o.oid {
                    removed.insert(id);
                }
                if let Some(p) = o.popup {
                    removed.insert(p);
                }
            }
        }

        let mut items: Vec<Object> = Vec::new();
        // Annotation du modèle → référence de son objet (pour les réponses).
        let mut parent_ref: HashMap<String, (ObjectId, Object)> = HashMap::new();
        for (i, o) in plan.originals.iter().enumerate() {
            let gone = match o.role {
                Role::Editable(_) => !by_source.contains_key(&(i as u32)),
                Role::Popup => o.popup_parent.map(|p| removed.contains(&p)).unwrap_or(false) || o.oid.map(|id| removed.contains(&id)).unwrap_or(false),
                Role::Reply => o.irt.map(|p| removed.contains(&p)).unwrap_or(false),
                Role::Other => false,
            };
            if gone {
                continue;
            }
            if let (Role::Editable(_), Some(a)) = (o.role, by_source.get(&(i as u32))) {
                let ap_ref = inc.new_document.new_object_id();
                if a.geom_dirty {
                    let (d, stream) = annot_dict(a, &plan.geom, plan.page_id, ap_ref, &date);
                    inc.new_document.set_object(ap_ref, Object::Stream(stream));
                    match o.oid {
                        // Même numéro d'objet : les réponses existantes restent rattachées.
                        Some(id) => {
                            inc.new_document.set_object(id, Object::Dictionary(d));
                            items.push(Object::Reference(id));
                        }
                        None => {
                            let id = inc.new_document.add_object(Object::Dictionary(d));
                            items.push(Object::Reference(id));
                        }
                    }
                } else {
                    if a.meta_dirty {
                        if let Some(id) = o.oid {
                            let _ = inc.opt_clone_object_to_new_document(id);
                            if let Ok(d) = inc.new_document.get_dictionary_mut(id) {
                                if a.contents.is_empty() {
                                    d.remove(b"Contents");
                                } else {
                                    d.set("Contents", text_object(&a.contents));
                                }
                                d.set("M", Object::string_literal(date.as_str()));
                            }
                        }
                    }
                    items.push(o.item.clone());
                }
                if let Some(id) = o.oid {
                    parent_ref.insert(a.id.clone(), (id, o.rect.clone()));
                }
                continue;
            }
            items.push(o.item.clone());
        }

        // Nouvelles annotations.
        for a in model.iter().filter(|a| a.source.is_none()) {
            let ap_ref = inc.new_document.new_object_id();
            let (d, stream) = annot_dict(a, &plan.geom, plan.page_id, ap_ref, &date);
            inc.new_document.set_object(ap_ref, Object::Stream(stream));
            let rect = d.get(b"Rect").cloned().unwrap_or(Object::Null);
            let id = inc.new_document.add_object(Object::Dictionary(d));
            items.push(Object::Reference(id));
            parent_ref.insert(a.id.clone(), (id, rect));
        }

        // Nouvelles réponses et changements d'état « résolu ».
        for a in &model {
            let Some((pid, rect)) = parent_ref.get(&a.id).cloned() else { continue };
            for r in a.replies.iter().filter(|r| !r.saved) {
                let id = inc.new_document.add_object(Object::Dictionary(reply_dict(pid, plan.page_id, &rect, &r.author, &r.text, None, &date)));
                items.push(Object::Reference(id));
            }
            if a.resolved != a.saved_resolved {
                let (state, label) = if a.resolved { ("Completed", "Résolu") } else { ("None", "Rouvert") };
                let id = inc.new_document.add_object(Object::Dictionary(reply_dict(pid, plan.page_id, &rect, &a.author, label, Some(state), &date)));
                items.push(Object::Reference(id));
            }
        }

        inc.opt_clone_object_to_new_document(plan.page_id).map_err(|e| e.to_string())?;
        let page = inc.new_document.get_dictionary_mut(plan.page_id).map_err(|e| e.to_string())?;
        page.set("Annots", Object::Array(items));
    }

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| format!("écriture du PDF : {e}"))?;
    Ok(out)
}
