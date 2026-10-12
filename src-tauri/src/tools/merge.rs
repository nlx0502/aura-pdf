//! Fusion de plusieurs PDF : pages importées dans l'ordre, un signet par
//! fichier (avec ses propres signets dessous), table des matières facultative,
//! uniformisation au format A4 facultative.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use serde::Deserialize;

use super::inspect::{outline, OutlineItem};
use super::{load, write, Job, Output};
use crate::annot::ap::n;
use crate::annot::pdf::{deref, num};
use crate::annot::pdf::text_object;
use crate::annot::text::{pdf_string, text_width};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeOptions {
    /// Un signet par fichier.
    pub bookmarks: bool,
    /// Page de sommaire au début.
    pub toc: bool,
    /// Pages mises au format A4 (orientation conservée).
    pub a4: bool,
}

const A4: (f32, f32) = (595.28, 841.89);

/// Signet à créer.
pub struct NewMark {
    pub title: String,
    pub page: Option<ObjectId>,
    pub children: Vec<NewMark>,
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Document".into())
}

pub fn merge(inputs: &[PathBuf], opts: &MergeOptions, out: &Path, job: &Job) -> Result<Output, String> {
    if inputs.len() < 2 {
        return Err("Choisissez au moins deux fichiers.".into());
    }
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut kids: Vec<ObjectId> = Vec::new();
    let mut marks: Vec<NewMark> = Vec::new();
    let mut firsts: Vec<(String, usize)> = Vec::new();

    for (k, path) in inputs.iter().enumerate() {
        job.check()?;
        job.progress(0, k, inputs.len(), &stem(path));
        let src = load(path)?;
        let src_pages: Vec<ObjectId> = src.get_pages().values().copied().collect();
        let mut map: HashMap<ObjectId, ObjectId> = HashMap::new();
        let start = kids.len();
        for &p in &src_pages {
            let id = crate::edit::pages::import_page(&mut doc, &src, p);
            map.insert(p, id);
            kids.push(id);
        }
        firsts.push((stem(path), start));
        if opts.bookmarks {
            fn convert(items: &[OutlineItem], map: &HashMap<ObjectId, ObjectId>) -> Vec<NewMark> {
                items
                    .iter()
                    .map(|it| NewMark { title: it.title.clone(), page: it.page.and_then(|p| map.get(&p).copied()), children: convert(&it.children, map) })
                    .collect()
            }
            marks.push(NewMark { title: stem(path), page: kids.get(start).copied(), children: convert(&outline(&src), &map) });
        }
    }

    for &id in &kids {
        if let Ok(d) = doc.get_dictionary_mut(id) {
            d.set("Parent", pages_id);
        }
        if opts.a4 {
            to_a4(&mut doc, id);
        }
    }

    // Sommaire : une ou plusieurs pages au début, avec liens vers chaque fichier.
    let mut toc_ids: Vec<ObjectId> = Vec::new();
    if opts.toc {
        toc_ids = toc_pages(&mut doc, pages_id, &firsts, &kids);
        if opts.bookmarks {
            marks.insert(0, NewMark { title: "Sommaire".into(), page: toc_ids.first().copied(), children: vec![] });
        }
    }
    let all: Vec<Object> = toc_ids.iter().chain(kids.iter()).map(|&i| Object::Reference(i)).collect();
    let count = all.len() as i64;
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => all, "Count" => count }));
    let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => pages_id };
    if !marks.is_empty() {
        let root = build_outline(&mut doc, &marks);
        catalog.set("Outlines", root);
        catalog.set("PageMode", "UseOutlines");
    }
    let cat = doc.add_object(catalog);
    doc.trailer.set("Root", cat);
    let info = doc.add_object(dictionary! { "Title" => text_object(&stem(out)), "Producer" => Object::string_literal("Aura PDF") });
    doc.trailer.set("Info", info);
    job.check()?;
    job.progress(1, inputs.len(), inputs.len(), "");
    doc.compress();
    write(&mut doc, out, false)
}

/// Met une page au format A4 (portrait ou paysage selon la page), contenu centré.
fn to_a4(doc: &mut Document, page: ObjectId) {
    let Ok(d) = doc.get_dictionary(page) else { return };
    let rect = |key: &[u8]| -> Option<[f32; 4]> {
        let a = d.get(key).ok().map(|o| deref(doc, o))?.as_array().ok()?;
        let v: Vec<f32> = a.iter().filter_map(|x| num(deref(doc, x))).collect();
        (v.len() == 4).then(|| [v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
    };
    let Some(b) = rect(b"CropBox").or_else(|| rect(b"MediaBox")) else { return };
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    if w < 1.0 || h < 1.0 {
        return;
    }
    let (tw, th) = if w > h { (A4.1, A4.0) } else { A4 };
    let s = (tw / w).min(th / h);
    let tx = (tw - w * s) / 2.0 - b[0] * s;
    let ty = (th - h * s) / 2.0 - b[1] * s;
    if (s - 1.0).abs() < 0.002 && tx.abs() < 0.5 && ty.abs() < 0.5 {
        return;
    }
    let contents = d.get(b"Contents").ok().cloned();
    let annots = d.get(b"Annots").ok().and_then(|a| deref(doc, a).as_array().ok()).cloned().unwrap_or_default();
    let pre = doc.add_object(Stream::new(Dictionary::new(), format!("q {} 0 0 {} {} {} cm\n", n(s), n(s), n(tx), n(ty)).into_bytes()));
    let post = doc.add_object(Stream::new(Dictionary::new(), b"\nQ\n".to_vec()));
    let mut arr = vec![Object::Reference(pre)];
    match contents {
        Some(Object::Array(a)) => arr.extend(a),
        Some(Object::Reference(r)) => match doc.get_object(r) {
            Ok(Object::Array(a)) => arr.extend(a.clone()),
            _ => arr.push(Object::Reference(r)),
        },
        _ => {}
    }
    arr.push(Object::Reference(post));
    // Les annotations suivent la mise à l'échelle.
    for a in annots {
        let Ok(id) = a.as_reference() else { continue };
        if let Ok(ad) = doc.get_dictionary_mut(id) {
            if let Ok(Object::Array(r)) = ad.get(b"Rect").cloned() {
                let v: Vec<f32> = r.iter().filter_map(num).collect();
                if v.len() == 4 {
                    ad.set("Rect", vec![Object::Real(v[0] * s + tx), Object::Real(v[1] * s + ty), Object::Real(v[2] * s + tx), Object::Real(v[3] * s + ty)]);
                }
            }
        }
    }
    if let Ok(d) = doc.get_dictionary_mut(page) {
        d.set("Contents", arr);
        d.set("MediaBox", vec![0.into(), 0.into(), Object::Real(tw), Object::Real(th)]);
        for k in [b"CropBox".as_slice(), b"BleedBox", b"TrimBox", b"ArtBox"] {
            d.remove(k);
        }
    }
}

const TOC_PER_PAGE: usize = 30;

/// Pages de sommaire : nom de chaque fichier et numéro de sa première page, avec lien.
fn toc_pages(doc: &mut Document, parent: ObjectId, firsts: &[(String, usize)], kids: &[ObjectId]) -> Vec<ObjectId> {
    let n_pages = firsts.len().div_ceil(TOC_PER_PAGE).max(1);
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" });
    let bold = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold", "Encoding" => "WinAnsiEncoding" });
    let (w, h) = A4;
    let mut ids = Vec::new();
    for (pi, chunk) in firsts.chunks(TOC_PER_PAGE).enumerate() {
        let mut c: Vec<u8> = Vec::new();
        let mut annots = Vec::new();
        if pi == 0 {
            c.extend_from_slice(format!("BT /F2 22 Tf 0.1 0.1 0.12 rg 64 {} Td ", n(h - 96.0)).as_bytes());
            c.extend_from_slice(&pdf_string("Sommaire"));
            c.extend_from_slice(b" Tj ET\n");
        }
        let top = if pi == 0 { h - 150.0 } else { h - 90.0 };
        for (i, (name, first)) in chunk.iter().enumerate() {
            let y = top - i as f32 * 22.0;
            let num_txt = (first + n_pages + 1).to_string();
            let nw = text_width(&num_txt, 11.0, false, 0.0);
            let mut title = name.clone();
            while text_width(&title, 11.0, false, 0.0) > w - 64.0 - 64.0 - 60.0 && title.chars().count() > 4 {
                title.pop();
                title.pop();
                title.push('…');
            }
            let tw = text_width(&title, 11.0, false, 0.0);
            c.extend_from_slice(format!("BT /F1 11 Tf 0.15 0.15 0.17 rg 64 {} Td ", n(y)).as_bytes());
            c.extend_from_slice(&pdf_string(&title));
            c.extend_from_slice(b" Tj ET\n");
            // Points de conduite.
            c.extend_from_slice(format!("q 0.75 0.75 0.73 RG 0.8 w [1 3] 0 d {} {} m {} {} l S Q\n", n(64.0 + tw + 6.0), n(y + 2.0), n(w - 64.0 - nw - 6.0), n(y + 2.0)).as_bytes());
            c.extend_from_slice(format!("BT /F1 11 Tf {} {} Td ", n(w - 64.0 - nw), n(y)).as_bytes());
            c.extend_from_slice(&pdf_string(&num_txt));
            c.extend_from_slice(b" Tj ET\n");
            if let Some(&target) = kids.get(*first) {
                annots.push(Object::Reference(doc.add_object(dictionary! {
                    "Type" => "Annot", "Subtype" => "Link", "Border" => vec![0.into(), 0.into(), 0.into()],
                    "Rect" => vec![Object::Real(60.0), Object::Real(y - 5.0), Object::Real(w - 60.0), Object::Real(y + 14.0)],
                    "Dest" => vec![Object::Reference(target), "Fit".into()],
                })));
            }
        }
        let mut st = Stream::new(Dictionary::new(), c);
        let _ = st.compress();
        let content = doc.add_object(st);
        let mut page = dictionary! {
            "Type" => "Page", "Parent" => parent,
            "MediaBox" => vec![0.into(), 0.into(), Object::Real(w), Object::Real(h)],
            "Contents" => content,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font, "F2" => bold } },
        };
        if !annots.is_empty() {
            page.set("Annots", annots);
        }
        ids.push(doc.add_object(page));
    }
    ids
}

/// Crée l'arbre des signets et renvoie la racine /Outlines.
pub fn build_outline(doc: &mut Document, marks: &[NewMark]) -> ObjectId {
    let root = doc.new_object_id();
    let (first, last, count) = build_level(doc, marks, root);
    let mut d = dictionary! { "Type" => "Outlines", "Count" => count };
    if let (Some(f), Some(l)) = (first, last) {
        d.set("First", f);
        d.set("Last", l);
    }
    doc.objects.insert(root, Object::Dictionary(d));
    root
}

fn build_level(doc: &mut Document, marks: &[NewMark], parent: ObjectId) -> (Option<ObjectId>, Option<ObjectId>, i64) {
    let ids: Vec<ObjectId> = marks.iter().map(|_| doc.new_object_id()).collect();
    let mut total = 0;
    for (i, m) in marks.iter().enumerate() {
        let mut d = dictionary! { "Title" => text_object(&m.title), "Parent" => parent };
        if let Some(p) = m.page {
            d.set("Dest", vec![Object::Reference(p), "Fit".into()]);
        }
        if i > 0 {
            d.set("Prev", ids[i - 1]);
        }
        if i + 1 < ids.len() {
            d.set("Next", ids[i + 1]);
        }
        if !m.children.is_empty() {
            let (f, l, c) = build_level(doc, &m.children, ids[i]);
            if let (Some(f), Some(l)) = (f, l) {
                d.set("First", f);
                d.set("Last", l);
                // Négatif : signet replié à l'ouverture.
                d.set("Count", -c);
            }
        }
        doc.objects.insert(ids[i], Object::Dictionary(d));
        total += 1;
    }
    (ids.first().copied(), ids.last().copied(), total)
}
