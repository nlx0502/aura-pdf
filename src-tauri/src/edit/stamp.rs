//! En-têtes et pieds de page, numérotation et filigrane.
//!
//! Chaque ajout est un flux de contenu supplémentaire, marqué comme artefact de
//! pagination (`/Artifact … BDC`) pour les lecteurs d'écran ; le contenu
//! d'origine est encadré par `q … Q` et n'est pas modifié.

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

use super::Plan;
use crate::annot::ap::{display_matrix, n};
use crate::annot::model::rgb;
use crate::annot::pdf::{deref, page_geom};
use crate::annot::text::{pdf_string, text_width};

const FONT_RES: &str = "AuraHelv";
const FONT_BOLD_RES: &str = "AuraHelvB";
const GS_RES: &str = "AuraWmGS";
/// Distance au bord des en-têtes, pieds de page et numéros (points).
const EDGE: f32 = 28.0;
const SIDE: f32 = 40.0;

/// Numéro affiché sur la page `i` (index 0) et dernier numéro, ou `None` pour
/// une page de garde non numérotée. `start` est le numéro de la première page numérotée.
pub fn page_number(i: u32, pages: u32, start: u32, skip_first: bool) -> Option<(u32, u32)> {
    let first = if skip_first { 1 } else { 0 };
    if i < first {
        return None;
    }
    Some((start + i - first, start + pages.saturating_sub(first + 1)))
}

/// Remplace `{n}` et `{total}`.
pub fn fill(template: &str, n: u32, total: u32) -> String {
    template.replace("{n}", &n.to_string()).replace("{total}", &total.to_string())
}

struct Shared {
    font: ObjectId,
    bold: ObjectId,
    gs: Option<ObjectId>,
    open: ObjectId,
}

pub fn apply(doc: &mut Document, pages: &[ObjectId], plan: &Plan) -> Result<(), String> {
    let header = plan.header.as_ref().filter(|h| !h.header.trim().is_empty() || !h.footer.trim().is_empty());
    let numbering = plan.numbering.as_ref().filter(|n| !n.format.trim().is_empty());
    let watermark = plan.watermark.as_ref().filter(|w| !w.text.trim().is_empty());
    if header.is_none() && numbering.is_none() && watermark.is_none() {
        return Ok(());
    }
    let encoding = dictionary! { "Type" => "Encoding", "BaseEncoding" => "WinAnsiEncoding" };
    let shared = Shared {
        font: doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => encoding.clone() }),
        bold: doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold", "Encoding" => encoding }),
        gs: watermark.map(|w| {
            let a = w.opacity.clamp(0.02, 1.0);
            doc.add_object(dictionary! { "Type" => "ExtGState", "ca" => Object::Real(a), "CA" => Object::Real(a) })
        }),
        open: doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec())),
    };

    let total = pages.len() as u32;
    for (i, &page) in pages.iter().enumerate() {
        let Ok(d) = doc.get_dictionary(page) else { continue };
        let g = page_geom(doc, d);
        let m = display_matrix(&g);
        let (w, h) = if g.rot % 2 == 1 { (g.y1 - g.y0, g.x1 - g.x0) } else { (g.x1 - g.x0, g.y1 - g.y0) };
        let mut bytes: Vec<u8> = b"Q\n".to_vec();

        if let Some(wm) = watermark {
            let text = wm.text.trim();
            let width1 = text_width(text, 1.0, true, 0.0).max(0.01);
            let a = wm.angle.to_radians();
            // Taille : le texte occupe environ 75 % de la diagonale disponible.
            let span = (w * a.cos().abs() + h * a.sin().abs()).min((w * w + h * h).sqrt());
            let size = (span * 0.75 / width1).clamp(12.0, 160.0);
            let tw = width1 * size;
            let cap = size * 0.72;
            // Point de départ de la ligne de base pour centrer le texte.
            let (dx, dy) = (a.cos(), -a.sin());
            let (ux, uy) = (-a.sin(), -a.cos());
            let x = w / 2.0 - dx * tw / 2.0 - ux * cap / 2.0;
            let y = h / 2.0 - dy * tw / 2.0 - uy * cap / 2.0;
            let (r, gg, b) = rgb(&wm.color);
            bytes.extend_from_slice(
                format!(
                    "q /{GS_RES} gs /Artifact <</Type /Pagination /Subtype /Watermark>> BDC\nBT /{FONT_BOLD_RES} {} Tf {} {} {} rg {} Tm ",
                    n(size),
                    n(r),
                    n(gg),
                    n(b),
                    text_matrix(&m, x, y, (dx, dy), (ux, uy))
                )
                .as_bytes(),
            );
            bytes.extend_from_slice(&pdf_string(text));
            bytes.extend_from_slice(b" Tj ET EMC Q\n");
        }

        let mut lines: Vec<(String, f32, f32, f32, String, &'static str)> = Vec::new();
        if let Some(hf) = header {
            for (txt, top) in [(&hf.header, true), (&hf.footer, false)] {
                let t = fill(txt.trim(), i as u32 + 1, total);
                if t.is_empty() {
                    continue;
                }
                let tw = text_width(&t, hf.size, false, 0.0);
                let y = if top { EDGE + hf.size * 0.75 } else { h - EDGE };
                lines.push((t, (w - tw) / 2.0, y, hf.size, hf.color.clone(), if top { "Header" } else { "Footer" }));
            }
        }
        if let Some((num, last)) = numbering.and_then(|nb| page_number(i as u32, total, nb.start, nb.skip_first)) {
            let nb = numbering.unwrap();
            let t = fill(&nb.format, num, last);
            let tw = text_width(&t, nb.size, false, 0.0);
            let pos = nb.position.as_bytes();
            let y = if pos.first() == Some(&b't') { EDGE + nb.size * 0.75 } else { h - EDGE };
            let x = match pos.get(1) {
                Some(b'l') => SIDE,
                Some(b'r') => w - SIDE - tw,
                _ => (w - tw) / 2.0,
            };
            lines.push((t, x, y, nb.size, nb.color.clone(), "Header"));
        }

        for (t, x, y, size, color, kind) in lines {
            let (r, gg, b) = rgb(&color);
            let sub = if kind == "Footer" { "Footer" } else { "Header" };
            bytes.extend_from_slice(
                format!(
                    "q /Artifact <</Type /Pagination /Subtype /{sub}>> BDC\nBT /{FONT_RES} {} Tf {} {} {} rg {} Tm ",
                    n(size),
                    n(r),
                    n(gg),
                    n(b),
                    text_matrix(&m, x, y, (1.0, 0.0), (0.0, -1.0))
                )
                .as_bytes(),
            );
            bytes.extend_from_slice(&pdf_string(&t));
            bytes.extend_from_slice(b" Tj ET EMC Q\n");
        }

        let close = doc.add_object(Stream::new(Dictionary::new(), bytes));
        wrap_contents(doc, page, shared.open, close);
        add_resources(doc, page, &shared);
    }
    Ok(())
}

/// Matrice de texte : point (x, y) et directions (ligne de base, haut des
/// glyphes) exprimés dans le repère d'affichage, convertis en espace utilisateur.
fn text_matrix(m: &[f32; 6], x: f32, y: f32, dir: (f32, f32), up: (f32, f32)) -> String {
    let lin = |vx: f32, vy: f32| (m[0] * vx + m[2] * vy, m[1] * vx + m[3] * vy);
    let (a, b) = lin(dir.0, dir.1);
    let (c, d) = lin(up.0, up.1);
    let (e, f) = (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
    format!("{} {} {} {} {} {}", n(a), n(b), n(c), n(d), n(e), n(f))
}

/// Encadre le contenu d'origine : `[q, contenu…, Q + ajouts]`.
pub fn wrap_contents(doc: &mut Document, page: ObjectId, open: ObjectId, close: ObjectId) {
    let current = doc.get_dictionary(page).ok().and_then(|d| d.get(b"Contents").ok().cloned());
    let mut arr = vec![Object::Reference(open)];
    match current {
        Some(Object::Array(a)) => arr.extend(a),
        Some(Object::Reference(r)) => match doc.get_object(r) {
            // Tableau indirect de flux.
            Ok(Object::Array(a)) => arr.extend(a.clone()),
            _ => arr.push(Object::Reference(r)),
        },
        _ => {}
    }
    arr.push(Object::Reference(close));
    if let Ok(d) = doc.get_dictionary_mut(page) {
        d.set("Contents", arr);
    }
}

/// Copie propre à la page de ses ressources, complétée par nos polices et états graphiques.
fn add_resources(doc: &mut Document, page: ObjectId, s: &Shared) {
    let mut res = own_resources(doc, page);
    let mut fonts = sub_dict(doc, &res, b"Font");
    fonts.set(FONT_RES, s.font);
    fonts.set(FONT_BOLD_RES, s.bold);
    res.set("Font", fonts);
    if let Some(gs) = s.gs {
        let mut g = sub_dict(doc, &res, b"ExtGState");
        g.set(GS_RES, gs);
        res.set("ExtGState", g);
    }
    if let Ok(d) = doc.get_dictionary_mut(page) {
        d.set("Resources", res);
    }
}

/// Dictionnaire de ressources de la page, résolu et copié (modifiable sans
/// toucher aux autres pages qui le partagent).
pub fn own_resources(doc: &Document, page: ObjectId) -> Dictionary {
    doc.get_dictionary(page)
        .ok()
        .and_then(|d| crate::annot::pdf::inherited(doc, d, b"Resources"))
        .and_then(|r| r.as_dict().ok())
        .cloned()
        .unwrap_or_default()
}

/// Sous-dictionnaire (Font, XObject…) résolu et copié.
pub fn sub_dict(doc: &Document, res: &Dictionary, key: &[u8]) -> Dictionary {
    res.get(key).ok().and_then(|o| deref(doc, o).as_dict().ok()).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modele_de_numerotation() {
        assert_eq!(fill("Page {n} sur {total}", 3, 12), "Page 3 sur 12");
        assert_eq!(fill("{n} / {total}", 1, 2), "1 / 2");
    }

    #[test]
    fn page_de_garde() {
        assert_eq!(page_number(0, 10, 1, false), Some((1, 10)));
        assert_eq!(page_number(0, 10, 2, true), None);
        assert_eq!(page_number(1, 10, 2, true), Some((2, 10)));
        // Page de garde exclue du décompte : la page 2 porte le numéro 1.
        assert_eq!(page_number(1, 10, 1, true), Some((1, 9)));
    }

    #[test]
    fn matrice_sans_rotation() {
        let g = crate::annot::geom::PageGeom::new(0.0, 0.0, 600.0, 800.0, 0);
        let m = display_matrix(&g);
        // Texte horizontal en haut à gauche : origine (40, 800 - 30) en espace utilisateur.
        assert_eq!(text_matrix(&m, 40.0, 30.0, (1.0, 0.0), (0.0, -1.0)), "1 0 0 1 40 770");
    }
}
