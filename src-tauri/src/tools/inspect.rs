//! Lecture rapide d'un PDF pour les outils : pages, taille, signets.

use std::collections::HashMap;
use std::path::Path;

use lopdf::{Document, Object, ObjectId};
use serde::Serialize;

use crate::annot::pdf::{deref, get, text};

/// Entrée de la table des matières.
#[derive(Debug, Clone)]
pub struct OutlineItem {
    pub title: String,
    pub depth: u32,
    pub page: Option<ObjectId>,
    /// Enfants (pour la recopie dans une fusion).
    pub children: Vec<OutlineItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mark {
    pub title: String,
    /// Index de page (0).
    pub page: u32,
    pub depth: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub pages: u32,
    pub encrypted: bool,
    pub marks: Vec<Mark>,
}

/// Destinations nommées (catalogue /Dests et arbre /Names /Dests).
fn named_dests(doc: &Document) -> HashMap<Vec<u8>, Object> {
    let mut out = HashMap::new();
    let Ok(cat) = doc.catalog() else { return out };
    if let Some(d) = get(doc, cat, b"Dests").and_then(|o| o.as_dict().ok()) {
        for (k, v) in d.iter() {
            out.insert(k.clone(), deref(doc, v).clone());
        }
    }
    fn walk(doc: &Document, node: &Object, out: &mut HashMap<Vec<u8>, Object>, depth: u32) {
        let Ok(d) = deref(doc, node).as_dict() else { return };
        if depth > 32 {
            return;
        }
        if let Some(names) = get(doc, d, b"Names").and_then(|o| o.as_array().ok()) {
            for pair in names.chunks(2) {
                if let [Object::String(k, _), v] = pair {
                    out.insert(k.clone(), deref(doc, v).clone());
                }
            }
        }
        if let Some(kids) = get(doc, d, b"Kids").and_then(|o| o.as_array().ok()) {
            for k in kids {
                walk(doc, k, out, depth + 1);
            }
        }
    }
    if let Some(tree) = get(doc, cat, b"Names").and_then(|o| o.as_dict().ok()).and_then(|n| n.get(b"Dests").ok()) {
        walk(doc, tree, &mut out, 0);
    }
    out
}

/// Page visée par une destination (tableau, nom, ou dictionnaire /D).
fn dest_page(doc: &Document, dest: &Object, names: &HashMap<Vec<u8>, Object>) -> Option<ObjectId> {
    match deref(doc, dest) {
        Object::Array(a) => a.first().and_then(|p| p.as_reference().ok()),
        Object::String(n, _) | Object::Name(n) => names.get(n).and_then(|d| dest_page(doc, d, &HashMap::new())),
        Object::Dictionary(d) => d.get(b"D").ok().and_then(|x| dest_page(doc, x, names)),
        _ => None,
    }
}

/// Table des matières complète (arbre).
pub fn outline(doc: &Document) -> Vec<OutlineItem> {
    let names = named_dests(doc);
    let Some(first) = doc
        .catalog()
        .ok()
        .and_then(|c| get(doc, c, b"Outlines"))
        .and_then(|o| o.as_dict().ok())
        .and_then(|o| o.get(b"First").ok())
        .cloned()
    else {
        return Vec::new();
    };
    let mut budget = 5_000;
    fn level(doc: &Document, first: Object, depth: u32, names: &HashMap<Vec<u8>, Object>, budget: &mut i32) -> Vec<OutlineItem> {
        let mut out = Vec::new();
        let mut cur = Some(first);
        let mut seen = std::collections::HashSet::new();
        while let Some(o) = cur {
            *budget -= 1;
            if *budget < 0 || depth > 12 {
                break;
            }
            if let Ok(id) = o.as_reference() {
                if !seen.insert(id) {
                    break;
                }
            }
            let Ok(d) = deref(doc, &o).as_dict() else { break };
            let title = d.get(b"Title").map(|t| text(deref(doc, t))).unwrap_or_default().trim().to_string();
            let page = d
                .get(b"Dest")
                .ok()
                .and_then(|x| dest_page(doc, x, names))
                .or_else(|| {
                    let a = get(doc, d, b"A")?.as_dict().ok()?;
                    (a.get(b"S").and_then(|s| s.as_name()).ok() == Some(b"GoTo".as_slice())).then(|| a.get(b"D").ok().and_then(|x| dest_page(doc, x, names)))?
                });
            let children = d.get(b"First").ok().cloned().map(|f| level(doc, f, depth + 1, names, budget)).unwrap_or_default();
            out.push(OutlineItem { title, depth, page, children });
            cur = d.get(b"Next").ok().cloned();
        }
        out
    }
    level(doc, first, 0, &names, &mut budget)
}

pub fn inspect(path: &Path) -> Result<FileInfo, String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let doc = Document::load(path).map_err(|e| format!("« {name} » est illisible ({e})."))?;
    let encrypted = doc.is_encrypted() || doc.trailer.has(b"Encrypt");
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let index: HashMap<ObjectId, u32> = pages.iter().enumerate().map(|(i, p)| (*p, i as u32)).collect();
    let mut marks = Vec::new();
    fn flat(items: &[OutlineItem], index: &HashMap<ObjectId, u32>, out: &mut Vec<Mark>) {
        for it in items {
            if let Some(p) = it.page.and_then(|p| index.get(&p)) {
                out.push(Mark { title: it.title.clone(), page: *p, depth: it.depth });
            }
            flat(&it.children, index, out);
        }
    }
    flat(&outline(&doc), &index, &mut marks);
    Ok(FileInfo { path: path.to_string_lossy().into(), name, size, pages: pages.len() as u32, encrypted, marks })
}
