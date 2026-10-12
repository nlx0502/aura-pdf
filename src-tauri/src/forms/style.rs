//! Apparence d'un champ : fond, contour, police, couleur, taille, alignement,
//! symbole des cases. Lue depuis /MK, /BS, /DA et /Q ; réécrite dans ces mêmes
//! entrées standard (Acrobat régénère ainsi la même apparence à la saisie).

use lopdf::{dictionary, Dictionary, Document, Object};
use serde::{Deserialize, Serialize};

use crate::annot::pdf::{deref, get, num, text};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FieldStyle {
    /// « #rrggbb », ou absent (transparent).
    pub background: Option<String>,
    pub border: Option<String>,
    pub border_width: f32,
    /// solid, dashed, underline.
    pub border_style: String,
    pub text_color: String,
    /// sans, serif, mono.
    pub font: String,
    pub bold: bool,
    /// 0 : automatique.
    pub font_size: f32,
    /// 0 gauche, 1 centré, 2 droite.
    pub align: u8,
    /// check, cross, circle, square, star, diamond.
    pub check_style: String,
}

impl FieldStyle {
    pub fn default_for(kind: &str) -> FieldStyle {
        FieldStyle {
            background: Some("#ffffff".into()),
            border: Some("#c9c7be".into()),
            border_width: 1.0,
            border_style: "solid".into(),
            text_color: "#1a1b1e".into(),
            font: "sans".into(),
            bold: false,
            font_size: 0.0,
            align: 0,
            check_style: if kind == "radio" { "circle" } else { "check" }.into(),
        }
    }

    /// Ressource de police standard (/DA).
    pub fn font_res(&self) -> &'static str {
        match (self.font.as_str(), self.bold) {
            ("serif", false) => "TiRo",
            ("serif", true) => "TiBo",
            ("mono", false) => "Cour",
            ("mono", true) => "CoBo",
            (_, true) => "HeBo",
            _ => "Helv",
        }
    }

    pub fn da(&self) -> String {
        let [r, g, b] = hex_rgb(&self.text_color).unwrap_or([0.0; 3]);
        format!("/{} {} Tf {r:.3} {g:.3} {b:.3} rg", self.font_res(), fmt(self.font_size))
    }

    /// Caractère ZapfDingbats du symbole des cases (/MK /CA).
    pub fn check_char(&self) -> &'static str {
        match self.check_style.as_str() {
            "cross" => "8",
            "circle" => "l",
            "square" => "n",
            "star" => "H",
            "diamond" => "u",
            _ => "4",
        }
    }

    pub fn mk(&self, kind: &str) -> Dictionary {
        let mut mk = Dictionary::new();
        if let Some(c) = self.background.as_deref().and_then(hex_rgb) {
            mk.set("BG", rgb_arr(c));
        }
        if let Some(c) = self.border.as_deref().and_then(hex_rgb) {
            if self.border_width > 0.0 {
                mk.set("BC", rgb_arr(c));
            }
        }
        if kind == "check" || kind == "radio" {
            mk.set("CA", Object::string_literal(self.check_char()));
        }
        mk
    }

    pub fn bs(&self) -> Dictionary {
        let s = match self.border_style.as_str() {
            "dashed" => "D",
            "underline" => "U",
            _ => "S",
        };
        let mut bs = dictionary! { "Type" => "Border", "W" => Object::Real(self.border_width), "S" => s };
        if s == "D" {
            bs.set("D", vec![Object::Integer(3), Object::Integer(2)]);
        }
        bs
    }

    /// Style d'un champ existant (widget, puis attributs hérités du champ).
    pub fn read(doc: &Document, merged: &Dictionary, w: &Dictionary, kind: &str) -> FieldStyle {
        let mut s = FieldStyle::default_for(kind);
        let mk = get(doc, w, b"MK").and_then(|o| o.as_dict().ok());
        s.background = mk.and_then(|mk| color(doc, mk, b"BG")).map(rgb_hex);
        s.border = mk.and_then(|mk| color(doc, mk, b"BC")).map(rgb_hex);
        let bs = get(doc, w, b"BS").and_then(|o| o.as_dict().ok());
        s.border_width = bs.and_then(|b| get(doc, b, b"W")).and_then(num).unwrap_or(if s.border.is_some() { 1.0 } else { 0.0 });
        if s.border.is_none() {
            s.border_width = 0.0;
        }
        s.border_style = match bs.and_then(|b| get(doc, b, b"S")).and_then(|o| o.as_name().ok()) {
            Some(b"D") => "dashed",
            Some(b"U") => "underline",
            _ => "solid",
        }
        .into();
        let da = w.get(b"DA").ok().or_else(|| merged.get(b"DA").ok()).map(text).unwrap_or_default();
        let (font, size, color) = parse_da(&da);
        let (family, bold) = family_of(&font);
        s.font = family.into();
        s.bold = bold;
        s.font_size = size;
        if let Some(c) = color {
            s.text_color = rgb_hex(c);
        }
        s.align = w.get(b"Q").ok().or_else(|| merged.get(b"Q").ok()).and_then(|o| o.as_i64().ok()).unwrap_or(0).clamp(0, 2) as u8;
        if let Some(ca) = mk.and_then(|mk| get(doc, mk, b"CA")).map(text) {
            s.check_style = match ca.as_str() {
                "8" => "cross",
                "l" => "circle",
                "n" => "square",
                "H" => "star",
                "u" => "diamond",
                "4" => "check",
                _ => s.check_style.as_str(),
            }
            .to_string();
        }
        s
    }
}

/// Police de base d'une ressource standard.
pub fn base_font(res: &str) -> &'static str {
    match res {
        "HeBo" => "Helvetica-Bold",
        "TiRo" => "Times-Roman",
        "TiBo" => "Times-Bold",
        "Cour" => "Courier",
        "CoBo" => "Courier-Bold",
        "ZaDb" => "ZapfDingbats",
        _ => "Helvetica",
    }
}

/// Famille et graisse d'une ressource de police (noms usuels d'Acrobat).
pub fn family_of(res: &str) -> (&'static str, bool) {
    let r = res.to_ascii_lowercase();
    let bold = r.contains("bo") || r.contains("bold");
    if r.starts_with("ti") || r.contains("times") || r.contains("serif") {
        ("serif", bold)
    } else if r.starts_with("co") || r.contains("courier") || r.contains("mono") {
        ("mono", bold)
    } else {
        ("sans", bold)
    }
}

/// Nom de police, taille et couleur d'une chaîne /DA.
pub fn parse_da(da: &str) -> (String, f32, Option<[f32; 3]>) {
    let t: Vec<&str> = da.split_whitespace().collect();
    let mut font = "Helv".to_string();
    let mut size = 0.0;
    let mut color = None;
    for (i, tok) in t.iter().enumerate() {
        match *tok {
            "Tf" if i >= 2 => {
                font = t[i - 2].trim_start_matches('/').to_string();
                size = t[i - 1].parse().unwrap_or(0.0);
            }
            "g" if i >= 1 => {
                let g: f32 = t[i - 1].parse().unwrap_or(0.0);
                color = Some([g, g, g]);
            }
            "rg" if i >= 3 => color = Some([t[i - 3].parse().unwrap_or(0.0), t[i - 2].parse().unwrap_or(0.0), t[i - 1].parse().unwrap_or(0.0)]),
            "k" if i >= 4 => {
                let v: Vec<f32> = (1..=4).map(|j| t[i - 5 + j].parse().unwrap_or(0.0)).collect();
                color = Some([(1.0 - v[0]) * (1.0 - v[3]), (1.0 - v[1]) * (1.0 - v[3]), (1.0 - v[2]) * (1.0 - v[3])]);
            }
            _ => {}
        }
    }
    (font, size, color)
}

fn color(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<[f32; 3]> {
    let a = get(doc, d, key)?.as_array().ok()?;
    let v: Vec<f32> = a.iter().filter_map(|o| num(deref(doc, o))).collect();
    match v.len() {
        1 => Some([v[0]; 3]),
        3 => Some([v[0], v[1], v[2]]),
        4 => Some([(1.0 - v[0]) * (1.0 - v[3]), (1.0 - v[1]) * (1.0 - v[3]), (1.0 - v[2]) * (1.0 - v[3])]),
        _ => None,
    }
}

pub fn hex_rgb(h: &str) -> Option<[f32; 3]> {
    let h = h.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(|v| v as f32 / 255.0);
    Some([p(0)?, p(2)?, p(4)?])
}

pub fn rgb_hex(c: [f32; 3]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

pub fn rgb_arr(c: [f32; 3]) -> Vec<Object> {
    c.iter().map(|v| Object::Real((v * 1000.0).round() / 1000.0)).collect()
}

/// Opérateur de couleur PDF (« r g b rg » ou « RG »).
pub fn rgb_op(hex: &str, stroke: bool) -> Option<String> {
    let [r, g, b] = hex_rgb(hex)?;
    Some(format!("{r:.3} {g:.3} {b:.3} {}", if stroke { "RG" } else { "rg" }))
}

pub fn fmt(v: f32) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn da_et_couleurs() {
        let (f, s, c) = parse_da("/HeBo 11 Tf 0.2 0.4 0.6 rg");
        assert_eq!((f.as_str(), s), ("HeBo", 11.0));
        assert_eq!(rgb_hex(c.unwrap()), "#336699");
        assert_eq!(family_of("TiBo"), ("serif", true));
        assert_eq!(family_of("Helv"), ("sans", false));
        let mut st = FieldStyle::default_for("text");
        st.font = "mono".into();
        st.text_color = "#336699".into();
        st.font_size = 9.5;
        assert_eq!(st.da(), "/Cour 9.5 Tf 0.200 0.400 0.600 rg");
        assert_eq!(hex_rgb("#ffffff"), Some([1.0, 1.0, 1.0]));
    }
}
