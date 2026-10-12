//! Apparences des champs (/AP) : fond, contour (plein, pointillé, souligné),
//! texte (police, taille, couleur, alignement, multiligne, texte indicatif),
//! cases et boutons radio (symbole choisi), libellés visibles.

use lopdf::{dictionary, Dictionary, Object, Stream};

use super::style::{fmt, rgb_op, FieldStyle};
use super::Caption;
use crate::annot::text::{pdf_string, text_width_in, wrap_in};

/// Couleur du texte indicatif (placeholder).
const PLACEHOLDER: &str = "#9c9a92";
const PAD: f32 = 2.0;

pub fn form(content: Vec<u8>, w: f32, h: f32, fonts: Dictionary) -> Stream {
    let mut d = dictionary! {
        "Type" => "XObject", "Subtype" => "Form",
        "BBox" => vec![Object::Real(0.0), Object::Real(0.0), Object::Real(w.max(0.1)), Object::Real(h.max(0.1))],
    };
    if !fonts.is_empty() {
        d.set("Resources", dictionary! { "Font" => fonts });
    }
    Stream::new(d, content)
}

/// Fond et contour d'un champ rectangulaire (ou rond pour un bouton radio).
pub fn frame(s: &FieldStyle, w: f32, h: f32, round: bool) -> String {
    let mut out = String::new();
    if let Some(bg) = s.background.as_deref().and_then(|c| rgb_op(c, false)) {
        if round {
            out += &format!("{bg} {} f\n", circle(w / 2.0, h / 2.0, w.min(h) / 2.0));
        } else {
            out += &format!("{bg} 0 0 {} {} re f\n", fmt(w), fmt(h));
        }
    }
    let bw = s.border_width;
    if bw > 0.0 {
        if let Some(bc) = s.border.as_deref().and_then(|c| rgb_op(c, true)) {
            let dash = if s.border_style == "dashed" { "[3 2] 0 d " } else { "" };
            let hw = bw / 2.0;
            if round {
                out += &format!("{bc} {} w {dash}{} S\n", fmt(bw), circle(w / 2.0, h / 2.0, w.min(h) / 2.0 - hw));
            } else if s.border_style == "underline" {
                out += &format!("{bc} {} w 0 {} m {} {} l S\n", fmt(bw), fmt(hw), fmt(w), fmt(hw));
            } else {
                out += &format!("{bc} {} w {dash}{} {} {} {} re S\n", fmt(bw), fmt(hw), fmt(hw), fmt(w - bw), fmt(h - bw));
            }
        }
    }
    out
}

fn circle(cx: f32, cy: f32, r: f32) -> String {
    let k = r * 0.5523;
    format!(
        "{} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c",
        fmt(cx + r), fmt(cy),
        fmt(cx + r), fmt(cy + k), fmt(cx + k), fmt(cy + r), fmt(cx), fmt(cy + r),
        fmt(cx - k), fmt(cy + r), fmt(cx - r), fmt(cy + k), fmt(cx - r), fmt(cy),
        fmt(cx - r), fmt(cy - k), fmt(cx - k), fmt(cy - r), fmt(cx), fmt(cy - r),
        fmt(cx + k), fmt(cy - r), fmt(cx + r), fmt(cy - k), fmt(cx + r), fmt(cy)
    )
}

/// Taille de police effective (automatique : selon la hauteur et la largeur).
pub fn auto_size(s: &FieldStyle, text: &str, w: f32, h: f32, multiline: bool) -> f32 {
    if s.font_size > 0.0 {
        return s.font_size;
    }
    if multiline {
        return 10.0;
    }
    let mut size = (h * 0.62).clamp(4.0, 12.0);
    let tw = text_width_in(text, size, &s.font, s.bold);
    if tw > w - 2.0 * PAD - s.border_width * 2.0 && tw > 0.0 {
        size = (size * (w - 2.0 * PAD - s.border_width * 2.0) / tw).max(4.0);
    }
    size
}

/// Contenu d'un champ texte ou liste : `shown` (ou le texte indicatif en gris).
pub fn text(s: &FieldStyle, w: f32, h: f32, shown: &str, placeholder: &str, multiline: bool) -> Vec<u8> {
    let mut out = frame(s, w, h, false).into_bytes();
    let (txt, color) = if shown.is_empty() { (placeholder, PLACEHOLDER) } else { (shown, s.text_color.as_str()) };
    out.extend(b"/Tx BMC\n");
    if txt.is_empty() {
        out.extend(b"EMC\n");
        return out;
    }
    let size = auto_size(s, txt, w, h, multiline);
    let inset = PAD + s.border_width;
    let lines: Vec<String> = if multiline {
        wrap_in(txt, size, (w - 2.0 * inset).max(1.0), &s.font, s.bold)
    } else {
        vec![txt.replace('\n', " ")]
    };
    out.extend(
        format!(
            "q {} {} {} {} re W n\nBT /{} {} Tf {}\n",
            fmt(s.border_width),
            fmt(s.border_width),
            fmt((w - 2.0 * s.border_width).max(0.1)),
            fmt((h - 2.0 * s.border_width).max(0.1)),
            s.font_res(),
            fmt(size),
            rgb_op(color, false).unwrap_or_else(|| "0 g".into())
        )
        .bytes(),
    );
    let lead = size * 1.15;
    let mut y = if multiline { h - inset - size * 0.9 } else { (h - size * 0.72) / 2.0 };
    for l in &lines {
        let tw = text_width_in(l, size, &s.font, s.bold);
        let x = match s.align {
            1 => (w - tw) / 2.0,
            2 => w - inset - tw,
            _ => inset,
        };
        out.extend(format!("1 0 0 1 {} {} Tm ", fmt(x), fmt(y)).bytes());
        out.extend(pdf_string(l));
        out.extend(b" Tj\n");
        y -= lead;
    }
    out.extend(b"ET Q\nEMC\n");
    out
}

/// Case ou bouton radio : (apparence activée, désactivée).
pub fn check(s: &FieldStyle, w: f32, h: f32, radio: bool) -> (Vec<u8>, Vec<u8>) {
    let base = frame(s, w, h, radio);
    let color = rgb_op(&s.text_color, false).unwrap_or_else(|| "0 g".into());
    let on = if radio && s.check_style == "circle" {
        format!("{base}{color} {} f\n", circle(w / 2.0, h / 2.0, w.min(h) * 0.24))
    } else {
        let size = if s.font_size > 0.0 { s.font_size } else { h.min(w) * 0.78 };
        // Largeurs ZapfDingbats (1/1000 em) des symboles proposés.
        let gw = match s.check_style.as_str() {
            "cross" => 0.692,
            "circle" => 0.791,
            "square" => 0.761,
            "star" => 0.816,
            "diamond" => 0.776,
            _ => 0.846,
        } * size;
        format!("{base}q {color} BT /ZaDb {} Tf {} {} Td ({}) Tj ET Q\n", fmt(size), fmt((w - gw) / 2.0), fmt((h - size * 0.7) / 2.0), s.check_char())
    };
    (on.into_bytes(), base.into_bytes())
}

/// Libellé visible (texte seul, sur une ligne).
pub fn caption(c: &Caption, h: f32) -> Vec<u8> {
    let res = if c.bold { "HeBo" } else { "Helv" };
    let mut out = format!("BT /{res} {} Tf {} 1 0 0 1 0 {} Tm ", fmt(c.size), rgb_op(&c.color, false).unwrap_or_else(|| "0 g".into()), fmt(h * 0.24)).into_bytes();
    out.extend(pdf_string(&c.text));
    out.extend(b" Tj ET\n");
    out
}
