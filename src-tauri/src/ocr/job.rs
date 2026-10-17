//! Tâche OCR d'un document ouvert : chaque page sans texte est rendue à
//! 300 ppp puis reconnue ; les lignes trouvées sont publiées au fur et à
//! mesure (aperçu dans l'interface). Résultat : un PDF recherchable (texte
//! invisible, pages redressées si demandé) ou un fichier texte.

use std::path::Path;

use serde::Deserialize;
use serde_json::json;

use super::layer::{self, PageOcr};
use super::Tess;
use crate::engine::Engine;
use crate::tools::{Job, Output};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrSpec {
    #[serde(default)]
    pub langs: Vec<String>,
    #[serde(default = "yes")]
    pub searchable: bool,
    #[serde(default)]
    pub deskew: bool,
    /// Reconnaître aussi les pages qui contiennent déjà du texte.
    #[serde(default)]
    pub all_pages: bool,
    /// Écrire le résultat dans le fichier lui-même (création de document).
    #[serde(default)]
    pub in_place: bool,
}

fn yes() -> bool {
    true
}

const DPI: f32 = 300.0;

fn block<T>(f: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(f)
}

pub fn run(engine: &Engine, doc: u32, spec: &OcrSpec, job: &Job) -> Result<Output, String> {
    let (path, password) = block(engine.credentials(doc)).map_err(|e| e.message)?;
    let pages = lopdf_pages(&path, password.as_deref())?;
    let mut tess = Tess::new(&spec.langs)?;
    let mut results: Vec<PageOcr> = Vec::new();
    let mut text = String::new();
    let mut skipped = 0;
    for p in 0..pages {
        job.check()?;
        let data = block(engine.extract(doc, p, Some(DPI), true)).map_err(|e| e.message)?;
        if !spec.all_pages && data.has_text() {
            skipped += 1;
            job.progress(1, p as usize + 1, pages as usize, &json!({ "page": p, "skipped": true }).to_string());
            for l in &data.lines {
                text += &l.text();
                text.push('\n');
            }
            continue;
        }
        let img = data.image.as_ref().ok_or("rendu impossible")?;
        job.progress(0, p as usize, pages as usize, &json!({ "page": p, "start": true }).to_string());
        let (res, quarter, turned) = tess.recognize_oriented(img, img.width as f32 / data.w * 72.0)?;
        let img = turned.as_ref().unwrap_or(img);
        // Aperçu : lignes (texte, boîte relative à la page) dans l'ordre de lecture.
        let mut lines: Vec<(String, [f32; 4], f32)> = Vec::new();
        for w in &res.words {
            let b = [w.x / img.width as f32, w.y / img.height as f32, w.w / img.width as f32, w.h / img.height as f32];
            match lines.get_mut(w.line as usize) {
                Some(l) => {
                    l.0 = format!("{} {}", l.0, w.text);
                    let x1 = (l.1[0] + l.1[2]).max(b[0] + b[2]);
                    let y1 = (l.1[1] + l.1[3]).max(b[1] + b[3]);
                    l.1[0] = l.1[0].min(b[0]);
                    l.1[1] = l.1[1].min(b[1]);
                    l.1[2] = x1 - l.1[0];
                    l.1[3] = y1 - l.1[1];
                    l.2 = (l.2 + w.conf) / 2.0;
                }
                None => {
                    while lines.len() < w.line as usize {
                        lines.push((String::new(), [0.0; 4], 0.0));
                    }
                    lines.push((w.text.clone(), b, w.conf));
                }
            }
        }
        lines.retain(|l| !l.0.is_empty());
        for l in &lines {
            text += &l.0;
            text.push('\n');
        }
        text.push('\n');
        // Aperçu affiché sur la page telle qu'elle était : boîtes ramenées dans son sens.
        let unturn = |b: &[f32; 4]| -> [f32; 4] {
            let [x, y, w, h] = *b;
            match quarter % 4 {
                1 => [y, 1.0 - x - w, h, w],
                2 => [1.0 - x - w, 1.0 - y - h, w, h],
                3 => [1.0 - y - h, x, h, w],
                _ => *b,
            }
        };
        let preview: Vec<_> = lines.iter().map(|(t, b, c)| json!({ "t": t, "b": unturn(b), "c": c })).collect();
        job.progress(1, p as usize + 1, pages as usize, &json!({ "page": p, "lines": preview, "conf": res.conf, "skew": res.skew, "rotate": quarter as u32 * 90 }).to_string());
        results.push(PageOcr { page: p, words: res.words, img_w: img.width, img_h: img.height, skew: res.skew, quarter });
    }
    job.check()?;
    job.progress(2, pages as usize, pages as usize, &json!({ "writing": true, "skipped": skipped }).to_string());
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    if !spec.searchable {
        let out = crate::convert::free_path_ext(&dir, &format!("{stem} (texte)"), "txt");
        let t = text.replace('\n', "\r\n");
        std::fs::write(&out, t.as_bytes()).map_err(|e| format!("écriture impossible ({e})"))?;
        return Ok(Output { path: out.to_string_lossy().into(), size: t.len() as u64, pages });
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let mut doc_l = match password.as_deref() {
        Some(p) => lopdf::Document::load_mem_with_options(&bytes, lopdf::LoadOptions::with_password(p)),
        None => lopdf::Document::load_mem(&bytes),
    }
    .map_err(|e| format!("lecture du PDF : {e}"))?;
    let id = doc_l.trailer.get(b"ID").ok().cloned();
    let state = doc_l.encryption_state.take();
    crate::security::strip_xref_streams(&mut doc_l);
    layer::apply(&mut doc_l, &results, spec.deskew)?;
    doc_l.compress();
    let mut out_bytes = Vec::new();
    doc_l.save_to(&mut out_bytes).map_err(|e| format!("écriture impossible ({e})"))?;
    if let Some(s) = state {
        out_bytes = crate::edit::reencrypt(&out_bytes, &s, id)?;
    }
    let out = if spec.in_place { path.clone() } else { crate::tools::free_path(&dir, &format!("{stem} (OCR)")) };
    crate::edit::write_atomic(&out, &out_bytes).map_err(|e| e.message)?;
    Ok(Output { path: out.to_string_lossy().into(), size: out_bytes.len() as u64, pages })
}

fn lopdf_pages(path: &Path, password: Option<&str>) -> Result<u32, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let d = match password {
        Some(p) => lopdf::Document::load_mem_with_options(&bytes, lopdf::LoadOptions::with_password(p)),
        None => lopdf::Document::load_mem(&bytes),
    }
    .map_err(|e| format!("lecture du PDF : {e}"))?;
    Ok(d.get_pages().len() as u32)
}
