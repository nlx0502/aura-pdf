//! Arbre des pages : réorganisation, rotation, pages vierges, import depuis un
//! autre fichier, rognage et nettoyage des références aux pages supprimées.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

use super::{Margins, PageSrc, Plan};
use crate::annot::pdf::{deref, get, inherited, num, page_geom};

/// Attributs de page hérités qu'une page doit porter elle-même une fois sortie
/// de son nœud d'origine.
const INHERITED: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// Copie dans la page les attributs qu'elle hérite de ses ancêtres.
pub fn materialize(doc: &mut Document, page: ObjectId) {
    let mut found: Vec<(&[u8], Object)> = Vec::new();
    if let Ok(d) = doc.get_dictionary(page) {
        for key in INHERITED {
            if d.has(key) {
                continue;
            }
            // `inherited` suit les références : on recopie la valeur résolue,
            // sauf pour les ressources qu'on garde partagées par référence.
            let mut cur = d;
            let mut value: Option<Object> = None;
            for _ in 0..32 {
                let Some(parent) = get(doc, cur, b"Parent").and_then(|p| p.as_dict().ok()) else { break };
                if let Ok(v) = parent.get(key) {
                    value = Some(v.clone());
                    break;
                }
                cur = parent;
            }
            if let Some(v) = value {
                found.push((key, v));
            }
        }
    }
    if let Ok(d) = doc.get_dictionary_mut(page) {
        for (k, v) in found {
            d.set(k, v);
        }
        if !d.has(b"MediaBox") {
            d.set("MediaBox", vec![0.into(), 0.into(), 612.into(), 792.into()]);
        }
    }
}

/// Construit la liste finale des pages et remplace l'arbre des pages par un
/// nœud unique. Renvoie les identifiants des pages, dans l'ordre du plan.
pub fn assemble(doc: &mut Document, orig: &[ObjectId], plan: &Plan) -> Result<Vec<ObjectId>, String> {
    let root_pages = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Pages").ok())
        .and_then(|p| p.as_reference().ok())
        .ok_or("arbre des pages introuvable")?;

    for &id in orig {
        materialize(doc, id);
    }

    let mut used: HashSet<ObjectId> = HashSet::new();
    let mut imported: HashMap<String, (Document, Vec<ObjectId>)> = HashMap::new();
    let mut ids = Vec::with_capacity(plan.pages.len());
    for p in &plan.pages {
        let id = match &p.src {
            PageSrc::Orig { index } => {
                let id = orig[*index as usize];
                if used.insert(id) {
                    id
                } else {
                    // Même page placée deux fois : copie indépendante.
                    duplicate_page(doc, id)
                }
            }
            PageSrc::Blank { w, h } => blank_page(doc, *w, *h),
            PageSrc::File { path, index } => {
                if !imported.contains_key(path) {
                    let other = Document::load(Path::new(path)).map_err(|e| format!("Impossible de lire « {path} » ({e})."))?;
                    if other.is_encrypted() {
                        return Err(format!("« {path} » est protégé par mot de passe."));
                    }
                    let list: Vec<ObjectId> = other.get_pages().values().copied().collect();
                    imported.insert(path.clone(), (other, list));
                }
                let (other, list) = imported.get(path).unwrap();
                let src = *list.get(*index as usize).ok_or_else(|| format!("Page {} introuvable dans « {path} ».", index + 1))?;
                import_page(doc, other, src)
            }
        };
        if p.rot.rem_euclid(4) != 0 {
            let cur = doc.get_dictionary(id).ok().and_then(|d| d.get(b"Rotate").ok()).and_then(|o| o.as_i64().ok()).unwrap_or(0);
            let next = (cur + 90 * p.rot as i64).rem_euclid(360);
            if let Ok(d) = doc.get_dictionary_mut(id) {
                d.set("Rotate", next);
            }
        }
        if let Ok(d) = doc.get_dictionary_mut(id) {
            d.set("Parent", root_pages);
        }
        ids.push(id);
    }

    let kids: Vec<Object> = ids.iter().map(|&i| Object::Reference(i)).collect();
    if let Ok(d) = doc.get_dictionary_mut(root_pages) {
        d.set("Kids", kids);
        d.set("Count", ids.len() as i64);
        // Les attributs hérités sont désormais portés par chaque page.
        for key in INHERITED {
            d.remove(key);
        }
    }

    let removed: HashSet<ObjectId> = orig.iter().copied().filter(|i| !used.contains(i)).collect();
    let order_changed = removed.len() + ids.len() != orig.len() || ids.iter().zip(orig).any(|(a, b)| a != b);
    if !removed.is_empty() {
        forget_pages(doc, &removed, &ids);
    }
    if order_changed || !removed.is_empty() {
        // Les étiquettes de pages (i, ii, 1, 2…) ne correspondent plus.
        if let Ok(c) = doc.catalog_mut() {
            c.remove(b"PageLabels");
        }
    }
    Ok(ids)
}

fn blank_page(doc: &mut Document, w: f32, h: f32) -> ObjectId {
    let content = doc.add_object(Stream::new(Dictionary::new(), Vec::new()));
    doc.add_object(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), Object::Real(w.max(36.0)), Object::Real(h.max(36.0))],
        "Resources" => Dictionary::new(),
        "Contents" => content,
    })
}

fn duplicate_page(doc: &mut Document, id: ObjectId) -> ObjectId {
    let mut d = doc.get_dictionary(id).cloned().unwrap_or_default();
    // Les annotations appartiennent à une page : la copie n'en garde pas.
    d.remove(b"Annots");
    d.remove(b"StructParents");
    d.remove(b"B");
    doc.add_object(d)
}

/// Copie une page (et tout ce qu'elle référence) d'un autre document.
pub fn import_page(dst: &mut Document, src: &Document, page: ObjectId) -> ObjectId {
    let mut page_dict = src.get_dictionary(page).cloned().unwrap_or_default();
    for key in INHERITED {
        if !page_dict.has(key) {
            if let Some(v) = inherited(src, &page_dict, key) {
                page_dict.set(key, v.clone());
            }
        }
    }
    for key in [b"Parent".as_slice(), b"StructParents", b"B", b"Annots"] {
        page_dict.remove(key);
    }
    let mut map: HashMap<ObjectId, ObjectId> = HashMap::new();
    let new_id = dst.new_object_id();
    map.insert(page, new_id);
    let copied = copy_object(dst, src, &Object::Dictionary(page_dict), &mut map, 0);
    dst.objects.insert(new_id, copied);

    // Annotations (hors widgets de formulaire, qui dépendent de l'AcroForm source).
    if let Some(annots) = src.get_dictionary(page).ok().and_then(|d| get(src, d, b"Annots")).and_then(|a| a.as_array().ok()) {
        let mut out = Vec::new();
        for a in annots {
            let Ok(ad) = deref(src, a).as_dict() else { continue };
            if matches!(ad.get(b"Subtype").and_then(|s| s.as_name()), Ok(b"Widget") | Ok(b"Popup")) {
                continue;
            }
            let mut ad = ad.clone();
            ad.remove(b"Popup");
            ad.remove(b"IRT");
            ad.set("P", Object::Reference(new_id));
            let c = copy_object(dst, src, &Object::Dictionary(ad), &mut map, 0);
            out.push(Object::Reference(dst.add_object(c)));
        }
        if !out.is_empty() {
            if let Ok(d) = dst.get_dictionary_mut(new_id) {
                d.set("Annots", out);
            }
        }
    }
    new_id
}

fn copy_object(dst: &mut Document, src: &Document, o: &Object, map: &mut HashMap<ObjectId, ObjectId>, depth: u32) -> Object {
    if depth > 64 {
        return Object::Null;
    }
    match o {
        Object::Reference(id) => {
            if let Some(n) = map.get(id) {
                return Object::Reference(*n);
            }
            let Ok(target) = src.get_object(*id) else { return Object::Null };
            // Une autre page (ou l'arbre) de la source n'est pas importée.
            if let Ok(d) = target.as_dict() {
                if matches!(d.get(b"Type").and_then(|t| t.as_name()), Ok(b"Page") | Ok(b"Pages")) {
                    return Object::Null;
                }
            }
            let new_id = dst.new_object_id();
            map.insert(*id, new_id);
            let copied = copy_object(dst, src, target, map, depth + 1);
            dst.objects.insert(new_id, copied);
            Object::Reference(new_id)
        }
        Object::Array(a) => Object::Array(a.iter().map(|x| copy_object(dst, src, x, map, depth + 1)).collect()),
        Object::Dictionary(d) => Object::Dictionary(copy_dict(dst, src, d, map, depth)),
        Object::Stream(s) => {
            let dict = copy_dict(dst, src, &s.dict, map, depth);
            let mut st = Stream::new(dict, s.content.clone());
            st.allows_compression = s.allows_compression;
            Object::Stream(st)
        }
        other => other.clone(),
    }
}

fn copy_dict(dst: &mut Document, src: &Document, d: &Dictionary, map: &mut HashMap<ObjectId, ObjectId>, depth: u32) -> Dictionary {
    let mut out = Dictionary::new();
    for (k, v) in d.iter() {
        if k == b"Parent" && d.get(b"Type").and_then(|t| t.as_name()).ok() != Some(b"Page".as_slice()) {
            // Parent d'un champ ou d'un signet : hors de la page importée.
            continue;
        }
        out.set(k.clone(), copy_object(dst, src, v, map, depth + 1));
    }
    out
}

/// Remplace par `null` toute référence aux pages supprimées (signets,
/// destinations, structure…) et retire leurs champs de formulaire.
fn forget_pages(doc: &mut Document, removed: &HashSet<ObjectId>, kept: &[ObjectId]) {
    // Annotations des pages supprimées.
    let mut dead_annots: HashSet<ObjectId> = HashSet::new();
    for id in removed {
        if let Ok(d) = doc.get_dictionary(*id) {
            if let Some(a) = get(doc, d, b"Annots").and_then(|a| a.as_array().ok()) {
                dead_annots.extend(a.iter().filter_map(|o| o.as_reference().ok()));
            }
        }
    }
    // Les pages conservées peuvent partager une annotation (fichiers mal formés) : on la garde.
    for id in kept {
        if let Ok(d) = doc.get_dictionary(*id) {
            if let Some(a) = get(doc, d, b"Annots").and_then(|a| a.as_array().ok()) {
                for o in a {
                    if let Ok(r) = o.as_reference() {
                        dead_annots.remove(&r);
                    }
                }
            }
        }
    }

    // Champs de formulaire dont tous les widgets ont disparu.
    if !dead_annots.is_empty() {
        let fields_ref = doc
            .catalog()
            .ok()
            .and_then(|c| c.get(b"AcroForm").ok().cloned())
            .and_then(|a| match a {
                Object::Reference(r) => Some(r),
                _ => None,
            });
        let mut acro = match fields_ref {
            Some(r) => doc.get_dictionary(r).ok().cloned(),
            None => doc.catalog().ok().and_then(|c| c.get(b"AcroForm").ok()).and_then(|a| a.as_dict().ok()).cloned(),
        };
        if let Some(acro_d) = acro.as_mut() {
            if let Some(fields) = get(doc, acro_d, b"Fields").and_then(|f| f.as_array().ok()).cloned() {
                let kept_fields = prune_fields(doc, &fields, &dead_annots, 0);
                acro_d.set("Fields", kept_fields);
            }
            match fields_ref {
                Some(r) => {
                    doc.objects.insert(r, Object::Dictionary(acro.unwrap()));
                }
                None => {
                    if let Ok(c) = doc.catalog_mut() {
                        c.set("AcroForm", acro.unwrap());
                    }
                }
            }
        }
    }

    let skip: HashSet<ObjectId> = removed.iter().chain(dead_annots.iter()).copied().collect();
    let keys: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for k in keys {
        if skip.contains(&k) {
            continue;
        }
        if let Some(o) = doc.objects.get_mut(&k) {
            null_refs(o, removed, 0);
        }
    }
    let mut trailer = std::mem::take(&mut doc.trailer);
    for (_, v) in trailer.iter_mut() {
        null_refs(v, removed, 0);
    }
    doc.trailer = trailer;
}

fn prune_fields(doc: &mut Document, fields: &[Object], dead: &HashSet<ObjectId>, depth: u32) -> Vec<Object> {
    let mut out = Vec::new();
    for f in fields {
        let Ok(id) = f.as_reference() else {
            out.push(f.clone());
            continue;
        };
        if dead.contains(&id) {
            continue;
        }
        let kids = doc.get_dictionary(id).ok().and_then(|d| get(doc, d, b"Kids")).and_then(|k| k.as_array().ok()).cloned();
        match kids {
            Some(k) if depth < 16 => {
                let kept = prune_fields(doc, &k, dead, depth + 1);
                if kept.is_empty() {
                    continue;
                }
                if let Ok(d) = doc.get_dictionary_mut(id) {
                    d.set("Kids", kept);
                }
                out.push(f.clone());
            }
            _ => out.push(f.clone()),
        }
    }
    out
}

fn null_refs(o: &mut Object, removed: &HashSet<ObjectId>, depth: u32) {
    if depth > 64 {
        return;
    }
    match o {
        Object::Reference(r) if removed.contains(r) => *o = Object::Null,
        Object::Array(a) => a.iter_mut().for_each(|x| null_refs(x, removed, depth + 1)),
        Object::Dictionary(d) => d.iter_mut().for_each(|(_, v)| null_refs(v, removed, depth + 1)),
        Object::Stream(s) => s.dict.iter_mut().for_each(|(_, v)| null_refs(v, removed, depth + 1)),
        _ => {}
    }
}

/// Rogne une page : les marges sont exprimées dans le sens d'affichage final.
pub fn crop(doc: &mut Document, page: ObjectId, m: Margins) {
    let Ok(d) = doc.get_dictionary(page) else { return };
    let g = page_geom(doc, d);
    let (w, h) = if g.rot % 2 == 1 { (g.y1 - g.y0, g.x1 - g.x0) } else { (g.x1 - g.x0, g.y1 - g.y0) };
    let (l, r, t, b) = (m.left.max(0.0), m.right.max(0.0), m.top.max(0.0), m.bottom.max(0.0));
    if w - l - r < 36.0 || h - t - b < 36.0 {
        return;
    }
    let disp = crate::annot::geom::Rect { x: l, y: t, w: w - l - r, h: h - t - b };
    let u = g.rect_to_user(&disp);
    let media = d
        .get(b"MediaBox")
        .ok()
        .and_then(|o| deref(doc, o).as_array().ok())
        .and_then(|a| Some([num(deref(doc, a.first()?))?, num(deref(doc, a.get(1)?))?, num(deref(doc, a.get(2)?))?, num(deref(doc, a.get(3)?))?]))
        .unwrap_or([u.x, u.y, u.x + u.w, u.y + u.h]);
    let (mx0, my0, mx1, my1) = (media[0].min(media[2]), media[1].min(media[3]), media[0].max(media[2]), media[1].max(media[3]));
    let x0 = u.x.max(mx0);
    let y0 = u.y.max(my0);
    let x1 = (u.x + u.w).min(mx1);
    let y1 = (u.y + u.h).min(my1);
    if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
        return;
    }
    if let Ok(d) = doc.get_dictionary_mut(page) {
        d.set("CropBox", vec![Object::Real(x0), Object::Real(y0), Object::Real(x1), Object::Real(y1)]);
        // Les autres boîtes ne doivent pas dépasser la nouvelle zone visible.
        for k in [b"TrimBox".as_slice(), b"BleedBox", b"ArtBox"] {
            d.remove(k);
        }
    }
}

/// Supprime les métadonnées et informations cachées : dictionnaire Info, XMP,
/// vignettes, données d'application, textes alternatifs des pages caviardées.
pub fn strip_metadata(doc: &mut Document, redacted: &HashSet<ObjectId>) {
    doc.trailer.remove(b"Info");
    if let Ok(c) = doc.catalog_mut() {
        c.remove(b"Metadata");
        c.remove(b"PieceInfo");
    }
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for id in pages {
        if let Ok(d) = doc.get_dictionary_mut(id) {
            d.remove(b"Thumb");
            d.remove(b"PieceInfo");
            d.remove(b"Metadata");
        }
    }
    if redacted.is_empty() {
        return;
    }
    // Éléments de structure rattachés à une page caviardée : leurs textes de
    // remplacement pourraient citer le contenu supprimé.
    let keys: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for k in keys {
        let Some(Object::Dictionary(d)) = doc.objects.get_mut(&k) else { continue };
        let on_page = matches!(d.get(b"Pg"), Ok(Object::Reference(r)) if redacted.contains(r));
        let is_elem = d.has(b"S") && (d.has(b"K") || d.has(b"P"));
        if on_page && is_elem {
            for key in [b"Alt".as_slice(), b"ActualText", b"E", b"T"] {
                d.remove(key);
            }
        }
    }
}

/// Retire des ressources des pages les XObjects `ids` qu'elles ne dessinent
/// pas (une ressource déclarée mais inutilisée garderait l'original caviardé
/// dans le fichier).
pub fn drop_unused_xobjects(doc: &mut Document, pages: &[ObjectId], ids: &HashSet<ObjectId>) {
    for &page in pages {
        let res = super::stamp::own_resources(doc, page);
        let xobjs = super::stamp::sub_dict(doc, &res, b"XObject");
        let names: Vec<Vec<u8>> = xobjs
            .iter()
            .filter(|(_, v)| matches!(v, Object::Reference(r) if ids.contains(r)))
            .map(|(k, _)| k.clone())
            .collect();
        if names.is_empty() {
            continue;
        }
        let content = String::from_utf8_lossy(&doc.get_page_content(page)).to_string();
        let mut x = xobjs.clone();
        let mut changed = false;
        for n in names {
            let pat = format!(r"/{}\s*Do", regex::escape(&String::from_utf8_lossy(&n)));
            let used = regex::Regex::new(&pat).map(|r| r.is_match(&content)).unwrap_or(true);
            if !used {
                x.remove(&n);
                changed = true;
            }
        }
        if changed {
            let mut r = res;
            r.set("XObject", x);
            if let Ok(d) = doc.get_dictionary_mut(page) {
                d.set("Resources", r);
            }
        }
    }
}
