//! Métriques de police pour situer chaque glyphe d'un flux de contenu
//! (caviardage) : découpage des codes, largeurs, hauteur des glyphes.

use std::collections::HashMap;

use lopdf::{Dictionary, Document, Object};

use crate::annot::pdf::{deref, get, num};
use crate::annot::text::char_width_in;

/// Découpage des octets d'une chaîne en codes de caractères.
#[derive(Debug, Clone)]
enum Codes {
    One,
    Two,
    /// Plages d'espace de codes d'un CMap : (longueur, octets mini, octets maxi).
    Ranges(Vec<(usize, Vec<u8>, Vec<u8>)>),
}

#[derive(Debug, Clone)]
pub struct FontMetrics {
    codes: Codes,
    /// Largeur par code (simple) ou par CID (composite), en 1/1000 em.
    widths: HashMap<u32, f32>,
    default_w: f32,
    /// Correspondance code → CID (CMap intégré) ; sinon CID = code.
    cid_map: Option<HashMap<u32, u32>>,
    /// Ascendante et descendante, en 1/1000 em.
    pub ascent: f32,
    pub descent: f32,
    /// Écriture verticale ou codage inconnu : positions approximatives.
    pub uncertain: bool,
    /// Police composite (le mot-espace Tw ne s'applique pas).
    pub composite: bool,
}

impl FontMetrics {
    /// Police inconnue : approximation prudente.
    pub fn fallback() -> Self {
        FontMetrics {
            codes: Codes::One,
            widths: HashMap::new(),
            default_w: 600.0,
            cid_map: None,
            ascent: 900.0,
            descent: -250.0,
            uncertain: true,
            composite: false,
        }
    }

    /// Codes successifs d'une chaîne : (code, nombre d'octets).
    pub fn split(&self, s: &[u8]) -> Vec<(u32, usize)> {
        let mut out = Vec::with_capacity(s.len());
        let mut i = 0;
        while i < s.len() {
            let len = match &self.codes {
                Codes::One => 1,
                Codes::Two => 2,
                Codes::Ranges(r) => {
                    let mut l = 0;
                    for (len, lo, hi) in r {
                        if i + len <= s.len() && (0..*len).all(|k| s[i + k] >= lo[k] && s[i + k] <= hi[k]) {
                            l = *len;
                            break;
                        }
                    }
                    if l == 0 { 1 } else { l }
                }
            }
            .min(s.len() - i);
            let code = s[i..i + len].iter().fold(0u32, |acc, &b| (acc << 8) | b as u32);
            out.push((code, len));
            i += len;
        }
        out
    }

    /// Largeur d'un code, en 1/1000 em.
    pub fn width(&self, code: u32) -> f32 {
        let key = match &self.cid_map {
            Some(m) => m.get(&code).copied().unwrap_or(code),
            None => code,
        };
        self.widths.get(&key).copied().unwrap_or(self.default_w)
    }
}

/// Cache des métriques par dictionnaire de police.
#[derive(Default)]
pub struct FontCache {
    by_id: HashMap<lopdf::ObjectId, FontMetrics>,
}

impl FontCache {
    pub fn get(&mut self, doc: &Document, font: &Object) -> FontMetrics {
        if let Ok(id) = font.as_reference() {
            if let Some(m) = self.by_id.get(&id) {
                return m.clone();
            }
            let m = deref(doc, font).as_dict().map(|d| metrics(doc, d)).unwrap_or_else(|_| FontMetrics::fallback());
            self.by_id.insert(id, m.clone());
            return m;
        }
        font.as_dict().map(|d| metrics(doc, d)).unwrap_or_else(|_| FontMetrics::fallback())
    }
}

fn name<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    get(doc, d, key).and_then(|o| o.as_name().ok())
}

fn descriptor_vertical(doc: &Document, desc: Option<&Dictionary>, scale: f32) -> (f32, f32) {
    let Some(fd) = desc else { return (800.0, -200.0) };
    let mut asc = get(doc, fd, b"Ascent").and_then(num).map(|v| v * scale);
    let mut des = get(doc, fd, b"Descent").and_then(num).map(|v| v * scale);
    if let Some(bb) = get(doc, fd, b"FontBBox").and_then(|o| o.as_array().ok()) {
        if bb.len() == 4 {
            let (y0, y1) = (num(deref(doc, &bb[1])).unwrap_or(-200.0) * scale, num(deref(doc, &bb[3])).unwrap_or(800.0) * scale);
            if asc.map(|a| a <= 0.0).unwrap_or(true) {
                asc = Some(y1);
            }
            if des.map(|d| d >= 0.0).unwrap_or(true) {
                des = Some(y0);
            }
        }
    }
    let a = asc.filter(|a| *a > 0.0).unwrap_or(800.0).clamp(500.0, 1200.0);
    let d = des.filter(|d| *d < 0.0).unwrap_or(-200.0).clamp(-500.0, -50.0);
    (a, d)
}

pub fn metrics(doc: &Document, d: &Dictionary) -> FontMetrics {
    let subtype = name(doc, d, b"Subtype").unwrap_or(b"Type1");
    if subtype == b"Type0" {
        return composite(doc, d);
    }
    let mut widths = HashMap::new();
    let first = get(doc, d, b"FirstChar").and_then(|o| o.as_i64().ok()).unwrap_or(0) as u32;
    // Type 3 : largeurs en espace glyphe, ramenées en 1/1000 em par la FontMatrix.
    let scale = if subtype == b"Type3" {
        get(doc, d, b"FontMatrix")
            .and_then(|o| o.as_array().ok())
            .and_then(|a| a.first().and_then(|x| num(deref(doc, x))))
            .map(|a| a * 1000.0)
            .unwrap_or(1.0)
    } else {
        1.0
    };
    if let Some(w) = get(doc, d, b"Widths").and_then(|o| o.as_array().ok()) {
        for (i, v) in w.iter().enumerate() {
            if let Some(x) = num(deref(doc, v)) {
                widths.insert(first + i as u32, x * scale);
            }
        }
    }
    let desc = get(doc, d, b"FontDescriptor").and_then(|o| o.as_dict().ok());
    let missing = desc.and_then(|fd| get(doc, fd, b"MissingWidth")).and_then(num).unwrap_or(0.0);
    let base = name(doc, d, b"BaseFont").map(|b| String::from_utf8_lossy(b).to_string()).unwrap_or_default();
    // Codes hors de /Widths : /MissingWidth (0 par défaut, comme le prévoit la norme).
    let mut default_w = missing;
    if widths.is_empty() {
        // Police standard sans /Widths : tables de métriques intégrées.
        let lower = base.to_ascii_lowercase();
        let family = if lower.contains("courier") {
            "mono"
        } else if lower.contains("times") {
            "serif"
        } else {
            "sans"
        };
        let bold = lower.contains("bold");
        for code in 32u32..=255 {
            let c = winansi(code as u8);
            widths.insert(code, char_width_in(c, family, bold));
        }
        default_w = if family == "mono" { 600.0 } else { 500.0 };
    }
    let (ascent, descent) = if subtype == b"Type3" {
        let bb = get(doc, d, b"FontBBox").and_then(|o| o.as_array().ok());
        let m3 = get(doc, d, b"FontMatrix")
            .and_then(|o| o.as_array().ok())
            .and_then(|a| a.get(3).and_then(|x| num(deref(doc, x))))
            .unwrap_or(0.001)
            * 1000.0;
        match bb {
            Some(b) if b.len() == 4 => {
                let y0 = num(deref(doc, &b[1])).unwrap_or(-200.0) * m3;
                let y1 = num(deref(doc, &b[3])).unwrap_or(800.0) * m3;
                (y1.max(y0).max(500.0), y0.min(y1).min(-50.0))
            }
            _ => (800.0, -200.0),
        }
    } else {
        descriptor_vertical(doc, desc, 1.0)
    };
    FontMetrics { codes: Codes::One, widths, default_w, cid_map: None, ascent, descent, uncertain: false, composite: false }
}

fn composite(doc: &Document, d: &Dictionary) -> FontMetrics {
    let mut m = FontMetrics { codes: Codes::Two, composite: true, ..FontMetrics::fallback() };
    m.uncertain = false;
    m.default_w = 1000.0;
    let enc = get(doc, d, b"Encoding");
    match enc {
        Some(Object::Name(n)) => {
            let n = n.as_slice();
            if n.ends_with(b"-V") {
                m.uncertain = true;
            }
            if n.starts_with(b"Identity") {
                m.codes = Codes::Two;
            } else if n.windows(4).any(|w| w == b"UCS2") || n.windows(5).any(|w| w == b"UTF16") {
                // Codes Unicode : CID inconnus sans la table du CMap prédéfini.
                m.codes = Codes::Two;
                m.uncertain = true;
            } else {
                m.uncertain = true;
                m.codes = Codes::Ranges(vec![(1, vec![0x00], vec![0x80]), (2, vec![0x81, 0x00], vec![0xFE, 0xFF])]);
            }
        }
        Some(Object::Stream(s)) => {
            let data = s.decompressed_content().unwrap_or_else(|_| s.content.clone());
            let (ranges, map, vertical) = parse_cmap(&data);
            if !ranges.is_empty() {
                m.codes = Codes::Ranges(ranges);
            }
            if !map.is_empty() {
                m.cid_map = Some(map);
            }
            m.uncertain = vertical;
        }
        _ => m.uncertain = true,
    }
    let desc_font = get(doc, d, b"DescendantFonts")
        .and_then(|o| o.as_array().ok())
        .and_then(|a| a.first())
        .and_then(|o| deref(doc, o).as_dict().ok());
    if let Some(cf) = desc_font {
        if let Some(dw) = get(doc, cf, b"DW").and_then(num) {
            m.default_w = dw;
        }
        if let Some(w) = get(doc, cf, b"W").and_then(|o| o.as_array().ok()) {
            let mut i = 0;
            while i < w.len() {
                let Some(c0) = num(deref(doc, &w[i])) else { break };
                match w.get(i + 1).map(|o| deref(doc, o)) {
                    Some(Object::Array(list)) => {
                        for (k, v) in list.iter().enumerate() {
                            if let Some(x) = num(deref(doc, v)) {
                                m.widths.insert(c0 as u32 + k as u32, x);
                            }
                        }
                        i += 2;
                    }
                    Some(o) => {
                        let (Some(c1), Some(x)) = (num(o), w.get(i + 2).and_then(|v| num(deref(doc, v)))) else { break };
                        if c1 - c0 <= 70_000.0 {
                            for c in c0 as u32..=c1 as u32 {
                                m.widths.insert(c, x);
                            }
                        }
                        i += 3;
                    }
                    None => break,
                }
            }
        }
        let (a, de) = descriptor_vertical(doc, get(doc, cf, b"FontDescriptor").and_then(|o| o.as_dict().ok()), 1.0);
        m.ascent = a;
        m.descent = de;
    }
    m
}

/// Plages d'espace de codes et correspondances code → CID d'un CMap intégré.
#[allow(clippy::type_complexity)]
fn parse_cmap(data: &[u8]) -> (Vec<(usize, Vec<u8>, Vec<u8>)>, HashMap<u32, u32>, bool) {
    let text = String::from_utf8_lossy(data);
    let mut ranges = Vec::new();
    let mut map = HashMap::new();
    let vertical = text.contains("/WMode 1");
    let hex = |s: &str| -> Option<Vec<u8>> {
        let s = s.trim().trim_start_matches('<').trim_end_matches('>');
        if s.len() % 2 != 0 {
            return None;
        }
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
    };
    let as_u32 = |b: &[u8]| b.iter().fold(0u32, |a, &x| (a << 8) | x as u32);
    let section = |start: &str, end: &str| -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = text.as_ref();
        while let Some(p) = rest.find(start) {
            let after = &rest[p + start.len()..];
            let Some(e) = after.find(end) else { break };
            out.push(after[..e].to_string());
            rest = &after[e + end.len()..];
        }
        out
    };
    let tokens = |s: &str| -> Vec<String> {
        s.replace('<', " <").replace('>', "> ").split_whitespace().map(str::to_string).collect()
    };
    for body in section("begincodespacerange", "endcodespacerange") {
        let t = tokens(&body);
        for pair in t.chunks(2) {
            if let [a, b] = pair {
                if let (Some(lo), Some(hi)) = (hex(a), hex(b)) {
                    if lo.len() == hi.len() && !lo.is_empty() {
                        ranges.push((lo.len(), lo, hi));
                    }
                }
            }
        }
    }
    for body in section("begincidrange", "endcidrange") {
        let t = tokens(&body);
        for tri in t.chunks(3) {
            if let [a, b, c] = tri {
                if let (Some(lo), Some(hi), Ok(cid)) = (hex(a), hex(b), c.parse::<u32>()) {
                    let (lo, hi) = (as_u32(&lo), as_u32(&hi));
                    if hi >= lo && hi - lo <= 70_000 {
                        for k in 0..=(hi - lo) {
                            map.insert(lo + k, cid + k);
                        }
                    }
                }
            }
        }
    }
    for body in section("begincidchar", "endcidchar") {
        let t = tokens(&body);
        for pair in t.chunks(2) {
            if let [a, c] = pair {
                if let (Some(code), Ok(cid)) = (hex(a), c.parse::<u32>()) {
                    map.insert(as_u32(&code), cid);
                }
            }
        }
    }
    // Plages les plus longues d'abord n'ont pas d'importance : elles sont disjointes.
    (ranges, map, vertical)
}

/// Caractère WinAnsi d'un octet (pour les métriques des polices standard).
fn winansi(b: u8) -> char {
    match b {
        0x80 => '€',
        0x85 => '…',
        0x91 => '‘',
        0x92 => '’',
        0x93 => '“',
        0x94 => '”',
        0x96 => '–',
        0x97 => '—',
        0x9C => 'œ',
        0x8C => 'Œ',
        b => b as char,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmap_integre() {
        let cmap = b"1 begincodespacerange <00> <7F> <8140> <FEFE> endcodespacerange\n2 begincidrange <20> <7E> 1 <8140> <8142> 633 endcidrange";
        let (ranges, map, v) = parse_cmap(cmap);
        assert_eq!(ranges.len(), 2);
        assert_eq!(map.get(&0x41), Some(&34));
        assert_eq!(map.get(&0x8141), Some(&634));
        assert!(!v);
        let m = FontMetrics { codes: Codes::Ranges(ranges), ..FontMetrics::fallback() };
        assert_eq!(m.split(&[0x41, 0x81, 0x40, 0x42]), vec![(0x41, 1), (0x8140, 2), (0x42, 1)]);
    }
}
