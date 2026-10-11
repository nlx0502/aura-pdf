//! Création de nouveaux PDF : pages vierges (avec fond) et assemblage d'images.

use std::path::Path;

use pdfium_render::prelude::*;
use serde::Deserialize;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PaperFormat {
    A4,
    A5,
    A3,
    Letter,
}

impl PaperFormat {
    /// Dimensions portrait en points.
    pub fn size_pt(self) -> (f32, f32) {
        match self {
            PaperFormat::A4 => (595.28, 841.89),
            PaperFormat::A5 => (419.53, 595.28),
            PaperFormat::A3 => (841.89, 1190.55),
            PaperFormat::Letter => (612.0, 792.0),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PageStyle {
    Blank,
    Lines,
    Grid,
    Dots,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CreateSpec {
    #[serde(rename_all = "camelCase")]
    Blank {
        format: PaperFormat,
        landscape: bool,
        pages: u32,
        style: PageStyle,
    },
    #[serde(rename_all = "camelCase")]
    Images {
        paths: Vec<String>,
        format: PaperFormat,
        landscape: bool,
        one_per_page: bool,
    },
}

fn page_dims(format: PaperFormat, landscape: bool) -> (f32, f32) {
    let (w, h) = format.size_pt();
    if landscape {
        (h, w)
    } else {
        (w, h)
    }
}

const MM: f32 = 72.0 / 25.4;

fn decorate(page: &mut PdfPage, style: PageStyle, w: f32, h: f32) -> Result<(), PdfiumError> {
    let line = PdfColor::new(207, 216, 234, 255);
    let grid = PdfColor::new(225, 224, 218, 255);
    let dot = PdfColor::new(201, 199, 190, 255);
    let objects = page.objects_mut();
    match style {
        PageStyle::Blank => {}
        PageStyle::Lines => {
            let step = 8.0 * MM;
            let mut y = h - 25.0 * MM;
            while y > 15.0 * MM {
                objects.create_path_object_line(
                    PdfPoints::new(15.0 * MM),
                    PdfPoints::new(y),
                    PdfPoints::new(w - 15.0 * MM),
                    PdfPoints::new(y),
                    line,
                    PdfPoints::new(0.6),
                )?;
                y -= step;
            }
        }
        PageStyle::Grid => {
            let step = 5.0 * MM;
            let mut x = step;
            while x < w {
                objects.create_path_object_line(PdfPoints::new(x), PdfPoints::new(0.0), PdfPoints::new(x), PdfPoints::new(h), grid, PdfPoints::new(0.4))?;
                x += step;
            }
            let mut y = step;
            while y < h {
                objects.create_path_object_line(PdfPoints::new(0.0), PdfPoints::new(y), PdfPoints::new(w), PdfPoints::new(y), grid, PdfPoints::new(0.4))?;
                y += step;
            }
        }
        PageStyle::Dots => {
            let step = 5.0 * MM;
            let mut y = step;
            while y < h {
                let mut x = step;
                while x < w {
                    objects.create_path_object_circle_at(PdfPoints::new(x), PdfPoints::new(y), PdfPoints::new(0.55), None, None, Some(dot))?;
                    x += step;
                }
                y += step;
            }
        }
    }
    Ok(())
}

pub fn create(pdfium: &Pdfium, spec: &CreateSpec, out: &Path) -> AppResult<()> {
    let mut doc = pdfium.create_new_pdf().map_err(AppError::internal)?;
    match spec {
        CreateSpec::Blank { format, landscape, pages, style } => {
            let (w, h) = page_dims(*format, *landscape);
            for _ in 0..(*pages).clamp(1, 500) {
                let mut page = doc
                    .pages_mut()
                    .create_page_at_end(PdfPagePaperSize::new_custom(PdfPoints::new(w), PdfPoints::new(h)))
                    .map_err(AppError::internal)?;
                decorate(&mut page, *style, w, h).map_err(AppError::internal)?;
            }
        }
        CreateSpec::Images { paths, format, landscape, one_per_page } => {
            if paths.is_empty() {
                return Err(AppError::new("no_images", "Ajoutez au moins une image."));
            }
            let (w, h) = page_dims(*format, *landscape);
            let margin = 12.0 * MM;
            let per_page = if *one_per_page { 1 } else { 4 };
            for chunk in paths.chunks(per_page) {
                let mut page = doc
                    .pages_mut()
                    .create_page_at_end(PdfPagePaperSize::new_custom(PdfPoints::new(w), PdfPoints::new(h)))
                    .map_err(AppError::internal)?;
                // Grille 1×1 ou 2×2 selon le mode.
                let (cols, rows) = if chunk.len() == 1 { (1, 1) } else { (2, 2) };
                let gap = 6.0 * MM;
                let cell_w = (w - 2.0 * margin - gap * (cols as f32 - 1.0)) / cols as f32;
                let cell_h = (h - 2.0 * margin - gap * (rows as f32 - 1.0)) / rows as f32;
                for (i, p) in chunk.iter().enumerate() {
                    let img = image::open(p).map_err(|e| {
                        AppError::new("image", format!("Impossible de lire l’image « {} » ({e}).", file_name(p)))
                    })?;
                    let (iw, ih) = (img.width() as f32, img.height() as f32);
                    let scale = (cell_w / iw).min(cell_h / ih);
                    let (dw, dh) = (iw * scale, ih * scale);
                    let col = (i % cols) as f32;
                    let row = (i / cols) as f32;
                    let x = margin + col * (cell_w + gap) + (cell_w - dw) / 2.0;
                    let y_top = h - margin - row * (cell_h + gap) - (cell_h - dh) / 2.0;
                    page.objects_mut()
                        .create_image_object(PdfPoints::new(x), PdfPoints::new(y_top - dh), &img, Some(PdfPoints::new(dw)), Some(PdfPoints::new(dh)))
                        .map_err(AppError::internal)?;
                }
            }
        }
    }
    doc.save_to_file(out).map_err(|e| AppError::new("save", format!("Impossible d’enregistrer le fichier ({e}).")))
}

fn file_name(p: &str) -> String {
    Path::new(p).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_paysage() {
        let (w, h) = page_dims(PaperFormat::A4, true);
        assert!(w > h);
    }

    #[test]
    fn deserialisation_spec() {
        let s: CreateSpec = serde_json::from_str(r#"{"kind":"blank","format":"a4","landscape":false,"pages":3,"style":"grid"}"#).unwrap();
        assert!(matches!(s, CreateSpec::Blank { pages: 3, style: PageStyle::Grid, .. }));
    }
}
