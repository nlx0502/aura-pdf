//! Aplatissement : les apparences des champs (et de leurs libellés visibles)
//! sont dessinées dans le contenu des pages ; le formulaire est retiré.

use std::collections::HashSet;

use lopdf::{dictionary, Object, ObjectId, Stream};

use super::appearance as ap;
use super::*;

/// Dessine les apparences des champs dans le contenu des pages et retire le
/// formulaire. Le document est réécrit ; un document chiffré le reste.
pub fn flatten(bytes: &[u8], password: Option<&str>) -> Result<Vec<u8>, String> {
    let mut doc = load_mem(bytes, password)?;
    let id = doc.trailer.get(b"ID").ok().cloned();
    let state = doc.encryption_state.take();
    crate::security::strip_xref_streams(&mut doc);

    // Champs remplis sans apparence (formulaires « NeedAppearances ») : on la génère.
    let fields = list(&doc);
    let all = nodes(&doc);
    let mut helv: std::collections::HashMap<String, Object> = Default::default();
    for field in &fields {
        if field.value.is_empty() || !matches!(field.kind.as_str(), "text" | "email" | "date" | "select" | "list") {
            continue;
        }
        let Some(node) = all.iter().find(|n| n.name == field.name && n.caption_of().is_none()) else { continue };
        let shown = field.options.iter().find(|o| o.value == field.value).map(|o| o.label.clone()).unwrap_or_else(|| field.value.clone());
        for w in &node.widgets {
            let Ok(wd) = doc.get_dictionary(*w).cloned() else { continue };
            if wd.has(b"AP") {
                continue;
            }
            let Some(r) = rect_of(&doc, &wd) else { continue };
            let res = field.style.font_res().to_string();
            let font = match helv.get(&res) {
                Some(f) => f.clone(),
                None => {
                    let f = Object::Reference(doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => style::base_font(&res), "Encoding" => "WinAnsiEncoding" }));
                    helv.insert(res.clone(), f.clone());
                    f
                }
            };
            let mut fonts = lopdf::Dictionary::new();
            fonts.set(res.as_bytes().to_vec(), font);
            let content = ap::text(&field.style, r.w, r.h, &shown, "", field.multiline);
            let ap_id = doc.add_object(ap::form(content, r.w, r.h, fonts));
            if let Ok(d) = doc.get_dictionary_mut(*w) {
                d.set("AP", dictionary! { "N" => ap_id });
            }
        }
    }

    let widget_ids: HashSet<ObjectId> = all.into_iter().flat_map(|n| n.widgets).collect();
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for pid in pages {
        let annots: Vec<Object> = doc.get_dictionary(pid).ok().and_then(|p| get(&doc, p, b"Annots")).and_then(|a| a.as_array().ok()).cloned().unwrap_or_default();
        let mut keep = Vec::new();
        let mut draws = String::new();
        let mut xobjects: Vec<(String, ObjectId)> = Vec::new();
        let mut had_widget = false;
        for a in annots {
            let in_tree = a.as_reference().map(|w| widget_ids.contains(&w)).unwrap_or(false);
            let is_widget = in_tree || deref(&doc, &a).as_dict().map(|d| matches!(d.get(b"Subtype"), Ok(Object::Name(n)) if n == b"Widget")).unwrap_or(false);
            if !is_widget {
                keep.push(a);
                continue;
            }
            had_widget = true;
            let Some(w) = deref(&doc, &a).as_dict().ok().cloned() else { continue };
            let hidden = w.get(b"F").and_then(|o| o.as_i64()).map(|f| f & 2 != 0).unwrap_or(false);
            if hidden {
                continue;
            }
            let Some(ap_id) = appearance_of(&doc, &w) else { continue };
            let Some(r) = rect_of(&doc, &w) else { continue };
            let Some((bx, by, bw, bh)) = form_box(&doc, ap_id) else { continue };
            if bw.abs() < 1e-3 || bh.abs() < 1e-3 {
                continue;
            }
            if let Ok(s) = doc.get_object_mut(ap_id).and_then(|o| o.as_stream_mut()) {
                s.dict.set("Type", "XObject");
                s.dict.set("Subtype", "Form");
            }
            let name = format!("AuraFlat{}", xobjects.len() + 1);
            let (sx, sy) = (r.w / bw, r.h / bh);
            draws += &format!("q {sx:.5} 0 0 {sy:.5} {:.3} {:.3} cm /{name} Do Q\n", r.x - bx * sx, r.y - by * sy);
            xobjects.push((name, ap_id));
        }
        if !had_widget {
            continue;
        }
        let pre = doc.add_object(Stream::new(dictionary! {}, b"q\n".to_vec()));
        let post = doc.add_object(Stream::new(dictionary! {}, format!("Q\n{draws}").into_bytes()));
        // Ressources (éventuellement héritées) copiées dans la page.
        let mut res = doc
            .get_dictionary(pid)
            .ok()
            .and_then(|p| crate::annot::pdf::inherited(&doc, p, b"Resources"))
            .and_then(|r| r.as_dict().ok())
            .cloned()
            .unwrap_or_default();
        let mut xo = get(&doc, &res, b"XObject").and_then(|x| x.as_dict().ok()).cloned().unwrap_or_default();
        for (n, id) in &xobjects {
            xo.set(n.as_bytes().to_vec(), *id);
        }
        res.set("XObject", xo);
        let page = doc.get_dictionary_mut(pid).map_err(|e| e.to_string())?;
        let mut contents = vec![Object::Reference(pre)];
        match page.get(b"Contents") {
            Ok(Object::Array(a)) => contents.extend(a.iter().cloned()),
            Ok(o) => contents.push(o.clone()),
            Err(_) => {}
        }
        contents.push(Object::Reference(post));
        page.set("Contents", contents);
        page.set("Resources", res);
        if keep.is_empty() {
            page.remove(b"Annots");
        } else {
            page.set("Annots", keep);
        }
    }
    // Retrait du formulaire.
    let root = doc.trailer.get(b"Root").and_then(|o| o.as_reference()).map_err(|e| e.to_string())?;
    if let Ok(cat) = doc.get_dictionary_mut(root) {
        cat.remove(b"AcroForm");
    }
    doc.prune_objects();
    doc.compress();
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| format!("écriture impossible ({e})"))?;
    match state {
        Some(s) => crate::edit::reencrypt(&out, &s, id),
        None => Ok(out),
    }
}

/// Boîte de l'apparence (BBox transformée par sa /Matrix) : x, y, largeur, hauteur.
fn form_box(doc: &lopdf::Document, id: ObjectId) -> Option<(f32, f32, f32, f32)> {
    let s = doc.get_object(id).ok()?.as_stream().ok()?;
    let b: Vec<f32> = get(doc, &s.dict, b"BBox")?.as_array().ok()?.iter().filter_map(|o| num(deref(doc, o))).collect();
    if b.len() != 4 {
        return None;
    }
    let m: Vec<f32> = get(doc, &s.dict, b"Matrix")
        .and_then(|m| m.as_array().ok())
        .map(|a| a.iter().filter_map(|o| num(deref(doc, o))).collect())
        .filter(|v: &Vec<f32>| v.len() == 6)
        .unwrap_or(vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let pts = [(b[0], b[1]), (b[2], b[1]), (b[0], b[3]), (b[2], b[3])].map(|(x, y)| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]));
    let (x0, x1) = pts.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
    let (y0, y1) = pts.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
    Some((x0, y0, x1 - x0, y1 - y0))
}
