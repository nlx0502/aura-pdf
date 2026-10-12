//! Édition du contenu d'une page avec PDFium : blocs de texte (repérés en
//! regroupant les objets texte en lignes puis en paragraphes), images.
//!
//! Les modifications sont faites dans le document chargé en mémoire (l'aperçu
//! est donc immédiat) ; l'enregistrement les écrit ensuite dans le fichier.
//! Les objets sont désignés par leur index sur la page ; l'interface relit la
//! liste après chaque opération.

use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};

use crate::annot::ap::display_matrix;
use crate::annot::geom::{PageGeom, Rect};
use crate::annot::model::rgb;
use crate::annot::text::text_width_in;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentItem {
    /// « text » ou « image ».
    pub kind: &'static str,
    /// Index des objets de la page qui composent l'élément.
    pub ids: Vec<u32>,
    /// Boîte en points d'affichage.
    pub rect: Rect,
    pub text: String,
    pub font: String,
    pub bold: bool,
    pub italic: bool,
    pub size: f32,
    pub color: String,
    pub align: u8,
    /// Interligne (multiple de la taille).
    pub line_height: f32,
    /// Dimensions en pixels (images).
    pub px: Option<[u32; 2]>,
    /// Ligne de base de la première ligne (texte), en points d'affichage.
    pub baseline: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSpec {
    pub text: String,
    pub rect: Rect,
    pub font: String,
    pub bold: bool,
    pub italic: bool,
    pub size: f32,
    pub color: String,
    #[serde(default)]
    pub align: u8,
    #[serde(default = "default_lh")]
    pub line_height: f32,
    /// Ligne de base de la première ligne (points d'affichage), pour un bloc existant.
    #[serde(default)]
    pub baseline: Option<f32>,
}

fn default_lh() -> f32 {
    1.2
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum ContentOp {
    ReplaceText { page: u32, ids: Vec<u32>, spec: TextSpec },
    AddText { page: u32, spec: TextSpec },
    Move { page: u32, ids: Vec<u32>, dx: f32, dy: f32 },
    Delete { page: u32, ids: Vec<u32> },
    ImageReplace { page: u32, id: u32, path: String },
    ImageRotate { page: u32, id: u32 },
    ImageResize { page: u32, id: u32, rect: Rect },
    /// Rogne l'image à la zone `rect` (points d'affichage).
    ImageCrop { page: u32, id: u32, rect: Rect },
    AddImage { page: u32, rect: Rect, path: String },
}

impl ContentOp {
    pub fn page(&self) -> u32 {
        match self {
            ContentOp::ReplaceText { page, .. }
            | ContentOp::AddText { page, .. }
            | ContentOp::Move { page, .. }
            | ContentOp::Delete { page, .. }
            | ContentOp::ImageReplace { page, .. }
            | ContentOp::ImageRotate { page, .. }
            | ContentOp::ImageResize { page, .. }
            | ContentOp::ImageCrop { page, .. }
            | ContentOp::AddImage { page, .. } => *page,
        }
    }
}

pub fn geom(page: &PdfPage) -> PageGeom {
    let b = page.boundaries();
    let r = b.crop().or_else(|_| b.media()).map(|x| x.bounds).unwrap_or(PdfRect::new_from_values(0.0, 0.0, 792.0, 612.0));
    let rot = match page.rotation().unwrap_or(PdfPageRenderRotation::None) {
        PdfPageRenderRotation::Degrees90 => 90,
        PdfPageRenderRotation::Degrees180 => 180,
        PdfPageRenderRotation::Degrees270 => 270,
        _ => 0,
    };
    PageGeom::new(r.left().value, r.bottom().value, r.right().value, r.top().value, rot)
}

fn quad_rect(q: &PdfQuadPoints, g: &PageGeom) -> Rect {
    let r = q.to_rect();
    g.rect_to_display(&Rect::from_corners(r.left().value, r.bottom().value, r.right().value, r.top().value))
}

fn hex(c: PdfColor) -> String {
    format!("#{:02X}{:02X}{:02X}", c.red(), c.green(), c.blue())
}

/// Famille, gras, italique d'une police d'après son nom et ses attributs.
fn font_style(f: &PdfFont) -> (String, bool, bool) {
    let raw = f.name();
    let name = raw.to_ascii_lowercase();
    // Police installée sur le poste (« ABCDEF+Calibri-Bold » → « Calibri ») : proposée telle quelle.
    let installed = crate::sysfonts::match_family(&raw);
    let family = if f.is_fixed_pitch() || name.contains("courier") || name.contains("mono") || name.contains("consol") {
        "mono"
    } else if f.is_serif() || ["times", "georgia", "garamond", "cambria", "serif", "minion", "book"].iter().any(|k| name.contains(k)) {
        "serif"
    } else {
        "sans"
    };
    let bold = name.contains("bold") || name.contains("black") || name.contains("heavy") || name.contains("semibold") || f.weight().map(|w| matches!(w, PdfFontWeight::Weight600 | PdfFontWeight::Weight700Bold | PdfFontWeight::Weight800 | PdfFontWeight::Weight900)).unwrap_or(false);
    let italic = name.contains("italic") || name.contains("oblique") || f.is_italic();
    (installed.unwrap_or_else(|| family.to_string()), bold, italic)
}

struct Run {
    id: u32,
    rect: Rect,
    baseline: f32,
    size: f32,
    text: String,
    font: String,
    bold: bool,
    italic: bool,
    color: String,
}

/// Éléments modifiables d'une page.
pub fn list(page: &PdfPage) -> Vec<ContentItem> {
    let g = geom(page);
    let mut runs: Vec<Run> = Vec::new();
    let mut items: Vec<ContentItem> = Vec::new();
    for (i, obj) in page.objects().iter().enumerate() {
        let Ok(bounds) = obj.bounds() else { continue };
        let rect = quad_rect(&bounds, &g);
        if rect.w <= 0.0 || rect.h <= 0.0 {
            continue;
        }
        if let Some(t) = obj.as_text_object() {
            let text = t.text();
            if text.trim().is_empty() {
                continue;
            }
            let Ok(m) = t.matrix() else { continue };
            // Seul le texte horizontal à l'écran est regroupé et modifiable.
            let (ex, ey) = (m.e(), m.f());
            let (p0x, p0y) = g.to_display(ex, ey);
            let (p1x, p1y) = g.to_display(ex + m.a(), ey + m.b());
            let (dx, dy) = (p1x - p0x, p1y - p0y);
            if !(dx > 0.0 && dy.abs() < 0.01 * dx) {
                continue;
            }
            let font = t.font();
            let (family, bold, italic) = font_style(&font);
            let base_y = p0y;
            runs.push(Run {
                id: i as u32,
                rect,
                baseline: base_y,
                size: t.scaled_font_size().value.max(1.0),
                text,
                font: family,
                bold,
                italic,
                color: t.fill_color().map(hex).unwrap_or_else(|_| "#000000".into()),
            });
        } else if let Some(img) = obj.as_image_object() {
            let px = match (img.width(), img.height()) {
                (Ok(w), Ok(h)) => Some([w as u32, h as u32]),
                _ => None,
            };
            items.push(ContentItem {
                kind: "image",
                ids: vec![i as u32],
                rect,
                text: String::new(),
                font: "sans".into(),
                bold: false,
                italic: false,
                size: 0.0,
                color: String::new(),
                align: 0,
                line_height: 0.0,
                px,
                baseline: 0.0,
            });
        }
    }
    let mut blocks = group(runs);
    blocks.append(&mut items);
    blocks
}

/// Regroupe les morceaux de texte en lignes, puis les lignes en paragraphes.
fn group(mut runs: Vec<Run>) -> Vec<ContentItem> {
    runs.sort_by(|a, b| a.baseline.total_cmp(&b.baseline).then(a.rect.x.total_cmp(&b.rect.x)));
    // Lignes.
    let mut lines: Vec<Vec<Run>> = Vec::new();
    for r in runs {
        let fits = lines.iter_mut().rev().take(6).find(|l| {
            let last = l.last().unwrap();
            (last.baseline - r.baseline).abs() < 0.35 * last.size.min(r.size)
                && (last.size - r.size).abs() < 0.25 * last.size
                && r.rect.x >= last.rect.x - 1.0
                && r.rect.x - (last.rect.x + last.rect.w) < 2.5 * last.size
        });
        match fits {
            Some(l) => l.push(r),
            None => lines.push(vec![r]),
        }
    }
    struct Line {
        ids: Vec<u32>,
        rect: Rect,
        baseline: f32,
        size: f32,
        text: String,
        first: (String, bool, bool, String),
    }
    let mut ls: Vec<Line> = lines
        .into_iter()
        .map(|l| {
            let mut text = String::new();
            let mut rect = l[0].rect;
            let mut prev_right = l[0].rect.x;
            for (k, r) in l.iter().enumerate() {
                if k > 0 {
                    let gap = r.rect.x - prev_right;
                    if gap > 0.18 * r.size && !text.ends_with(' ') && !r.text.starts_with(' ') {
                        text.push(' ');
                    }
                    rect = rect.union(&r.rect);
                }
                text.push_str(&r.text);
                prev_right = r.rect.x + r.rect.w;
            }
            let size = l.iter().map(|r| r.size).fold(0.0, f32::max);
            Line {
                ids: l.iter().map(|r| r.id).collect(),
                rect,
                baseline: l[0].baseline,
                size,
                text: text.trim_end().to_string(),
                first: (l[0].font.clone(), l[0].bold, l[0].italic, l[0].color.clone()),
            }
        })
        .collect();
    ls.sort_by(|a, b| a.baseline.total_cmp(&b.baseline));

    // Paragraphes : lignes successives de même corps, alignées et rapprochées.
    let mut blocks: Vec<Vec<Line>> = Vec::new();
    'outer: for l in ls {
        for b in blocks.iter_mut().rev().take(4) {
            let last = b.last().unwrap();
            let gap = l.baseline - last.baseline;
            let overlap = l.rect.x < last.rect.x + last.rect.w && last.rect.x < l.rect.x + l.rect.w;
            let same_style = (last.size - l.size).abs() < 0.15 * last.size && last.first.1 == l.first.1;
            if gap > 0.0 && gap < 1.9 * last.size && overlap && same_style && (l.rect.x - last.rect.x).abs() < 4.0 * l.size {
                b.push(l);
                continue 'outer;
            }
        }
        blocks.push(vec![l]);
    }

    blocks
        .into_iter()
        .map(|b| {
            let mut rect = b[0].rect;
            for l in &b[1..] {
                rect = rect.union(&l.rect);
            }
            let size = b[0].size;
            let lh = if b.len() > 1 {
                let mut d: Vec<f32> = b.windows(2).map(|w| w[1].baseline - w[0].baseline).collect();
                d.sort_by(f32::total_cmp);
                (d[d.len() / 2] / size).clamp(0.9, 3.0)
            } else {
                1.2
            };
            // Alignement : centré si les centres coïncident et les bords gauches varient.
            let align = if b.len() > 1 {
                let lefts: Vec<f32> = b.iter().map(|l| l.rect.x).collect();
                let rights: Vec<f32> = b.iter().map(|l| l.rect.x + l.rect.w).collect();
                let centers: Vec<f32> = b.iter().map(|l| l.rect.x + l.rect.w / 2.0).collect();
                let spread = |v: &[f32]| v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min);
                if spread(&lefts) < 2.0 {
                    0
                } else if spread(&centers) < 3.0 {
                    1
                } else if spread(&rights) < 2.0 {
                    2
                } else {
                    0
                }
            } else {
                0
            };
            // Texte : les lignes d'un paragraphe sont jointes par un espace ; une
            // ligne nettement plus courte termine un paragraphe.
            let width = rect.w;
            let mut text = String::new();
            for (k, l) in b.iter().enumerate() {
                text.push_str(&l.text);
                if k + 1 < b.len() {
                    let short = l.rect.w < width * 0.7 && align == 0;
                    if short {
                        text.push('\n');
                    } else if l.text.ends_with('-') && !l.text.ends_with(" -") {
                        // Césure : on recolle le mot.
                        text.pop();
                    } else {
                        text.push(' ');
                    }
                }
            }
            let (font, bold, italic, color) = b[0].first.clone();
            ContentItem {
                kind: "text",
                ids: b.iter().flat_map(|l| l.ids.iter().copied()).collect(),
                rect,
                text,
                font,
                bold,
                italic,
                size: (size * 10.0).round() / 10.0,
                color,
                align,
                line_height: (lh * 100.0).round() / 100.0,
                px: None,
                baseline: b[0].baseline,
            }
        })
        .collect()
}

fn std_font(doc: &mut PdfDocument, family: &str, bold: bool, italic: bool) -> PdfFontToken {
    let f = doc.fonts_mut();
    match (family, bold, italic) {
        ("serif", false, false) => f.times_roman(),
        ("serif", true, false) => f.times_bold(),
        ("serif", false, true) => f.times_italic(),
        ("serif", true, true) => f.times_bold_italic(),
        ("mono", false, false) => f.courier(),
        ("mono", true, false) => f.courier_bold(),
        ("mono", false, true) => f.courier_oblique(),
        ("mono", true, true) => f.courier_bold_oblique(),
        (_, false, false) => f.helvetica(),
        (_, true, false) => f.helvetica_bold(),
        (_, false, true) => f.helvetica_oblique(),
        _ => f.helvetica_bold_oblique(),
    }
}

fn remove_ids(page: &mut PdfPage, ids: &[u32]) -> Result<(), PdfiumError> {
    let mut sorted: Vec<u32> = ids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    for id in sorted.into_iter().rev() {
        page.objects_mut().remove_object_at_index(id as PdfPageObjectIndex)?;
    }
    Ok(())
}

/// Découpe en lignes avec une fonction de mesure.
fn wrap(text: &str, max_w: f32, measure: &dyn Fn(&str) -> f32) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let cand = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if !line.is_empty() && measure(&cand) > max_w + 0.5 {
                out.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = cand;
            }
        }
        out.push(line);
    }
    out
}

/// Place un bloc de texte (police standard de la famille choisie, découpage exact).
fn place_text(doc: &mut PdfDocument, page_index: u32, spec: &TextSpec) -> Result<(), PdfiumError> {
    let size = spec.size.clamp(2.0, 400.0);
    let g = {
        let page = doc.pages().get(page_index as PdfPageIndex)?;
        geom(&page)
    };
    let lin = display_matrix(&g);
    let family = spec.font.as_str();
    // Police du système : sous-ensemble des glyphes du texte, intégré au document.
    let sys = if matches!(family, "sans" | "serif" | "mono") { None } else { crate::sysfonts::load(family, spec.bold, spec.italic) };
    let token = match &sys {
        Some(f) => doc.fonts_mut().load_true_type_from_bytes(&f.subset(&spec.text), true)?,
        None => std_font(doc, family, spec.bold, spec.italic),
    };
    let measure = |s: &str| match &sys {
        Some(f) => f.text_width(s, size),
        None => text_width_in(s, size, family, spec.bold),
    };
    let lines = wrap(&spec.text, spec.rect.w.max(size), &measure);
    let fake_bold = sys.as_ref().map(|f| f.fake_bold).unwrap_or(false);
    let skew = if sys.as_ref().map(|f| f.fake_italic).unwrap_or(false) { 0.2 } else { 0.0 };
    let first_base = spec.baseline.unwrap_or(spec.rect.y + size * 0.8);
    let (r, gg, b) = rgb(&spec.color);
    let color = PdfColor::new((r * 255.0).round() as u8, (gg * 255.0).round() as u8, (b * 255.0).round() as u8, 255);

    let mut page = doc.pages().get(page_index as PdfPageIndex)?;
    for (k, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let w = measure(line);
        let x = match spec.align {
            1 => spec.rect.x + (spec.rect.w - w) / 2.0,
            2 => spec.rect.x + spec.rect.w - w,
            _ => spec.rect.x,
        };
        let y = first_base + k as f32 * size * spec.line_height;
        let mut obj = PdfPageTextObject::new(doc, line, token, PdfPoints::new(size))?;
        obj.set_fill_color(color)?;
        if fake_bold {
            // Gras absent de la famille : contour de la couleur du texte.
            obj.set_render_mode(PdfPageTextRenderMode::FilledThenStroked)?;
            obj.set_stroke_color(color)?;
            obj.set_stroke_width(PdfPoints::new(size * 0.03))?;
        }
        // Ligne de base horizontale à l'écran, quelle que soit la rotation de la page
        // (inclinaison si l'italique est absent de la famille).
        let (e, f) = g.to_user(x, y);
        let (a, b, c, d) = (lin[0], lin[1], -lin[2], -lin[3]);
        obj.reset_matrix(PdfMatrix::new(a, b, c + skew * a, d + skew * b, e, f))?;
        page.objects_mut().add_text_object(obj)?;
    }
    Ok(())
}

/// Applique une opération au document en mémoire.
pub fn apply(doc: &mut PdfDocument, op: &ContentOp) -> Result<(), String> {
    let e = |x: PdfiumError| format!("{x:?}");
    let page_index = op.page();
    match op {
        ContentOp::ReplaceText { ids, spec, .. } => {
            {
                let mut page = doc.pages().get(page_index as PdfPageIndex).map_err(e)?;
                remove_ids(&mut page, ids).map_err(e)?;
            }
            if !spec.text.trim().is_empty() {
                place_text(doc, page_index, spec).map_err(e)?;
            }
        }
        ContentOp::AddText { spec, .. } => {
            if !spec.text.trim().is_empty() {
                place_text(doc, page_index, spec).map_err(e)?;
            }
        }
        ContentOp::Move { ids, dx, dy, .. } => {
            let mut page = doc.pages().get(page_index as PdfPageIndex).map_err(e)?;
            let g = geom(&page);
            let (x0, y0) = g.to_user(0.0, 0.0);
            let (x1, y1) = g.to_user(*dx, *dy);
            for id in ids {
                if let Ok(mut o) = page.objects_mut().get(*id as PdfPageObjectIndex) {
                    o.translate(PdfPoints::new(x1 - x0), PdfPoints::new(y1 - y0)).map_err(e)?;
                }
            }
        }
        ContentOp::Delete { ids, .. } => {
            let mut page = doc.pages().get(page_index as PdfPageIndex).map_err(e)?;
            remove_ids(&mut page, ids).map_err(e)?;
        }
        ContentOp::ImageReplace { id, path, .. } => {
            let img = image::open(path).map_err(|x| format!("image illisible ({x})"))?;
            let mut page = doc.pages().get(page_index as PdfPageIndex).map_err(e)?;
            let g = geom(&page);
            let mut o = page.objects_mut().get(*id as PdfPageObjectIndex).map_err(e)?;
            let r = quad_rect(&o.bounds().map_err(e)?, &g);
            if let Some(io) = o.as_image_object_mut() {
                io.set_image(&img).map_err(e)?;
            }
            // Nouvelle image centrée dans l'ancienne boîte, proportions respectées.
            let ratio = img.width() as f32 / img.height().max(1) as f32;
            let (w, h) = if r.w / r.h.max(0.01) > ratio { (r.h * ratio, r.h) } else { (r.w, r.w / ratio) };
            let target = Rect { x: r.x + (r.w - w) / 2.0, y: r.y + (r.h - h) / 2.0, w, h };
            o.reset_matrix(image_matrix(&g, &target)).map_err(e)?;
        }
        ContentOp::ImageRotate { id, .. } => {
            let mut page = doc.pages().get(page_index as PdfPageIndex).map_err(e)?;
            let mut o = page.objects_mut().get(*id as PdfPageObjectIndex).map_err(e)?;
            let b = o.bounds().map_err(e)?.to_rect();
            let (cx, cy) = ((b.left().value + b.right().value) / 2.0, (b.bottom().value + b.top().value) / 2.0);
            o.translate(PdfPoints::new(-cx), PdfPoints::new(-cy)).map_err(e)?;
            o.rotate_clockwise_degrees(90.0).map_err(e)?;
            o.translate(PdfPoints::new(cx), PdfPoints::new(cy)).map_err(e)?;
        }
        ContentOp::ImageResize { id, rect, .. } => {
            let mut page = doc.pages().get(page_index as PdfPageIndex).map_err(e)?;
            let g = geom(&page);
            let mut o = page.objects_mut().get(*id as PdfPageObjectIndex).map_err(e)?;
            let cur = g.rect_to_user(&quad_rect(&o.bounds().map_err(e)?, &g));
            let tgt = g.rect_to_user(rect);
            if cur.w < 0.01 || cur.h < 0.01 {
                return Ok(());
            }
            o.translate(PdfPoints::new(-cur.x), PdfPoints::new(-cur.y)).map_err(e)?;
            o.scale(tgt.w / cur.w, tgt.h / cur.h).map_err(e)?;
            o.translate(PdfPoints::new(tgt.x), PdfPoints::new(tgt.y)).map_err(e)?;
        }
        ContentOp::ImageCrop { id, rect, .. } => {
            let mut page = doc.pages().get(page_index as PdfPageIndex).map_err(e)?;
            let g = geom(&page);
            let mut o = page.objects_mut().get(*id as PdfPageObjectIndex).map_err(e)?;
            let m = o.matrix().map_err(e)?;
            let mm = [m.a(), m.b(), m.c(), m.d(), m.e(), m.f()];
            let det = mm[0] * mm[3] - mm[1] * mm[2];
            if det.abs() < 1e-6 {
                return Ok(());
            }
            let inv = |x: f32, y: f32| {
                let (x, y) = (x - mm[4], y - mm[5]);
                ((mm[3] * x - mm[2] * y) / det, (-mm[1] * x + mm[0] * y) / det)
            };
            let u = g.rect_to_user(rect);
            let pts = [inv(u.x, u.y), inv(u.x + u.w, u.y), inv(u.x, u.y + u.h), inv(u.x + u.w, u.y + u.h)];
            let u0 = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min).clamp(0.0, 1.0);
            let u1 = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max).clamp(0.0, 1.0);
            let v0 = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min).clamp(0.0, 1.0);
            let v1 = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max).clamp(0.0, 1.0);
            if u1 - u0 < 0.01 || v1 - v0 < 0.01 {
                return Ok(());
            }
            let Some(io) = o.as_image_object_mut() else { return Ok(()) };
            let img = io.get_raw_image().map_err(e)?;
            let (w, h) = (img.width(), img.height());
            let (px0, px1) = ((u0 * w as f32).floor() as u32, (u1 * w as f32).ceil().min(w as f32) as u32);
            // Ligne 0 de l'image = haut du carré unité (v = 1).
            let (py0, py1) = (((1.0 - v1) * h as f32).floor() as u32, ((1.0 - v0) * h as f32).ceil().min(h as f32) as u32);
            let cropped = img.crop_imm(px0, py0, (px1 - px0).max(1), (py1 - py0).max(1));
            io.set_image(&cropped).map_err(e)?;
            // Le carré unité correspond désormais à la sous-zone [u0,u1]×[v0,v1].
            let su = (px1 - px0) as f32 / w as f32;
            let sv = (py1 - py0) as f32 / h as f32;
            let ou = px0 as f32 / w as f32;
            let ov = 1.0 - py1 as f32 / h as f32;
            let nm = [mm[0] * su, mm[1] * su, mm[2] * sv, mm[3] * sv, mm[0] * ou + mm[2] * ov + mm[4], mm[1] * ou + mm[3] * ov + mm[5]];
            o.reset_matrix(PdfMatrix::new(nm[0], nm[1], nm[2], nm[3], nm[4], nm[5])).map_err(e)?;
        }
        ContentOp::AddImage { rect, path, .. } => {
            let img = image::open(path).map_err(|x| format!("image illisible ({x})"))?;
            let obj = PdfPageImageObject::new(doc, &img).map_err(e)?;
            let mut page = doc.pages().get(page_index as PdfPageIndex).map_err(e)?;
            let g = geom(&page);
            let mut added = page.objects_mut().add_image_object(obj).map_err(e)?;
            added.reset_matrix(image_matrix(&g, rect)).map_err(e)?;
        }
    }
    Ok(())
}

/// Matrice qui place le carré unité d'une image dans un rectangle d'affichage (image droite à l'écran).
fn image_matrix(g: &PageGeom, r: &Rect) -> PdfMatrix {
    let lin = display_matrix(g);
    // Axe x de l'image : vers la droite à l'écran ; axe y : vers le haut à l'écran.
    let (a, b) = (lin[0] * r.w, lin[1] * r.w);
    let (c, d) = (-lin[2] * r.h, -lin[3] * r.h);
    let (e, f) = g.to_user(r.x, r.y + r.h);
    PdfMatrix::new(a, b, c, d, e, f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoupage_avec_mesure() {
        let l = wrap("un deux trois quatre", 30.0, &|s: &str| s.len() as f32 * 3.0);
        assert_eq!(l, vec!["un deux", "trois", "quatre"]);
    }
}
