//! Division d'un PDF en plusieurs fichiers (plages de pages, taille maximale,
//! signets). Chaque partie garde ses signets, liens et pièces jointes.

use std::collections::HashSet;
use std::path::Path;

use lopdf::{Document, Object, ObjectId};
use serde::Deserialize;

use super::inspect::{outline, OutlineItem};
use super::merge::{build_outline, NewMark};
use super::{free_path, Job, Output};
use crate::edit::{PageSrc, Plan, PlanPage};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    /// Première et dernière page (index 0, incluses).
    pub start: u32,
    pub end: u32,
    /// Nom du fichier, sans extension.
    pub name: String,
}

pub fn split(path: &Path, parts: &[Part], out_dir: &Path, job: &Job) -> Result<Vec<Output>, String> {
    let doc = super::load(path)?;
    let n = doc.get_pages().len() as u32;
    if parts.is_empty() {
        return Err("Aucune partie à créer.".into());
    }
    if parts.iter().any(|p| p.start > p.end || p.end >= n) {
        return Err(format!("Plages invalides : le document compte {n} pages."));
    }
    let mut outs = Vec::new();
    for (k, part) in parts.iter().enumerate() {
        job.check()?;
        job.progress(0, k, parts.len(), &part.name);
        let mut d = doc.clone();
        let pages: Vec<ObjectId> = d.get_pages().values().copied().collect();
        let keep: HashSet<ObjectId> = pages[part.start as usize..=part.end as usize].iter().copied().collect();
        keep_outline(&mut d, &keep);
        let plan = Plan {
            pages: (part.start..=part.end).map(|i| PlanPage { src: PageSrc::Orig { index: i }, rot: 0, crop: false, redact: vec![], links: vec![] }).collect(),
            header: None,
            numbering: None,
            watermark: None,
            crop: None,
            clean_metadata: false,
        };
        let bytes = crate::edit::build(&mut d, &plan)?;
        let out = free_path(out_dir, &part.name);
        crate::edit::write_atomic(&out, &bytes).map_err(|e| e.message)?;
        outs.push(Output { path: out.to_string_lossy().into(), size: bytes.len() as u64, pages: part.end - part.start + 1 });
    }
    job.progress(1, parts.len(), parts.len(), "");
    Ok(outs)
}

/// Ne garde que les signets qui mènent aux pages conservées (ou à leurs enfants).
fn keep_outline(doc: &mut Document, keep: &HashSet<ObjectId>) {
    let items = outline(doc);
    if items.is_empty() {
        return;
    }
    fn filter(items: &[OutlineItem], keep: &HashSet<ObjectId>) -> Vec<NewMark> {
        items
            .iter()
            .filter_map(|it| {
                let children = filter(&it.children, keep);
                let page = it.page.filter(|p| keep.contains(p));
                (page.is_some() || !children.is_empty()).then(|| NewMark { title: it.title.clone(), page: page.or_else(|| children[0].page), children })
            })
            .collect()
    }
    let marks = filter(&items, keep);
    let root = if marks.is_empty() { None } else { Some(build_outline(doc, &marks)) };
    if let Ok(c) = doc.catalog_mut() {
        match root {
            Some(r) => c.set("Outlines", r),
            None => {
                c.remove(b"Outlines");
                if c.get(b"PageMode").and_then(|m| m.as_name()).ok() == Some(b"UseOutlines".as_slice()) {
                    c.remove(b"PageMode");
                }
            }
        }
    }
}

/// Taille approximative d'un objet une fois écrit.
fn obj_size(o: &Object) -> usize {
    match o {
        Object::Stream(s) => s.content.len() + 40 + s.dict.len() * 14,
        Object::Dictionary(d) => 20 + d.len() * 14,
        Object::Array(a) => 10 + a.len() * 6,
        _ => 12,
    }
}

/// Objets atteints depuis une page (sans remonter à l'arbre ni aux autres pages).
fn reachable(doc: &Document, page: ObjectId, pages: &HashSet<ObjectId>, out: &mut HashSet<ObjectId>) {
    let mut stack = vec![page];
    while let Some(id) = stack.pop() {
        if !out.insert(id) {
            continue;
        }
        let Ok(o) = doc.get_object(id) else { continue };
        let mut refs = Vec::new();
        collect(o, &mut refs, 0);
        for r in refs {
            if (r == page || !pages.contains(&r)) && !out.contains(&r) {
                stack.push(r);
            }
        }
    }
}

fn collect(o: &Object, refs: &mut Vec<ObjectId>, depth: u32) {
    if depth > 32 {
        return;
    }
    match o {
        Object::Reference(r) => refs.push(*r),
        Object::Array(a) => a.iter().for_each(|x| collect(x, refs, depth + 1)),
        Object::Dictionary(d) => d.iter().filter(|(k, _)| k.as_slice() != b"Parent" && k.as_slice() != b"P").for_each(|(_, v)| collect(v, refs, depth + 1)),
        Object::Stream(s) => s.dict.iter().for_each(|(_, v)| collect(v, refs, depth + 1)),
        _ => {}
    }
}

/// Découpage par taille : parties successives ne dépassant pas `max_bytes`
/// (une page plus grosse que la limite forme une partie à elle seule).
pub fn by_size(path: &Path, max_bytes: u64) -> Result<Vec<[u32; 2]>, String> {
    let doc = super::load(path)?;
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let set: HashSet<ObjectId> = pages.iter().copied().collect();
    let base = 4_000usize;
    let mut parts = Vec::new();
    let mut cur: HashSet<ObjectId> = HashSet::new();
    let mut bytes = base;
    let mut start = 0u32;
    for (i, &p) in pages.iter().enumerate() {
        let mut own = HashSet::new();
        reachable(&doc, p, &set, &mut own);
        let added: usize = own.iter().filter(|id| !cur.contains(id)).filter_map(|id| doc.get_object(*id).ok()).map(obj_size).sum();
        if i as u32 > start && (bytes + added) as u64 > max_bytes {
            parts.push([start, i as u32 - 1]);
            start = i as u32;
            cur.clear();
            bytes = base + own.iter().filter_map(|id| doc.get_object(*id).ok()).map(obj_size).sum::<usize>();
        } else {
            bytes += added;
        }
        cur.extend(own);
    }
    parts.push([start, pages.len().saturating_sub(1) as u32]);
    Ok(parts)
}
