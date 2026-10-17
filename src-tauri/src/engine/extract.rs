//! Extraction du contenu d'une page pour la conversion : segments de texte
//! avec leur mise en forme (taille, gras, italique, couleur), regroupés en
//! lignes ; image de la page à la résolution demandée.

use pdfium_render::prelude::*;

use super::{coord_config, to_display_rect};
use crate::convert::model::{Figure, Line, PageData, PageImage, Span};
use crate::error::{AppError, AppResult};

struct Ch {
    c: char,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    size: f32,
    bold: bool,
    italic: bool,
    color: String,
    family: &'static str,
}

fn family_of(ch: &PdfPageTextChar, name: &str) -> &'static str {
    if ch.font_is_fixed_pitch() || name.contains("courier") || name.contains("mono") || name.contains("consol") {
        "mono"
    } else if ch.font_is_serif() || ["times", "georgia", "garamond", "serif", "cambria", "book"].iter().any(|k| name.contains(k)) {
        "serif"
    } else {
        "sans"
    }
}

fn style_of(ch: &PdfPageTextChar) -> (f32, bool, bool, String) {
    let name = ch.font_name().to_ascii_lowercase();
    let bold = ch.font_weight().map(|w| matches!(w, PdfFontWeight::Weight600 | PdfFontWeight::Weight700Bold | PdfFontWeight::Weight800 | PdfFontWeight::Weight900 | PdfFontWeight::Custom(600..=1000))).unwrap_or(false)
        || name.contains("bold")
        || name.contains("black")
        || name.contains("heavy")
        || ch.font_is_bold_reenforced();
    let italic = ch.font_is_italic() || name.contains("italic") || name.contains("oblique");
    let color = ch.fill_color().map(|c| format!("{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue())).unwrap_or_else(|_| "000000".into());
    (ch.scaled_font_size().value, bold, italic, color)
}

pub(super) fn extract_page(doc: &PdfDocument, page: u32, dpi: Option<f32>, with_text: bool, with_figures: bool) -> AppResult<PageData> {
    let p = doc.pages().get(page as PdfPageIndex).map_err(AppError::internal)?;
    let cfg = coord_config();
    let (w, h) = {
        let r = to_display_rect(&p, &cfg, PdfRect::new_from_values(0.0, 0.0, p.height().value, p.width().value));
        (r.w, r.h)
    };
    let mut lines: Vec<Line> = Vec::new();
    if with_text {
        if let Ok(text) = p.text() {
            let mut chars: Vec<Option<Ch>> = Vec::new();
            for ch in text.chars().iter() {
                let c = ch.unicode_char().unwrap_or('\u{FFFD}');
                if c == '\r' || c == '\n' {
                    chars.push(None);
                    continue;
                }
                let (size, bold, italic, color) = style_of(&ch);
                let family = family_of(&ch, &ch.font_name().to_ascii_lowercase());
                let Some(r) = ch.loose_bounds().ok().map(|r| to_display_rect(&p, &cfg, r)) else { continue };
                chars.push(Some(Ch { c, x: r.x, y: r.y, w: r.w, h: r.h, size, bold, italic, color, family }));
            }
            lines = group(chars);
        }
    }
    let image = match dpi {
        Some(dpi) => {
            let width = ((w * dpi / 72.0).round() as i32).clamp(16, 12_000);
            let cfg = PdfRenderConfig::new()
                .set_target_width(width as Pixels)
                .set_maximum_height(16_000)
                .render_annotations(true)
                .render_form_data(true)
                .set_reverse_byte_order(true)
                .clear_before_rendering(true);
            let bmp = p.render_with_config(&cfg).map_err(AppError::internal)?;
            Some(PageImage { width: bmp.width() as u32, height: bmp.height() as u32, rgba: bmp.as_raw_bytes() })
        }
        None => None,
    };
    let figures = if with_figures { find_figures(&p, &cfg, w, h, &mut lines) } else { Vec::new() };
    Ok(PageData { w, h, lines, image, figures })
}

#[derive(Clone, Copy)]
struct Bx {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Bx {
    fn near(&self, o: &Bx, pad: f32) -> bool {
        self.x - pad < o.x + o.w && o.x - pad < self.x + self.w && self.y - pad < o.y + o.h && o.y - pad < self.y + self.h
    }
    /// Voisins, ou alignés en rangée (ou en colonne) avec un écart modéré :
    /// les barres d'un histogramme, par exemple.
    fn related(&self, o: &Bx) -> bool {
        if self.near(o, 6.0) {
            return true;
        }
        let vo = (self.y + self.h).min(o.y + o.h) - self.y.max(o.y);
        let ho = (self.x + self.w).min(o.x + o.w) - self.x.max(o.x);
        let hgap = (self.x.max(o.x) - (self.x + self.w).min(o.x + o.w)).max(0.0);
        let vgap = (self.y.max(o.y) - (self.y + self.h).min(o.y + o.h)).max(0.0);
        (vo > 0.0 && (self.y + self.h - (o.y + o.h)).abs() < 2.0 || vo >= 0.5 * self.h.min(o.h)) && hgap <= 30.0f32.max(0.75 * self.w.min(o.w))
            || (ho > 0.0 && (self.x - o.x).abs() < 2.0 || ho >= 0.5 * self.w.min(o.w)) && vgap <= 30.0f32.max(0.75 * self.h.min(o.h))
    }
    fn union(&self, o: &Bx) -> Bx {
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        Bx { x, y, w: (self.x + self.w).max(o.x + o.w) - x, h: (self.y + self.h).max(o.y + o.h) - y }
    }
    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

/// Illustrations de la page : images, et groupes de tracés vectoriels assez
/// grands et pauvres en texte (graphiques, schémas). Les tableaux et les
/// encadrés de texte restent du texte. Chaque zone est rendue en PNG (150 dpi)
/// et les lignes de texte qu'elle couvre (légendes d'axes…) sont retirées.
fn find_figures(p: &PdfPage, cfg: &PdfRenderConfig, w: f32, h: f32, lines: &mut Vec<Line>) -> Vec<Figure> {
    let page_area = w * h;
    let mut items: Vec<(Bx, bool)> = Vec::new();
    for obj in p.objects().iter() {
        let is_image = match obj.object_type() {
            PdfPageObjectType::Image => true,
            PdfPageObjectType::Path | PdfPageObjectType::Shading | PdfPageObjectType::XObjectForm => false,
            _ => continue,
        };
        let Ok(q) = obj.bounds() else { continue };
        let r = to_display_rect(p, cfg, q.to_rect());
        let b = Bx { x: r.x.max(0.0), y: r.y.max(0.0), w: r.w.min(w), h: r.h.min(h) };
        // Fonds de page et filets isolés : ignorés.
        if b.w * b.h > page_area * 0.6 || (b.w < 1.5 && b.h < 1.5) {
            continue;
        }
        items.push((b, is_image));
        if items.len() > 4000 {
            return Vec::new();
        }
    }
    // Regroupement des objets voisins.
    let mut groups: Vec<(Bx, usize, bool)> = Vec::new();
    for (b, img) in items {
        let mut g = (b, 1usize, img);
        loop {
            let Some(i) = groups.iter().position(|(o, _, _)| o.related(&g.0)) else { break };
            let o = groups.swap_remove(i);
            g = (g.0.union(&o.0), g.1 + o.1, g.2 || o.2);
        }
        groups.push(g);
    }
    let chars_in = |b: &Bx, lines: &[Line]| -> usize {
        lines.iter().flat_map(|l| l.spans.iter()).filter(|s| b.contains(s.x + s.w / 2.0, s.y + s.h / 2.0)).map(|s| s.text.chars().count()).sum()
    };
    let zones: Vec<Bx> = groups
        .into_iter()
        .filter(|(b, n, img)| {
            let area = b.w * b.h;
            if b.w * b.h > page_area * 0.85 {
                return false;
            }
            let big = if *img { area >= 400.0 && b.w >= 12.0 && b.h >= 12.0 } else { *n >= 3 && area >= 3000.0 && b.w >= 40.0 && b.h >= 30.0 };
            // Peu de texte (moins de 1,2 caractère pour 1 000 pt²) : sinon tableau ou encadré.
            big && (*img || (chars_in(b, lines) as f32) < area / 1000.0 * 1.2)
        })
        .map(|(b, _, _)| b)
        .collect();
    if zones.is_empty() {
        return Vec::new();
    }
    let scale = 150.0 / 72.0;
    let rc = PdfRenderConfig::new()
        .set_target_width(((w * scale).round() as i32).clamp(16, 8000) as Pixels)
        .set_maximum_height(12_000)
        .render_annotations(true)
        .render_form_data(true)
        .set_reverse_byte_order(true)
        .clear_before_rendering(true);
    let Ok(bmp) = p.render_with_config(&rc) else { return Vec::new() };
    let (bw, bh) = (bmp.width() as u32, bmp.height() as u32);
    let Some(full) = image::RgbaImage::from_raw(bw, bh, bmp.as_raw_bytes()) else { return Vec::new() };
    let k = bw as f32 / w;
    let mut out = Vec::new();
    for z in zones {
        let (x0, y0) = ((z.x * k).floor().max(0.0) as u32, (z.y * k).floor().max(0.0) as u32);
        let (x1, y1) = (((z.x + z.w) * k).ceil().min(bw as f32) as u32, ((z.y + z.h) * k).ceil().min(bh as f32) as u32);
        if x1 <= x0 + 2 || y1 <= y0 + 2 {
            continue;
        }
        let crop = image::imageops::crop_imm(&full, x0, y0, x1 - x0, y1 - y0).to_image();
        let mut png = Vec::new();
        if crop.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).is_err() {
            continue;
        }
        // Le texte de la zone fait partie de l'image.
        for l in lines.iter_mut() {
            l.spans.retain(|s| !z.contains(s.x + s.w / 2.0, s.y + s.h / 2.0));
        }
        lines.retain(|l| !l.spans.is_empty());
        out.push(Figure { x: z.x, y: z.y, w: z.w, h: z.h, png });
    }
    out
}

/// Caractères → segments (même style, sans grand écart) → lignes.
fn group(chars: Vec<Option<Ch>>) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    let mut spans: Vec<Span> = Vec::new();
    let mut cur: Option<Span> = None;
    let mut last: Option<(f32, f32, f32)> = None; // (x fin, y, h) du dernier glyphe
    let flush_span = |cur: &mut Option<Span>, spans: &mut Vec<Span>| {
        if let Some(mut s) = cur.take() {
            let trimmed = s.text.trim_end().to_string();
            if !trimmed.trim().is_empty() {
                s.text = trimmed;
                spans.push(s);
            }
        }
    };
    let flush_line = |spans: &mut Vec<Span>, lines: &mut Vec<Line>| {
        if !spans.is_empty() {
            lines.push(Line { spans: std::mem::take(spans) });
        }
    };
    for ch in chars {
        let Some(ch) = ch else {
            flush_span(&mut cur, &mut spans);
            flush_line(&mut spans, &mut lines);
            last = None;
            continue;
        };
        if ch.c.is_whitespace() {
            if let Some(s) = cur.as_mut() {
                if !s.text.ends_with(' ') {
                    s.text.push(' ');
                }
            }
            continue;
        }
        if ch.w <= 0.0 || ch.h <= 0.0 {
            continue;
        }
        // Changement de ligne implicite : retour en arrière ou saut vertical.
        if let Some((xe, y, h)) = last {
            if (ch.y - y).abs() > h.max(ch.h) * 0.6 || ch.x < xe - h * 2.0 {
                flush_span(&mut cur, &mut spans);
                flush_line(&mut spans, &mut lines);
            }
        }
        let gap = last.map(|(xe, _, _)| ch.x - xe).unwrap_or(0.0);
        let same = cur
            .as_ref()
            .map(|s| s.bold == ch.bold && s.italic == ch.italic && s.color == ch.color && s.family == ch.family && (s.size - ch.size).abs() < 0.6 && gap < ch.size.max(4.0) * 1.2)
            .unwrap_or(false);
        if !same {
            flush_span(&mut cur, &mut spans);
            cur = Some(Span { text: String::new(), x: ch.x, y: ch.y, w: 0.0, h: ch.h, size: ch.size, bold: ch.bold, italic: ch.italic, color: ch.color.clone(), family: ch.family.to_string() });
        }
        let s = cur.as_mut().unwrap();
        s.text.push(ch.c);
        let x1 = (s.x + s.w).max(ch.x + ch.w);
        let y0 = s.y.min(ch.y);
        let y1 = (s.y + s.h).max(ch.y + ch.h);
        s.w = x1 - s.x;
        s.y = y0;
        s.h = y1 - y0;
        last = Some((ch.x + ch.w, ch.y, ch.h));
    }
    flush_span(&mut cur, &mut spans);
    flush_line(&mut spans, &mut lines);
    // Ordre de lecture : de haut en bas, puis de gauche à droite.
    lines.sort_by(|a, b| a.y().partial_cmp(&b.y()).unwrap_or(std::cmp::Ordering::Equal).then(a.x().partial_cmp(&b.x()).unwrap_or(std::cmp::Ordering::Equal)));
    merge_same_row(lines)
}

/// Segments d'une même rangée émis séparément (colonnes d'un tableau) : une ligne.
fn merge_same_row(lines: Vec<Line>) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::new();
    for l in lines {
        if let Some(prev) = out.last_mut() {
            let (py, ph) = (prev.y(), prev.bottom() - prev.y());
            let (ly, lh) = (l.y(), l.bottom() - l.y());
            let overlap = (prev.bottom().min(l.bottom()) - py.max(ly)).max(0.0);
            if overlap > ph.min(lh) * 0.6 && l.x() >= prev.right() - 1.0 {
                prev.spans.extend(l.spans);
                continue;
            }
        }
        out.push(l);
    }
    for l in &mut out {
        l.spans.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
    }
    out
}
