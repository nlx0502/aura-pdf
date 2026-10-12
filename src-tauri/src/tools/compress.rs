//! Compression : analyse du poids par catégorie, rééchantillonnage et
//! recompression des images, sous-ensembles et retrait des polices standard,
//! nettoyage (métadonnées, miniatures, pièces jointes), déduplication et
//! flux d'objets.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::Path;

use image::{DynamicImage, GenericImageView};
use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};

use super::{load, write, Job, Output};
use crate::annot::pdf::{deref, get, num};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressOptions {
    /// Résolution maximale des images (ppp).
    pub dpi: u32,
    /// Qualité JPEG (30 à 100).
    pub quality: u8,
    pub gray: bool,
    /// Sous-ensembles des polices intégrées complètes.
    pub subset: bool,
    /// Retirer les polices standard intégrées (Helvetica, Times, Courier…).
    pub unembed: bool,
    pub meta: bool,
    pub thumbs: bool,
    pub attach: bool,
    /// Structure compacte (flux d'objets et table de références compressée).
    pub compact: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInfo {
    pub bytes: u64,
    pub w: u32,
    pub h: u32,
    pub comps: u32,
    pub jpeg: bool,
    /// Résolution affichée la plus fine (ppp), si l'image est dessinée sur une page.
    pub ppi: Option<f32>,
    /// Format que la compression ne sait pas réécrire.
    pub fixed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontInfo {
    pub bytes: u64,
    /// Police standard (retirable).
    pub standard: bool,
    /// Police complète réductible en sous-ensemble.
    pub subsettable: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub total: u64,
    pub pages: u32,
    pub images: u64,
    pub fonts: u64,
    pub content: u64,
    pub meta: u64,
    /// Parties de « meta » : miniatures et pièces jointes.
    pub thumbs: u64,
    pub attach: u64,
    /// Flux non compressés (gain attendu de la compression Flate).
    pub uncompressed: u64,
    pub objects: u32,
    pub image_list: Vec<ImageInfo>,
    pub font_list: Vec<FontInfo>,
}

const STANDARD: [&str; 14] = [
    "Helvetica", "Helvetica-Bold", "Helvetica-Oblique", "Helvetica-BoldOblique", "Times-Roman", "Times-Bold", "Times-Italic", "Times-BoldItalic", "Courier", "Courier-Bold", "Courier-Oblique", "Courier-BoldOblique", "Symbol", "ZapfDingbats",
];

fn base_name(name: &[u8]) -> String {
    let s = String::from_utf8_lossy(name).to_string();
    match s.split_once('+') {
        Some((pre, rest)) if pre.len() == 6 && pre.chars().all(|c| c.is_ascii_uppercase()) => rest.to_string(),
        _ => s,
    }
}

fn is_subset(name: &[u8]) -> bool {
    let s = String::from_utf8_lossy(name);
    s.len() > 7 && s.as_bytes()[6] == b'+' && s[..6].chars().all(|c| c.is_ascii_uppercase())
}

fn obj_bytes(o: &Object) -> u64 {
    match o {
        Object::Stream(s) => (s.content.len() + 40 + s.dict.len() * 14) as u64,
        Object::Dictionary(d) => (20 + d.len() * 14) as u64,
        Object::Array(a) => (10 + a.len() * 6) as u64,
        _ => 12,
    }
}

type M = [f32; 6];
fn mul(a: &M, b: &M) -> M {
    [a[0] * b[0] + a[1] * b[2], a[0] * b[1] + a[1] * b[3], a[2] * b[0] + a[3] * b[2], a[2] * b[1] + a[3] * b[3], a[4] * b[0] + a[5] * b[2] + b[4], a[4] * b[1] + a[5] * b[3] + b[5]]
}

fn stream_data(s: &Stream) -> Option<Vec<u8>> {
    if s.dict.has(b"Filter") {
        s.decompressed_content().ok()
    } else {
        Some(s.content.clone())
    }
}

/// Parcourt les contenus (pages, formulaires, apparences) en suivant la matrice
/// courante : taille affichée de chaque image, codes utilisés par police.
struct Walker<'a> {
    doc: &'a Document,
    /// Image → plus grande taille affichée (points, largeur et hauteur).
    shown: HashMap<ObjectId, (f32, f32)>,
    /// Police → codes utilisés (2 octets) ; None si un flux illisible l'utilise.
    codes: HashMap<ObjectId, Option<BTreeSet<u16>>>,
    visited: HashSet<(ObjectId, [i32; 4])>,
}

impl Walker<'_> {
    fn walk(&mut self, data: &[u8], res: &Dictionary, ctm: M, depth: u32) {
        if depth > 10 {
            return;
        }
        let fonts = get(self.doc, res, b"Font").and_then(|o| o.as_dict().ok());
        let xobjs = get(self.doc, res, b"XObject").and_then(|o| o.as_dict().ok());
        let Ok(content) = Content::decode(data) else {
            // Contenu illisible : ses polices ne doivent pas être réduites.
            if let Some(f) = fonts {
                for (_, v) in f.iter() {
                    if let Ok(id) = v.as_reference() {
                        self.codes.insert(id, None);
                    }
                }
            }
            return;
        };
        let mut stack: Vec<M> = Vec::new();
        let mut cur = ctm;
        let mut font: Option<ObjectId> = None;
        for op in content.operations {
            let o = &op.operands;
            match op.operator.as_str() {
                "q" => stack.push(cur),
                "Q" => cur = stack.pop().unwrap_or(ctm),
                "cm" if o.len() == 6 => {
                    let m: Vec<f32> = o.iter().map(|x| num(x).unwrap_or(0.0)).collect();
                    cur = mul(&[m[0], m[1], m[2], m[3], m[4], m[5]], &cur);
                }
                "Tf" => font = o.first().and_then(|n| n.as_name().ok()).and_then(|n| fonts?.get(n).ok()).and_then(|r| r.as_reference().ok()),
                "Tj" | "TJ" | "'" | "\"" => {
                    let Some(fid) = font else { continue };
                    let entry = self.codes.entry(fid).or_insert_with(|| Some(BTreeSet::new()));
                    let Some(set) = entry.as_mut() else { continue };
                    let mut add = |b: &[u8]| {
                        for c in b.chunks(2) {
                            set.insert(u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]));
                        }
                    };
                    for x in o {
                        match x {
                            Object::String(b, _) => add(b),
                            Object::Array(a) => a.iter().for_each(|y| {
                                if let Object::String(b, _) = y {
                                    add(b)
                                }
                            }),
                            _ => {}
                        }
                    }
                }
                "Do" => {
                    let Some(r) = o.first().and_then(|n| n.as_name().ok()).and_then(|n| xobjs?.get(n).ok()).and_then(|r| r.as_reference().ok()) else { continue };
                    let Ok(Object::Stream(s)) = self.doc.get_object(r) else { continue };
                    match s.dict.get(b"Subtype").and_then(|x| x.as_name()) {
                        Ok(b"Image") => {
                            let w = (cur[0] * cur[0] + cur[1] * cur[1]).sqrt();
                            let h = (cur[2] * cur[2] + cur[3] * cur[3]).sqrt();
                            let e = self.shown.entry(r).or_insert((0.0, 0.0));
                            e.0 = e.0.max(w);
                            e.1 = e.1.max(h);
                        }
                        Ok(b"Form") => {
                            let key = (r, [cur[0] as i32, cur[3] as i32, cur[4] as i32, cur[5] as i32]);
                            if !self.visited.insert(key) {
                                continue;
                            }
                            let fm = get(self.doc, &s.dict, b"Matrix")
                                .and_then(|m| m.as_array().ok())
                                .filter(|a| a.len() == 6)
                                .map(|a| {
                                    let v: Vec<f32> = a.iter().map(|x| num(deref(self.doc, x)).unwrap_or(0.0)).collect();
                                    [v[0], v[1], v[2], v[3], v[4], v[5]]
                                })
                                .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                            let fres = get(self.doc, &s.dict, b"Resources").and_then(|x| x.as_dict().ok()).cloned().unwrap_or_else(|| res.clone());
                            if let Some(d) = stream_data(s) {
                                self.walk(&d, &fres, mul(&fm, &cur), depth + 1);
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

fn walk_document(doc: &Document) -> Walker<'_> {
    let mut w = Walker { doc, shown: HashMap::new(), codes: HashMap::new(), visited: HashSet::new() };
    for (_, pid) in doc.get_pages() {
        let Ok(pd) = doc.get_dictionary(pid) else { continue };
        let res = crate::annot::pdf::inherited(doc, pd, b"Resources").and_then(|r| r.as_dict().ok()).cloned().unwrap_or_default();
        let data = doc.get_page_content(pid);
        w.walk(&data, &res, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], 0);
        // Apparences des annotations (elles utilisent aussi des polices).
        if let Some(annots) = get(doc, pd, b"Annots").and_then(|a| a.as_array().ok()) {
            for a in annots {
                let Some(ap) = deref(doc, a).as_dict().ok().and_then(|d| get(doc, d, b"AP")).and_then(|x| x.as_dict().ok()) else { continue };
                for (_, v) in ap.iter() {
                    let streams: Vec<&Stream> = match deref(doc, v) {
                        Object::Stream(s) => vec![s],
                        Object::Dictionary(d) => d.iter().filter_map(|(_, x)| deref(doc, x).as_stream().ok()).collect(),
                        _ => vec![],
                    };
                    for s in streams {
                        let r = get(doc, &s.dict, b"Resources").and_then(|x| x.as_dict().ok()).cloned().unwrap_or_default();
                        if let Some(d) = stream_data(s) {
                            w.walk(&d, &r, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], 1);
                        }
                    }
                }
            }
        }
    }
    w
}

fn components(doc: &Document, cs: Option<&Object>) -> Option<u32> {
    match cs.map(|o| deref(doc, o))? {
        Object::Name(n) => match n.as_slice() {
            b"DeviceGray" | b"CalGray" => Some(1),
            b"DeviceRGB" | b"CalRGB" => Some(3),
            b"DeviceCMYK" => Some(4),
            _ => None,
        },
        Object::Array(a) => match a.first().and_then(|x| x.as_name().ok())? {
            b"ICCBased" => deref(doc, a.get(1)?).as_stream().ok()?.dict.get(b"N").ok()?.as_i64().ok().map(|v| v as u32),
            b"CalRGB" => Some(3),
            b"CalGray" => Some(1),
            _ => None,
        },
        _ => None,
    }
}

fn filters(doc: &Document, d: &Dictionary) -> Vec<Vec<u8>> {
    match d.get(b"Filter").ok().map(|o| deref(doc, o)) {
        Some(Object::Name(n)) => vec![n.clone()],
        Some(Object::Array(a)) => a.iter().filter_map(|o| o.as_name().ok().map(|x| x.to_vec())).collect(),
        _ => vec![],
    }
}

fn smask_ids(doc: &Document) -> HashSet<ObjectId> {
    doc.objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter_map(|s| s.dict.get(b"SMask").ok().and_then(|x| x.as_reference().ok()))
        .collect()
}

/// Analyse du poids du document par catégorie.
pub fn analyze(path: &Path) -> Result<Analysis, String> {
    let total = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    let doc = load(path)?;
    let walker = walk_document(&doc);
    let smasks = smask_ids(&doc);
    let mut a = Analysis { total, pages: doc.get_pages().len() as u32, objects: doc.objects.len() as u32, ..Default::default() };
    let mut font_files: HashMap<ObjectId, (bool, bool)> = HashMap::new();
    // Polices : programmes intégrés, standard ou réductibles.
    for o in doc.objects.values() {
        let Ok(d) = o.as_dict() else { continue };
        if d.get(b"Type").and_then(|t| t.as_name()).ok() != Some(b"Font".as_slice()) {
            continue;
        }
        let name = d.get(b"BaseFont").and_then(|n| n.as_name()).unwrap_or(b"");
        let sub = d.get(b"Subtype").and_then(|n| n.as_name()).unwrap_or(b"");
        let (desc, cid_identity) = if sub == b"Type0" {
            let cf = get(&doc, d, b"DescendantFonts").and_then(|x| x.as_array().ok()).and_then(|a| a.first()).and_then(|x| deref(&doc, x).as_dict().ok());
            let ident = cf.map(|c| c.get(b"CIDToGIDMap").and_then(|m| m.as_name()).ok() == Some(b"Identity".as_slice())).unwrap_or(false)
                && cf.and_then(|c| c.get(b"Subtype").and_then(|s| s.as_name()).ok()) == Some(b"CIDFontType2".as_slice());
            (cf.and_then(|c| get(&doc, c, b"FontDescriptor")).and_then(|x| x.as_dict().ok()), ident)
        } else {
            (get(&doc, d, b"FontDescriptor").and_then(|x| x.as_dict().ok()), false)
        };
        let Some(desc) = desc else { continue };
        for key in [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"] {
            if let Ok(Object::Reference(r)) = desc.get(key) {
                let standard = sub != b"Type0" && STANDARD.contains(&base_name(name).as_str()) && d.has(b"Widths");
                let subsettable = cid_identity && key == b"FontFile2" && !is_subset(name);
                let e = font_files.entry(*r).or_insert((false, false));
                e.0 |= standard;
                e.1 |= subsettable;
            }
        }
    }
    let thumbs: HashSet<ObjectId> = doc
        .get_pages()
        .values()
        .filter_map(|p| doc.get_dictionary(*p).ok()?.get(b"Thumb").ok()?.as_reference().ok())
        .collect();
    for (id, o) in &doc.objects {
        let size = obj_bytes(o);
        let Ok(s) = o.as_stream() else {
            a.content += size;
            continue;
        };
        if !s.dict.has(b"Filter") && s.content.len() > 64 {
            a.uncompressed += s.content.len() as u64;
        }
        let t = s.dict.get(b"Type").and_then(|x| x.as_name()).unwrap_or(b"");
        let st = s.dict.get(b"Subtype").and_then(|x| x.as_name()).unwrap_or(b"");
        if thumbs.contains(id) {
            a.thumbs += size;
            a.meta += size;
        } else if t == b"EmbeddedFile" {
            a.attach += size;
            a.meta += size;
        } else if t == b"Metadata" {
            a.meta += size;
        } else if st == b"Image" {
            a.images += size;
            if smasks.contains(id) {
                continue;
            }
            let w = get(&doc, &s.dict, b"Width").and_then(|x| x.as_i64().ok()).unwrap_or(0) as u32;
            let h = get(&doc, &s.dict, b"Height").and_then(|x| x.as_i64().ok()).unwrap_or(0) as u32;
            let f = filters(&doc, &s.dict);
            let jpeg = f.last().map(|x| x.as_slice() == b"DCTDecode").unwrap_or(false);
            let comps = components(&doc, s.dict.get(b"ColorSpace").ok());
            let bpc = get(&doc, &s.dict, b"BitsPerComponent").and_then(|x| x.as_i64().ok()).unwrap_or(8);
            let fixed = comps.is_none() || comps == Some(4) || (bpc != 8 && !jpeg) || f.iter().any(|x| matches!(x.as_slice(), b"JPXDecode" | b"JBIG2Decode" | b"CCITTFaxDecode"));
            let ppi = walker.shown.get(id).map(|(sw, sh)| {
                let px = if *sw > 0.0 { w as f32 / (sw / 72.0) } else { f32::MAX };
                let py = if *sh > 0.0 { h as f32 / (sh / 72.0) } else { f32::MAX };
                px.min(py)
            });
            a.image_list.push(ImageInfo { bytes: size, w, h, comps: comps.unwrap_or(3), jpeg, ppi: ppi.filter(|v| v.is_finite()), fixed });
        } else if let Some((standard, subsettable)) = font_files.get(id) {
            a.fonts += size;
            a.font_list.push(FontInfo { bytes: size, standard: *standard, subsettable: *subsettable });
        } else {
            a.content += size;
        }
    }
    if let Some(info) = doc.trailer.get(b"Info").ok().and_then(|o| doc.dereference(o).ok()).map(|(_, o)| obj_bytes(o)) {
        a.meta += info;
    }
    // L'écart (table de références, en-têtes) est compté dans le contenu.
    let sum = a.images + a.fonts + a.content + a.meta;
    if sum <= total {
        a.content += total - sum;
    } else {
        // Estimations des en-têtes un peu fortes (fichier très compact) : on
        // ramène les catégories à la taille réelle du fichier.
        let k = total as f64 / sum as f64;
        a.images = (a.images as f64 * k) as u64;
        a.fonts = (a.fonts as f64 * k) as u64;
        a.meta = (a.meta as f64 * k) as u64;
        a.content = total - a.images - a.fonts - a.meta;
    }
    Ok(a)
}

/// Décode une image (8 bits par composante, gris ou RGB, JPEG ou sans perte).
fn decode(doc: &Document, s: &Stream) -> Option<(DynamicImage, bool)> {
    let w = get(doc, &s.dict, b"Width")?.as_i64().ok()? as u32;
    let h = get(doc, &s.dict, b"Height")?.as_i64().ok()? as u32;
    let f = filters(doc, &s.dict);
    if f.last().map(|x| x.as_slice() == b"DCTDecode").unwrap_or(false) {
        if f.len() > 1 || components(doc, s.dict.get(b"ColorSpace").ok()) == Some(4) || s.dict.has(b"Decode") {
            return None;
        }
        let img = image::load_from_memory_with_format(&s.content, image::ImageFormat::Jpeg).ok()?;
        return Some((img, true));
    }
    if !f.iter().all(|x| matches!(x.as_slice(), b"FlateDecode" | b"LZWDecode" | b"ASCII85Decode" | b"ASCIIHexDecode" | b"RunLengthDecode")) {
        return None;
    }
    if get(doc, &s.dict, b"BitsPerComponent").and_then(|x| x.as_i64().ok()) != Some(8) || s.dict.has(b"Decode") || s.dict.get(b"ImageMask").and_then(|x| x.as_bool()).unwrap_or(false) {
        return None;
    }
    let comps = components(doc, s.dict.get(b"ColorSpace").ok())?;
    let data = stream_data(s)?;
    let need = (w * h * comps) as usize;
    if data.len() < need {
        return None;
    }
    let img = match comps {
        1 => DynamicImage::ImageLuma8(image::GrayImage::from_raw(w, h, data[..need].to_vec())?),
        3 => DynamicImage::ImageRgb8(image::RgbImage::from_raw(w, h, data[..need].to_vec())?),
        _ => return None,
    };
    Some((img, false))
}

/// Image peu colorée (schéma, capture d'écran) : à garder sans perte.
fn few_colors(img: &DynamicImage) -> bool {
    let (w, h) = img.dimensions();
    let step = ((w * h) as f32 / 4000.0).sqrt().max(1.0) as u32;
    let mut set = HashSet::new();
    let rgb = img.to_rgb8();
    for y in (0..h).step_by(step as usize) {
        for x in (0..w).step_by(step as usize) {
            let p = rgb.get_pixel(x, y);
            set.insert((p[0] / 8, p[1] / 8, p[2] / 8));
            if set.len() > 64 {
                return false;
            }
        }
    }
    true
}

fn jpeg(img: &DynamicImage, quality: u8) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(10, 100));
    match img {
        DynamicImage::ImageLuma8(_) => img.write_with_encoder(enc).ok()?,
        _ => DynamicImage::ImageRgb8(img.to_rgb8()).write_with_encoder(enc).ok()?,
    }
    Some(out)
}

fn rewrite_images(doc: &mut Document, opts: &CompressOptions, job: &Job) -> Result<(), String> {
    let walker = walk_document(doc);
    let shown = walker.shown.clone();
    drop(walker);
    let smasks = smask_ids(doc);
    let ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter(|(id, o)| !smasks.contains(id) && o.as_stream().ok().and_then(|s| s.dict.get(b"Subtype").and_then(|x| x.as_name()).ok()) == Some(b"Image".as_slice()))
        .map(|(id, _)| *id)
        .collect();
    let total = ids.len();
    for (k, id) in ids.into_iter().enumerate() {
        job.check()?;
        job.progress(1, k, total, "");
        let Ok(Object::Stream(s)) = doc.get_object(id) else { continue };
        let Some((img, was_jpeg)) = decode(doc, s) else { continue };
        let (w, h) = img.dimensions();
        // Rééchantillonnage selon la taille affichée la plus grande.
        let scale = shown
            .get(&id)
            .map(|(sw, sh)| {
                let ppi = (w as f32 / (sw / 72.0).max(0.01)).min(h as f32 / (sh / 72.0).max(0.01));
                if ppi > opts.dpi as f32 * 1.15 {
                    opts.dpi as f32 / ppi
                } else {
                    1.0
                }
            })
            .unwrap_or(1.0);
        let mut out = img;
        if scale < 0.98 {
            let nw = ((w as f32 * scale).round() as u32).max(1);
            let nh = ((h as f32 * scale).round() as u32).max(1);
            out = out.resize_exact(nw, nh, image::imageops::FilterType::Triangle);
        }
        if opts.gray && !matches!(out, DynamicImage::ImageLuma8(_)) {
            out = DynamicImage::ImageLuma8(out.to_luma8());
        }
        let gray = matches!(out, DynamicImage::ImageLuma8(_));
        let old_len = s.content.len();
        let lossless = !was_jpeg && few_colors(&out);
        let (data, filter) = if lossless {
            let raw = if gray { out.to_luma8().into_raw() } else { out.to_rgb8().into_raw() };
            let mut st = Stream::new(Dictionary::new(), raw);
            let _ = st.compress();
            (st.content, "FlateDecode")
        } else {
            match jpeg(&out, opts.quality) {
                Some(d) => (d, "DCTDecode"),
                None => continue,
            }
        };
        // Jamais plus lourd que l'original ; gain minimal exigé si rien d'autre ne change.
        if data.len() >= old_len || (data.len() as f32 > old_len as f32 * 0.92 && scale >= 0.98 && !(opts.gray && !gray)) {
            continue;
        }
        let mut dict = s.dict.clone();
        let (nw, nh) = out.dimensions();
        dict.set("Width", nw as i64);
        dict.set("Height", nh as i64);
        dict.set("BitsPerComponent", 8);
        dict.set("ColorSpace", if gray { "DeviceGray" } else { "DeviceRGB" });
        dict.set("Filter", filter);
        for k in [b"DecodeParms".as_slice(), b"Decode", b"Length", b"Alternates"] {
            dict.remove(k);
        }
        let mut ns = Stream::new(dict, data);
        ns.allows_compression = false;
        doc.objects.insert(id, Object::Stream(ns));
    }
    Ok(())
}

fn fonts(doc: &mut Document, opts: &CompressOptions) {
    let walker = if opts.subset { Some(walk_document(doc)) } else { None };
    let codes = walker.map(|w| w.codes).unwrap_or_default();
    let font_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter(|(_, o)| o.as_dict().ok().and_then(|d| d.get(b"Type").and_then(|t| t.as_name()).ok()) == Some(b"Font".as_slice()))
        .map(|(id, _)| *id)
        .collect();
    for fid in font_ids {
        let Ok(d) = doc.get_dictionary(fid).cloned() else { continue };
        let name = d.get(b"BaseFont").and_then(|n| n.as_name()).unwrap_or(b"").to_vec();
        let sub = d.get(b"Subtype").and_then(|n| n.as_name()).unwrap_or(b"").to_vec();
        if opts.unembed && sub != b"Type0" && STANDARD.contains(&base_name(&name).as_str()) && d.has(b"Widths") {
            if let Some(desc) = d.get(b"FontDescriptor").ok().and_then(|x| x.as_reference().ok()) {
                if let Ok(fd) = doc.get_dictionary_mut(desc) {
                    for k in [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"] {
                        fd.remove(k);
                    }
                }
                if let Ok(d) = doc.get_dictionary_mut(fid) {
                    d.set("BaseFont", Object::Name(base_name(&name).into_bytes()));
                }
            }
            continue;
        }
        if !opts.subset || sub != b"Type0" || is_subset(&name) {
            continue;
        }
        // Police composite Identity : identifiants de glyphe = codes utilisés.
        let Some(Some(used)) = codes.get(&fid) else { continue };
        let Some(cf_id) = get(doc, &d, b"DescendantFonts").and_then(|x| x.as_array().ok()).and_then(|a| a.first()).and_then(|x| x.as_reference().ok()) else { continue };
        let Ok(cf) = doc.get_dictionary(cf_id).cloned() else { continue };
        if cf.get(b"CIDToGIDMap").and_then(|m| m.as_name()).ok() != Some(b"Identity".as_slice()) {
            continue;
        }
        let Some(desc_id) = cf.get(b"FontDescriptor").ok().and_then(|x| x.as_reference().ok()) else { continue };
        let Some(ff_id) = doc.get_dictionary(desc_id).ok().and_then(|fd| fd.get(b"FontFile2").ok()).and_then(|x| x.as_reference().ok()) else { continue };
        let Ok(Object::Stream(ff)) = doc.get_object(ff_id) else { continue };
        let Some(data) = stream_data(ff) else { continue };
        let Some(sub_bytes) = crate::sysfonts::subset_bytes(&data, used) else { continue };
        if sub_bytes.len() >= data.len() {
            continue;
        }
        let mut ns = Stream::new(lopdf::dictionary! { "Length1" => sub_bytes.len() as i64 }, sub_bytes);
        let _ = ns.compress();
        doc.objects.insert(ff_id, Object::Stream(ns));
        // Nom de sous-ensemble (convention PDF : préfixe de six lettres).
        let tag = format!("AURA{:02}+", fid.0 % 100);
        let tag: String = tag.chars().map(|c| if c.is_ascii_digit() { (b'A' + (c as u8 - b'0')) as char } else { c }).collect();
        let new_name = Object::Name(format!("{tag}{}", base_name(&name)).into_bytes());
        for (id, key) in [(fid, b"BaseFont".as_slice()), (cf_id, b"BaseFont"), (desc_id, b"FontName")] {
            if let Ok(x) = doc.get_dictionary_mut(id) {
                x.set(key, new_name.clone());
            }
        }
    }
}

fn clean(doc: &mut Document, opts: &CompressOptions) {
    if opts.meta {
        doc.trailer.remove(b"Info");
        if let Ok(c) = doc.catalog_mut() {
            c.remove(b"Metadata");
            c.remove(b"PieceInfo");
        }
    }
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for p in &pages {
        if let Ok(d) = doc.get_dictionary_mut(*p) {
            if opts.thumbs {
                d.remove(b"Thumb");
            }
            if opts.meta {
                d.remove(b"PieceInfo");
                d.remove(b"Metadata");
            }
        }
    }
    if opts.attach {
        let names = doc.catalog().ok().and_then(|c| c.get(b"Names").ok()).cloned();
        match names {
            Some(Object::Reference(r)) => {
                if let Ok(n) = doc.get_dictionary_mut(r) {
                    n.remove(b"EmbeddedFiles");
                }
            }
            Some(Object::Dictionary(mut n)) => {
                n.remove(b"EmbeddedFiles");
                if let Ok(c) = doc.catalog_mut() {
                    c.set("Names", n);
                }
            }
            _ => {}
        }
        for p in &pages {
            let Some(list) = doc.get_dictionary(*p).ok().and_then(|d| get(doc, d, b"Annots")).and_then(|a| a.as_array().ok()).cloned() else { continue };
            let keep: Vec<Object> = list
                .into_iter()
                .filter(|a| deref(doc, a).as_dict().ok().and_then(|d| d.get(b"Subtype").and_then(|s| s.as_name()).ok()) != Some(b"FileAttachment".as_slice()))
                .collect();
            if let Ok(d) = doc.get_dictionary_mut(*p) {
                d.set("Annots", keep);
            }
        }
    }
}

/// Fusionne les flux identiques (images ou polices répétées).
fn dedupe(doc: &mut Document) -> usize {
    let mut seen: HashMap<u64, ObjectId> = HashMap::new();
    let mut remap: HashMap<ObjectId, ObjectId> = HashMap::new();
    for (id, o) in &doc.objects {
        let Ok(s) = o.as_stream() else { continue };
        if s.content.len() < 256 {
            continue;
        }
        let mut h = std::collections::hash_map::DefaultHasher::new();
        s.content.hash(&mut h);
        format!("{:?}", s.dict).hash(&mut h);
        let key = h.finish();
        match seen.get(&key) {
            Some(first) => {
                remap.insert(*id, *first);
            }
            None => {
                seen.insert(key, *id);
            }
        }
    }
    if remap.is_empty() {
        return 0;
    }
    fn fix(o: &mut Object, remap: &HashMap<ObjectId, ObjectId>, depth: u32) {
        if depth > 64 {
            return;
        }
        match o {
            Object::Reference(r) => {
                if let Some(n) = remap.get(r) {
                    *r = *n;
                }
            }
            Object::Array(a) => a.iter_mut().for_each(|x| fix(x, remap, depth + 1)),
            Object::Dictionary(d) => d.iter_mut().for_each(|(_, v)| fix(v, remap, depth + 1)),
            Object::Stream(s) => s.dict.iter_mut().for_each(|(_, v)| fix(v, remap, depth + 1)),
            _ => {}
        }
    }
    for o in doc.objects.values_mut() {
        fix(o, &remap, 0);
    }
    remap.len()
}

pub fn compress(path: &Path, opts: &CompressOptions, out: &Path, job: &Job) -> Result<Output, String> {
    job.progress(0, 0, 1, "");
    let mut doc = load(path)?;
    job.check()?;
    rewrite_images(&mut doc, opts, job)?;
    job.progress(2, 0, 1, "");
    job.check()?;
    fonts(&mut doc, opts);
    job.progress(3, 0, 1, "");
    job.check()?;
    clean(&mut doc, opts);
    let merged = dedupe(&mut doc);
    doc.prune_objects();
    doc.compress();
    job.progress(3, 1, 1, &merged.to_string());
    job.check()?;
    job.progress(4, 0, 1, "");
    let res = write(&mut doc, out, opts.compact)?;
    // Document déjà optimal : on ne livre pas un fichier plus lourd (sauf nettoyage demandé).
    let before = std::fs::metadata(path).map(|m| m.len()).unwrap_or(u64::MAX);
    if res.size > before && !opts.meta && !opts.attach {
        std::fs::copy(path, out).map_err(|e| format!("écriture impossible ({e})"))?;
        return Ok(Output { size: before, ..res });
    }
    Ok(res)
}
