//! Reconnaissance de texte hors ligne (Tesseract 5, données « tessdata_fast »
//! français, anglais, allemand, néerlandais fournies avec l'application).
//!
//! Tesseract et Leptonica sont liés statiquement (vcpkg) : appel direct de
//! l'API C, sans processus externe ni DLL supplémentaire.

mod ffi;
pub mod job;
pub mod layer;
#[cfg(test)]
mod tests;

use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::Serialize;

use crate::convert::model::{Line, PageImage, Span};

pub const LANGS: [&str; 4] = ["fra", "eng", "deu", "nld"];

static DATA: OnceLock<PathBuf> = OnceLock::new();

/// Dossier des données de langue (fixé au démarrage).
pub fn set_data_dir(p: PathBuf) {
    // Tesseract ne comprend pas les chemins étendus « \\?\ » de Windows.
    let s = p.to_string_lossy().to_string();
    let _ = DATA.set(PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s)));
}

fn data_dir() -> PathBuf {
    DATA.get().cloned().unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join("tessdata"))
}

/// Langues disponibles (fichiers présents).
pub fn available_langs() -> Vec<String> {
    LANGS.iter().filter(|l| data_dir().join(format!("{l}.traineddata")).exists()).map(|l| l.to_string()).collect()
}

/// Mot reconnu, en pixels de l'image.
#[derive(Debug, Clone, Serialize)]
pub struct Word {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub conf: f32,
    /// Indice de la ligne dans la page.
    pub line: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct PageResult {
    pub words: Vec<Word>,
    /// Inclinaison du texte (degrés, sens trigonométrique).
    pub skew: f32,
    /// Confiance moyenne (0 à 100).
    pub conf: f32,
    /// Orientation du texte selon Tesseract (0 droit, 1 haut vers la droite,
    /// 2 tête en bas, 3 haut vers la gauche).
    pub orientation: i32,
}

/// Moteur Tesseract initialisé pour un jeu de langues (à réutiliser page après page).
pub struct Tess {
    api: *mut ffi::TessBaseAPI,
}

// L'instance n'est utilisée que par un thread à la fois.
unsafe impl Send for Tess {}

impl Tess {
    pub fn new(langs: &[String]) -> Result<Tess, String> {
        let avail = available_langs();
        let wanted: Vec<&str> = langs.iter().map(|s| s.as_str()).filter(|l| avail.iter().any(|a| a == l)).collect();
        let lang = if wanted.is_empty() { "fra+eng".to_string() } else { wanted.join("+") };
        let dir = data_dir();
        let dir_c = CString::new(dir.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
        let lang_c = CString::new(lang.as_str()).map_err(|e| e.to_string())?;
        unsafe {
            let api = ffi::TessBaseAPICreate();
            if api.is_null() {
                return Err("moteur OCR indisponible".into());
            }
            if ffi::TessBaseAPIInit3(api, dir_c.as_ptr(), lang_c.as_ptr()) != 0 {
                ffi::TessBaseAPIDelete(api);
                return Err(format!("données de langue introuvables ({lang})"));
            }
            // Segmentation automatique de la page.
            ffi::TessBaseAPISetPageSegMode(api, 3);
            Ok(Tess { api })
        }
    }

    /// Reconnaît une image RVBA.
    pub fn recognize(&mut self, img: &PageImage, dpi: f32) -> Result<PageResult, String> {
        // Niveaux de gris (plus rapide, aussi précis pour du texte).
        let gray: Vec<u8> = img.rgba.chunks_exact(4).map(|p| ((p[0] as u32 * 299 + p[1] as u32 * 587 + p[2] as u32 * 114) / 1000) as u8).collect();
        unsafe {
            ffi::TessBaseAPISetImage(self.api, gray.as_ptr(), img.width as i32, img.height as i32, 1, img.width as i32);
            ffi::TessBaseAPISetSourceResolution(self.api, dpi.round().max(70.0) as i32);
            if ffi::TessBaseAPIRecognize(self.api, std::ptr::null_mut()) != 0 {
                return Err("reconnaissance impossible".into());
            }
            let it = ffi::TessBaseAPIGetIterator(self.api);
            let mut words = Vec::new();
            let mut skew = 0.0f32;
            let mut orientation = 0;
            let mut line = 0u32;
            let mut first = true;
            if !it.is_null() {
                let pit = ffi::TessResultIteratorGetPageIterator(it);
                // Inclinaison du premier bloc.
                let (mut o, mut wd, mut tlo, mut deskew) = (0i32, 0i32, 0i32, 0f32);
                ffi::TessPageIteratorOrientation(pit, &mut o, &mut wd, &mut tlo, &mut deskew);
                skew = deskew.to_degrees();
                orientation = o;
                loop {
                    if !first && ffi::TessPageIteratorIsAtBeginningOf(pit, ffi::RIL_TEXTLINE) != 0 {
                        line += 1;
                    }
                    first = false;
                    let txt = ffi::TessResultIteratorGetUTF8Text(it, ffi::RIL_WORD);
                    if !txt.is_null() {
                        let text = CStr::from_ptr(txt).to_string_lossy().trim().to_string();
                        ffi::TessDeleteText(txt);
                        let conf = ffi::TessResultIteratorConfidence(it, ffi::RIL_WORD);
                        let (mut l, mut t, mut r, mut b) = (0, 0, 0, 0);
                        if !text.is_empty() && ffi::TessPageIteratorBoundingBox(pit, ffi::RIL_WORD, &mut l, &mut t, &mut r, &mut b) != 0 {
                            words.push(Word { text, x: l as f32, y: t as f32, w: (r - l) as f32, h: (b - t) as f32, conf, line });
                        }
                    }
                    if ffi::TessPageIteratorNext(pit, ffi::RIL_WORD) == 0 {
                        break;
                    }
                }
                ffi::TessResultIteratorDelete(it);
            }
            ffi::TessBaseAPIClear(self.api);
            let conf = if words.is_empty() { 0.0 } else { words.iter().map(|w| w.conf).sum::<f32>() / words.len() as f32 };
            Ok(PageResult { words, skew, conf, orientation })
        }
    }
}

/// Image tournée de `quarter` quarts de tour dans le sens horaire.
pub fn rotate_image(img: &PageImage, quarter: u16) -> PageImage {
    let (w, h) = (img.width as usize, img.height as usize);
    let q = quarter % 4;
    if q == 0 {
        return img.clone();
    }
    let (nw, nh) = if q % 2 == 1 { (h, w) } else { (w, h) };
    let mut out = vec![0u8; nw * nh * 4];
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = match q {
                1 => (h - 1 - y, x),
                2 => (w - 1 - x, h - 1 - y),
                _ => (y, w - 1 - x),
            };
            let s = (y * w + x) * 4;
            let d = (ny * nw + nx) * 4;
            out[d..d + 4].copy_from_slice(&img.rgba[s..s + 4]);
        }
    }
    PageImage { width: nw as u32, height: nh as u32, rgba: out }
}

/// Qualité d'une lecture : confiance moyenne pondérée par la quantité de texte
/// lu avec assurance (une page tournée donne peu de mots, et peu sûrs).
fn score(r: &PageResult) -> f32 {
    let sure: usize = r.words.iter().filter(|w| w.conf >= 60.0).map(|w| w.text.chars().filter(|c| c.is_alphanumeric()).count()).sum();
    r.conf * sure as f32
}

impl Tess {
    /// Reconnaissance avec détection de l'orientation : si la lecture est
    /// médiocre, la page est essayée tournée d'un quart, d'un demi et de trois
    /// quarts de tour. Renvoie le résultat, la rotation retenue (quarts de tour
    /// horaires) et l'image correspondante.
    pub fn recognize_oriented(&mut self, img: &PageImage, dpi: f32) -> Result<(PageResult, u16, Option<PageImage>), String> {
        let first = self.recognize(img, dpi)?;
        // Texte couché : Tesseract le lit mais signale son orientation ; la page
        // est relue redressée (le sens exact est vérifié par la qualité de lecture).
        // Mots couchés : boîtes plus hautes que larges pour la plupart des mots longs.
        let long: Vec<&Word> = first.words.iter().filter(|w| w.text.chars().count() >= 4).collect();
        let vertical = long.len() >= 3 && long.iter().filter(|w| w.h > w.w * 1.2).count() * 2 > long.len();
        let candidates: Vec<u16> = match first.orientation {
            1 => vec![3, 1],
            3 => vec![1, 3],
            2 => vec![2],
            _ if vertical => vec![1, 3],
            _ if first.conf >= 75.0 && first.words.len() >= 8 => return Ok((first, 0, None)),
            _ => vec![2, 1, 3],
        };
        let lying = vertical || first.orientation == 1 || first.orientation == 3;
        let mut best = (if lying { 0.0 } else { score(&first) }, first, 0u16, None);
        for q in candidates {
            let turned = rotate_image(img, q);
            let r = self.recognize(&turned, dpi)?;
            let s = score(&r);
            // Une autre orientation doit être nettement meilleure.
            if s > best.0 * if best.2 == 0 && !lying { 1.5 } else { 1.0 } && s > 0.0 {
                best = (s, r, q, Some(turned));
            }
        }
        Ok((best.1, best.2, best.3))
    }
}

impl Drop for Tess {
    fn drop(&mut self) {
        unsafe {
            ffi::TessBaseAPIEnd(self.api);
            ffi::TessBaseAPIDelete(self.api);
        }
    }
}

/// Mots → lignes (en points d'affichage de la page), pour la conversion.
pub fn to_lines(res: &PageResult, scale: f32) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    let mut cur: Option<(u32, Vec<Span>)> = None;
    for w in &res.words {
        let size = (w.h / scale * 0.85).clamp(4.0, 72.0);
        let span = Span { text: w.text.clone(), x: w.x / scale, y: w.y / scale, w: w.w / scale, h: w.h / scale, size, bold: false, italic: false, color: "000000".into(), family: "sans".into() };
        match cur.as_mut() {
            Some((l, spans)) if *l == w.line => spans.push(span),
            _ => {
                if let Some((_, spans)) = cur.take() {
                    lines.push(Line { spans });
                }
                cur = Some((w.line, vec![span]));
            }
        }
    }
    if let Some((_, spans)) = cur {
        lines.push(Line { spans });
    }
    // Mots d'une même ligne : un seul segment, séparés par des espaces.
    for l in &mut lines {
        let mut merged: Vec<Span> = Vec::new();
        for s in l.spans.drain(..) {
            if let Some(m) = merged.last_mut() {
                if s.x - (m.x + m.w) < m.size * 1.5 {
                    m.text = format!("{} {}", m.text, s.text);
                    let right = s.x + s.w;
                    m.w = right - m.x;
                    m.h = m.h.max(s.h);
                    continue;
                }
            }
            merged.push(s);
        }
        l.spans = merged;
    }
    lines
}

/// Fonction OCR pour la conversion (pages sans texte) : lignes, et rotation
/// appliquée pour lire la page (quarts de tour horaires).
pub fn convert_ocr(img: &PageImage, dpi: f32, langs: &[String]) -> Result<(Vec<Line>, u16), String> {
    let mut e = Tess::new(langs)?;
    let (res, q, _) = e.recognize_oriented(img, dpi)?;
    Ok((to_lines(&res, dpi / 72.0), q))
}
