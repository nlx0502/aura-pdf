//! Outil Convertir : PDF vers images (PNG, JPEG), Word, Excel, texte ; images et
//! Word vers PDF. Tâche annulable avec progression (fichier, page).
//!
//! Le rendu et l'extraction du texte passent par le moteur (PDFium), une page
//! à la fois : l'affichage reste fluide pendant une conversion. Les pages sans
//! texte peuvent être reconnues par l'OCR.

mod docx;
pub mod model;
mod office;
mod render_docx;
pub mod scan;
#[cfg(test)]
mod tests;
mod xlsx;

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::engine::Engine;
use crate::tools::{Job, Output};
use model::{Line, PageData, PageImage};

/// Reconnaissance de texte d'une image de page (dpi, langues) → lignes en points.
pub use office::available as office_available;
pub use render_docx::text_to_pdf;

pub type OcrFn<'a> = &'a (dyn Fn(&PageImage, f32, &[String]) -> Result<Vec<Line>, String> + Sync);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertSpec {
    /// png, jpeg, docx, xlsx, txt (depuis un PDF) ; pdf (vers un PDF).
    pub target: String,
    #[serde(default = "default_dpi")]
    pub dpi: u32,
    #[serde(default)]
    pub keep_layout: bool,
    #[serde(default)]
    pub auto_ocr: bool,
    #[serde(default)]
    pub per_page: bool,
    #[serde(default)]
    pub langs: Vec<String>,
}

fn default_dpi() -> u32 {
    300
}

/// Archive zip (documents Office).
pub(crate) fn zipfile(files: &[(&str, String)]) -> Result<Vec<u8>, String> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, content) in files {
            z.start_file(*name, opts).map_err(|e| e.to_string())?;
            z.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
        }
        z.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

/// Chemin libre « nom.ext », sinon « nom (2).ext »…
pub fn free_path_ext(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let clean: String = stem.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) { '-' } else { c }).collect();
    let clean = clean.trim().trim_end_matches('.').to_string();
    let clean = if clean.is_empty() { "Document".to_string() } else { clean };
    let first = dir.join(format!("{clean}.{ext}"));
    if !first.exists() {
        return first;
    }
    (2..).map(|n| dir.join(format!("{clean} ({n}).{ext}"))).find(|p| !p.exists()).unwrap()
}

fn block<T>(f: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(f)
}

fn stem_of(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Document".into())
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// Convertit chaque fichier ; renvoie les fichiers (ou dossiers) produits.
pub fn run(engine: &Engine, files: &[String], spec: &ConvertSpec, ocr: Option<OcrFn>, job: &Job) -> Result<Vec<Output>, String> {
    let mut outputs = Vec::new();
    for (i, f) in files.iter().enumerate() {
        job.check()?;
        let path = PathBuf::from(f);
        job.progress(i, 0, 1, &name_of(&path));
        let out = if spec.target == "pdf" { to_pdf(engine, &path, i, job)? } else { from_pdf(engine, &path, spec, ocr, i, job)? };
        outputs.push(out);
    }
    Ok(outputs)
}

// ---------------------------------------------------------------- PDF → …

fn from_pdf(engine: &Engine, path: &Path, spec: &ConvertSpec, ocr: Option<OcrFn>, step: usize, job: &Job) -> Result<Output, String> {
    let name = name_of(path);
    let info = block(engine.open(path.to_path_buf(), None)).map_err(|e| format!("« {name} » : {}", e.message))?;
    let doc = info.id;
    let res = (|| {
        let n = info.page_count;
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let stem = stem_of(path);
        let image_target = spec.target == "png" || spec.target == "jpeg";
        let ocr_dpi = 300.0;
        let mut pages: Vec<(u32, PageData)> = Vec::new();
        // Dossier de sortie quand chaque page donne un fichier.
        let per_page = image_target || spec.per_page;
        let folder = if per_page && n > 1 {
            let suffix = if image_target { "images" } else { spec.target.as_str() };
            let d = free_dir(&dir, &format!("{stem} ({suffix})"));
            std::fs::create_dir_all(&d).map_err(|e| format!("dossier impossible à créer ({e})"))?;
            Some(d)
        } else {
            None
        };
        let mut written = 0u64;
        let mut files = 0u32;
        let mut single: Option<PathBuf> = None;
        for p in 0..n {
            job.check()?;
            job.progress(step, p as usize, n as usize, &name);
            let dpi = if image_target { Some(spec.dpi.clamp(36, 1200) as f32) } else { None };
            let mut data = block(engine.extract(doc, p, dpi, !image_target)).map_err(|e| e.message)?;
            if !image_target && spec.auto_ocr && !data.has_text() {
                if let Some(ocr) = ocr {
                    let img = block(engine.extract(doc, p, Some(ocr_dpi), false)).map_err(|e| e.message)?;
                    if let Some(im) = &img.image {
                        // Résolution effective de l'image rendue (pixels par pouce de page).
                        data.lines = ocr(im, im.width as f32 / img.w * 72.0, &spec.langs)?;
                    }
                }
            }
            if image_target {
                let img = data.image.take().ok_or("rendu impossible")?;
                let target = match &folder {
                    Some(d) => d.join(format!("{stem} - page {:0width$}.{}", p + 1, ext(&spec.target), width = digits(n))),
                    None => free_path_ext(&dir, &stem, ext(&spec.target)),
                };
                written += save_image(&img, &target, &spec.target)?;
                single = Some(target);
                files += 1;
            } else if per_page && folder.is_some() {
                let d = folder.as_ref().unwrap();
                let target = d.join(format!("{stem} - page {:0width$}.{}", p + 1, ext(&spec.target), width = digits(n)));
                written += write_text_target(&spec.target, &[(p, &data)], spec.keep_layout, &stem, &target)?;
                files += 1;
            } else {
                pages.push((p, data));
            }
        }
        job.progress(step, n as usize, n as usize, &name);
        if let Some(d) = folder {
            return Ok(Output { path: d.to_string_lossy().into(), size: written, pages: files });
        }
        if let (true, Some(target)) = (image_target, single) {
            // Document d'une seule page : le fichier image lui-même.
            return Ok(Output { path: target.to_string_lossy().into(), size: written, pages: 1 });
        }
        let target = free_path_ext(&dir, &stem, ext(&spec.target));
        let refs: Vec<(u32, &PageData)> = pages.iter().map(|(p, d)| (*p, d)).collect();
        let size = write_text_target(&spec.target, &refs, spec.keep_layout, &stem, &target)?;
        Ok(Output { path: target.to_string_lossy().into(), size, pages: n })
    })();
    let _ = engine.close(doc);
    res
}

fn digits(n: u32) -> usize {
    n.to_string().len().max(2)
}

fn ext(target: &str) -> &'static str {
    match target {
        "jpeg" => "jpg",
        "png" => "png",
        "docx" => "docx",
        "xlsx" => "xlsx",
        _ => "txt",
    }
}

fn free_dir(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    (2..).map(|n| dir.join(format!("{name} {n}"))).find(|p| !p.exists()).unwrap()
}

fn save_image(img: &PageImage, target: &Path, kind: &str) -> Result<u64, String> {
    let rgba = image::RgbaImage::from_raw(img.width, img.height, img.rgba.clone()).ok_or("image invalide")?;
    let file = std::fs::File::create(target).map_err(|e| format!("écriture impossible ({e})"))?;
    let mut w = std::io::BufWriter::new(file);
    if kind == "jpeg" {
        let rgb = image::DynamicImage::ImageRgba8(rgba).to_rgb8();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut w, 92).encode_image(&rgb).map_err(|e| e.to_string())?;
    } else {
        rgba.write_to(&mut w, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    }
    drop(w);
    Ok(std::fs::metadata(target).map(|m| m.len()).unwrap_or(0))
}

fn write_text_target(target: &str, pages: &[(u32, &PageData)], keep_layout: bool, title: &str, out: &Path) -> Result<u64, String> {
    match target {
        "docx" => {
            let owned: Vec<PageData> = pages.iter().map(|(_, d)| (*d).clone()).collect();
            docx::write(&owned, keep_layout, title, out)
        }
        "xlsx" => xlsx::write(pages, out),
        _ => {
            let text = plain_text(pages.iter().map(|(_, d)| *d), keep_layout);
            std::fs::write(out, text.as_bytes()).map_err(|e| format!("écriture impossible ({e})"))?;
            Ok(text.len() as u64)
        }
    }
}

/// Texte brut : une ligne par ligne du PDF ; paragraphes séparés par une ligne vide.
pub fn plain_text<'a>(pages: impl Iterator<Item = &'a PageData>, keep_layout: bool) -> String {
    let mut out = String::new();
    for (pi, p) in pages.enumerate() {
        if pi > 0 {
            out += if keep_layout { "\n\u{c}\n" } else { "\n\n" };
        }
        let mut prev: Option<&Line> = None;
        for l in &p.lines {
            if let Some(pl) = prev {
                if l.y() - pl.bottom() > pl.size().max(6.0) * 0.8 {
                    out.push('\n');
                }
            }
            let t = l.text();
            if keep_layout {
                // Retrait approximatif en espaces.
                let indent = ((l.x() - p.lines.iter().map(|l| l.x()).fold(f32::MAX, f32::min)) / (l.size().max(6.0) * 0.5)).round().clamp(0.0, 80.0) as usize;
                out += &" ".repeat(indent);
            }
            out += &t.replace('\t', if keep_layout { "    " } else { " " });
            out.push('\n');
            prev = Some(l);
        }
    }
    out.replace("\r\n", "\n").replace('\n', "\r\n")
}

// ---------------------------------------------------------------- … → PDF

/// Document Office en PDF : par l'application Office si elle est installée,
/// sinon par le rendu intégré (Word .docx uniquement).
pub fn document_to_pdf(path: &Path, out: &Path, job: &Job) -> Result<Output, String> {
    let name = name_of(path);
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match office::app_for(&ext) {
        Some(app) if office::available(app) => office::to_pdf(path, out, job).map_err(|e| format!("« {name} » : {e}"))?,
        Some(_) if ext == "docx" => render_docx::to_pdf(path, out).map_err(|e| format!("« {name} » : {e}"))?,
        Some(app) => return Err(format!("« {name} » : ce format nécessite Microsoft {app} (sans Office, seuls les documents Word .docx sont convertis).")),
        None => return Err(format!("« {name} » : format non pris en charge.")),
    }
    let size = std::fs::metadata(out).map(|m| m.len()).unwrap_or(0);
    let pages = lopdf::Document::load(out).map(|d| d.get_pages().len() as u32).unwrap_or(0);
    Ok(Output { path: out.to_string_lossy().into(), size, pages })
}

const IMAGE_EXT: [&str; 9] = ["png", "jpg", "jpeg", "tif", "tiff", "bmp", "gif", "webp", "jfif"];

fn to_pdf(engine: &Engine, path: &Path, step: usize, job: &Job) -> Result<Output, String> {
    let name = name_of(path);
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let out = crate::tools::free_path(&dir, &stem_of(path));
    job.progress(step, 0, 1, &name);
    if IMAGE_EXT.contains(&ext.as_str()) {
        let spec = crate::create::CreateSpec::Scans { paths: vec![path.to_string_lossy().into()] };
        block(engine.create(spec, out.clone())).map_err(|e| format!("« {name} » : {}", e.message))?;
    } else {
        document_to_pdf(path, &out, job)?;
    }
    job.progress(step, 1, 1, &name);
    let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    let pages = lopdf::Document::load(&out).map(|d| d.get_pages().len() as u32).unwrap_or(0);
    Ok(Output { path: out.to_string_lossy().into(), size, pages })
}
