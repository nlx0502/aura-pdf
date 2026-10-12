//! PDF recherchable : le texte reconnu est ajouté sous forme de texte
//! invisible (mode de rendu 3) exactement sur les mots de l'image, ce qui
//! permet la recherche, la sélection et la copie. Option : redresser les pages
//! inclinées (rotation du contenu de la page, texte compris).

use lopdf::{dictionary, Document, Object, ObjectId, Stream};

use super::Word;
use crate::annot::geom::PageGeom;
use crate::annot::pdf::{get, page_geom};
use crate::annot::text::{pdf_string, text_width};

/// Résultat de l'OCR d'une page, dans le repère de son image.
pub struct PageOcr {
    pub page: u32,
    pub words: Vec<Word>,
    pub img_w: u32,
    pub img_h: u32,
    /// Inclinaison détectée (degrés).
    pub skew: f32,
}

/// Seuil de redressement (degrés).
const MIN_SKEW: f32 = 0.15;

fn page_text(g: &PageGeom, p: &PageOcr) -> Vec<u8> {
    let dw = if g.rot % 2 == 1 { g.y1 - g.y0 } else { g.x1 - g.x0 };
    let scale = p.img_w as f32 / dw.max(1.0);
    let mut out: Vec<u8> = b"q BT 3 Tr\n".to_vec();
    for w in &p.words {
        let (x, y, ww, hh) = (w.x / scale, w.y / scale, w.w / scale, w.h / scale);
        if ww <= 0.5 || hh <= 0.5 {
            continue;
        }
        let size = hh * 0.9;
        let base = y + hh * 0.8;
        // Repère du texte : direction des x d'affichage, hauteur vers le haut, dans l'espace de la page.
        let o = g.to_user(x, base);
        let ex = g.to_user(x + 1.0, base);
        let ey = g.to_user(x, base - 1.0);
        let (a, b) = (ex.0 - o.0, ex.1 - o.1);
        let (c, d) = (ey.0 - o.0, ey.1 - o.1);
        let natural = text_width(&w.text, size, false, 0.0).max(0.1);
        let tz = (ww / natural * 100.0).clamp(10.0, 1000.0);
        out.extend(format!("/AuraOCR {size:.2} Tf {tz:.1} Tz {a:.4} {b:.4} {c:.4} {d:.4} {:.2} {:.2} Tm ", o.0, o.1).bytes());
        out.extend(pdf_string(&format!("{} ", w.text)));
        out.extend(b" Tj\n");
    }
    out.extend(b"ET Q\n");
    out
}

/// Ajoute le texte reconnu aux pages de `doc` (et redresse si demandé).
pub fn apply(doc: &mut Document, pages: &[PageOcr], deskew: bool) -> Result<(), String> {
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" });
    let ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for p in pages {
        let Some(&pid) = ids.get(p.page as usize) else { continue };
        let (g, mut res, contents) = {
            let d = doc.get_dictionary(pid).map_err(|e| e.to_string())?;
            let res = crate::annot::pdf::inherited(doc, d, b"Resources").and_then(|r| r.as_dict().ok()).cloned().unwrap_or_default();
            let contents: Vec<Object> = match d.get(b"Contents") {
                Ok(Object::Array(a)) => a.clone(),
                Ok(o) => vec![o.clone()],
                Err(_) => Vec::new(),
            };
            (page_geom(doc, d), res, contents)
        };
        let mut fonts = get(doc, &res, b"Font").and_then(|f| f.as_dict().ok()).cloned().unwrap_or_default();
        fonts.set("AuraOCR", font);
        res.set("Font", fonts);
        let text = doc.add_object(Stream::new(dictionary! {}, page_text(&g, p)));
        // q [rotation] · q contenu d'origine Q · texte invisible · Q
        let rotate = deskew && p.skew.abs() > MIN_SKEW && p.skew.abs() < 20.0;
        let outer = if rotate {
            // Rotation autour du centre de la page (sens trigonométrique, comme Tesseract).
            let t = p.skew.to_radians();
            let (cs, sn) = (t.cos(), t.sin());
            let (cx, cy) = ((g.x0 + g.x1) / 2.0, (g.y0 + g.y1) / 2.0);
            let e = cx - cs * cx + sn * cy;
            let f = cy - sn * cx - cs * cy;
            format!("q {cs:.6} {sn:.6} {:.6} {cs:.6} {e:.3} {f:.3} cm\nq\n", -sn)
        } else {
            "q\nq\n".to_string()
        };
        let mut list = vec![Object::Reference(doc.add_object(Stream::new(dictionary! {}, outer.into_bytes())))];
        list.extend(contents);
        list.push(Object::Reference(doc.add_object(Stream::new(dictionary! {}, b"Q\n".to_vec()))));
        list.push(Object::Reference(text));
        list.push(Object::Reference(doc.add_object(Stream::new(dictionary! {}, b"Q\n".to_vec()))));
        let page = doc.get_dictionary_mut(pid).map_err(|e| e.to_string())?;
        page.set("Contents", list);
        page.set("Resources", res);
    }
    Ok(())
}
