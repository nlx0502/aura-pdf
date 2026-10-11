//! Lecture des annotations existantes d'un document (lopdf).

use std::collections::HashMap;

use lopdf::{Document, Object, ObjectId};

use super::geom::Rect;
use super::model::{hex, Annot, Kind, Reply, STAMPS};
use super::pdf::{classify, deref, get, num, page_annots, page_geom, text, Role};

fn floats(doc: &Document, o: Option<&Object>) -> Vec<f32> {
    o.and_then(|o| o.as_array().ok())
        .map(|a| a.iter().filter_map(|v| num(deref(doc, v))).collect())
        .unwrap_or_default()
}

fn color(doc: &Document, d: &lopdf::Dictionary) -> String {
    let c = floats(doc, get(doc, d, b"C"));
    match c.len() {
        1 => hex(c[0], c[0], c[0]),
        3 => hex(c[0], c[1], c[2]),
        4 => hex((1.0 - c[0]) * (1.0 - c[3]), (1.0 - c[1]) * (1.0 - c[3]), (1.0 - c[2]) * (1.0 - c[3])),
        _ => "#F2B84B".into(),
    }
}

fn border_width(doc: &Document, d: &lopdf::Dictionary) -> f32 {
    if let Some(w) = get(doc, d, b"BS").and_then(|bs| bs.as_dict().ok()).and_then(|bs| get(doc, bs, b"W")).and_then(num) {
        return w;
    }
    let b = floats(doc, get(doc, d, b"Border"));
    if b.len() >= 3 {
        return b[2];
    }
    1.0
}

/// Apparence par défaut (/DA) : police, taille, couleur du texte.
pub struct Da {
    pub font: String,
    pub size: Option<f32>,
    pub color: Option<String>,
}

pub fn parse_da(da: &str) -> Da {
    let t: Vec<&str> = da.split_whitespace().collect();
    let mut out = Da { font: String::new(), size: None, color: None };
    for (i, tok) in t.iter().enumerate() {
        match *tok {
            "Tf" if i >= 2 => {
                out.font = t[i - 2].trim_start_matches('/').to_string();
                out.size = t[i - 1].parse().ok();
            }
            "rg" if i >= 3 => {
                let v: Vec<f32> = t[i - 3..i].iter().filter_map(|s| s.parse().ok()).collect();
                if v.len() == 3 {
                    out.color = Some(hex(v[0], v[1], v[2]));
                }
            }
            "g" if i >= 1 => {
                if let Ok(v) = t[i - 1].parse::<f32>() {
                    out.color = Some(hex(v, v, v));
                }
            }
            _ => {}
        }
    }
    out
}

fn str_of(doc: &Document, d: &lopdf::Dictionary, key: &[u8]) -> String {
    get(doc, d, key).map(text).unwrap_or_default().trim_matches('\0').to_string()
}

/// Lit toutes les annotations modifiables d'un document.
pub fn read(doc: &Document) -> Vec<Annot> {
    let mut out: Vec<Annot> = Vec::new();
    // Objet indirect → position dans `out` (pour rattacher réponses et états).
    let mut by_id: HashMap<ObjectId, usize> = HashMap::new();
    let mut replies: Vec<(ObjectId, Reply, Option<bool>)> = Vec::new();

    for (pno, page_id) in doc.get_pages() {
        let Ok(page) = doc.get_dictionary(page_id) else { continue };
        let g = page_geom(doc, page);
        for (idx, (oid, d)) in page_annots(doc, page).into_iter().enumerate() {
            match classify(doc, d) {
                Role::Reply => {
                    let Some(parent) = d.get(b"IRT").ok().and_then(|o| o.as_reference().ok()) else { continue };
                    let state = get(doc, d, b"State").and_then(|o| o.as_str().ok().or_else(|| o.as_name().ok()).map(|s| s.to_vec()));
                    let is_state = d.has(b"StateModel") || state.is_some();
                    let resolved = state.as_deref().map(|s| s == b"Completed" || s == b"Accepted");
                    let reply = Reply {
                        author: str_of(doc, d, b"T"),
                        text: str_of(doc, d, b"Contents"),
                        date: Some(str_of(doc, d, b"M")).filter(|s| !s.is_empty()),
                        saved: true,
                    };
                    replies.push((parent, reply, if is_state { Some(resolved.unwrap_or(false)) } else { None }));
                }
                Role::Editable(kind) => {
                    let mut rect = get(doc, d, b"Rect").map(|o| floats(doc, Some(o))).filter(|v| v.len() >= 4).map(|v| g.rect_to_display(&Rect::from_corners(v[0], v[1], v[2], v[3])));
                    // /RD : marge entre /Rect et la forme (tampons, rectangles d'autres logiciels).
                    let rd = floats(doc, get(doc, d, b"RD"));
                    if let (Some(r), true) = (rect.as_mut(), rd.len() == 4) {
                        let m = rd.iter().copied().fold(0.0_f32, f32::max);
                        *r = r.inflate(-m);
                    }
                    let nm = str_of(doc, d, b"NM");
                    let mut a = Annot {
                        id: if nm.is_empty() { format!("p{}-{}", pno - 1, idx) } else { nm.clone() },
                        page: pno - 1,
                        kind,
                        color: color(doc, d),
                        opacity: get(doc, d, b"CA").and_then(num).unwrap_or(1.0),
                        width: border_width(doc, d),
                        rect,
                        contents: str_of(doc, d, b"Contents"),
                        author: str_of(doc, d, b"T"),
                        modified: Some(str_of(doc, d, b"M")).filter(|s| !s.is_empty()),
                        source: Some(idx as u32),
                        ..Default::default()
                    };
                    match kind {
                        k if k.is_markup() => {
                            let q = floats(doc, get(doc, d, b"QuadPoints"));
                            for c in q.chunks_exact(8) {
                                let pts: Vec<(f32, f32)> = c.chunks_exact(2).map(|p| g.to_display(p[0], p[1])).collect();
                                let x0 = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min);
                                let x1 = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max);
                                let y0 = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
                                let y1 = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max);
                                a.rects.push(Rect::from_corners(x0, y0, x1, y1));
                            }
                            if a.rects.is_empty() {
                                a.rects.extend(a.rect);
                            }
                        }
                        Kind::Ink => {
                            if let Some(list) = get(doc, d, b"InkList").and_then(|o| o.as_array().ok()) {
                                for stroke in list {
                                    let v = floats(doc, Some(deref(doc, stroke)));
                                    a.ink.push(v.chunks_exact(2).map(|p| {
                                        let (x, y) = g.to_display(p[0], p[1]);
                                        [x, y]
                                    }).collect());
                                }
                            }
                        }
                        Kind::Line | Kind::Arrow => {
                            let l = floats(doc, get(doc, d, b"L"));
                            if l.len() >= 4 {
                                let (x1, y1) = g.to_display(l[0], l[1]);
                                let (x2, y2) = g.to_display(l[2], l[3]);
                                // Flèche au début : on inverse pour la garder à la fin.
                                let starts_with_arrow = get(doc, d, b"LE")
                                    .and_then(|o| o.as_array().ok())
                                    .and_then(|a| a.first())
                                    .map(|e| matches!(e.as_name(), Ok(n) if n.ends_with(b"Arrow")))
                                    .unwrap_or(false);
                                a.line = Some(if starts_with_arrow { [x2, y2, x1, y1] } else { [x1, y1, x2, y2] });
                            }
                        }
                        Kind::Text => {
                            a.text = a.contents.clone();
                            a.contents.clear();
                            let da = parse_da(&str_of(doc, d, b"DA"));
                            a.font_size = da.size.unwrap_or(12.0);
                            let (family, bold, italic) = super::model::font_from_resource(&da.font);
                            a.font = family;
                            a.bold = bold;
                            a.italic = italic;
                            // Couleur du texte : /DA ; bordure : /C si /BS /W > 0 ; fond : /IC.
                            let border = a.color.clone();
                            a.color = da.color.unwrap_or_else(|| "#1A1B1E".into());
                            a.border_color = if a.width > 0.0 && d.has(b"C") { Some(border) } else { None };
                            let ic = floats(doc, get(doc, d, b"IC"));
                            a.fill = (ic.len() == 3).then(|| hex(ic[0], ic[1], ic[2]));
                            a.align = get(doc, d, b"Q").and_then(|o| o.as_i64().ok()).unwrap_or(0).clamp(0, 2) as u8;
                        }
                        Kind::Stamp => {
                            let name = get(doc, d, b"Name").and_then(|o| o.as_name().ok()).map(|n| String::from_utf8_lossy(n).to_string()).unwrap_or_default();
                            a.text = STAMPS.iter().find(|s| s.1 == name).map(|s| s.0.to_string()).unwrap_or_else(|| name.to_uppercase());
                            // Tampons d'autres logiciels (souvent des images) : non redessinables.
                            a.locked = !nm.starts_with("aura-");
                        }
                        _ => {}
                    }
                    if let Some(oid) = oid {
                        by_id.insert(oid, out.len());
                    }
                    out.push(a);
                }
                Role::Popup | Role::Other => {}
            }
        }
    }

    for (parent, reply, state) in replies {
        let Some(&i) = by_id.get(&parent) else { continue };
        match state {
            Some(resolved) => {
                out[i].resolved = resolved;
                out[i].saved_resolved = resolved;
            }
            None => out[i].replies.push(reply),
        }
    }
    out
}
