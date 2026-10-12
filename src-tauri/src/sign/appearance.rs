//! Apparence visible d'une signature : tracé à main levée, nom écrit dans une
//! police manuscrite, ou image importée, avec une mention facultative.
//!
//! Le dessin est fait dans le repère d'affichage de la page (y vers le bas) et
//! ramené en espace utilisateur : la signature reste droite sur une page tournée.

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use serde::Deserialize;

use crate::annot::ap::{display_matrix, n};
use crate::annot::geom::{PageGeom, Rect};
use crate::annot::model::rgb;
use crate::annot::text::{pdf_string, text_width};

/// Polices manuscrites fournies avec l'application (licences OFL et Apache 2.0).
pub const FONTS: [(&str, &str, &[u8]); 3] = [
    ("caveat", "Caveat", include_bytes!("../../resources/fonts/Caveat.ttf")),
    ("apple", "Homemade Apple", include_bytes!("../../resources/fonts/HomemadeApple.ttf")),
    ("delafield", "Mrs Saint Delafield", include_bytes!("../../resources/fonts/MrsSaintDelafield.ttf")),
];

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Visual {
    /// Traits dessinés (coordonnées libres, y vers le bas).
    Draw { strokes: Vec<Vec<[f32; 2]>> },
    /// Nom écrit dans une police manuscrite (`caveat`, `apple`, `delafield`).
    Type { text: String, font: String },
    /// Image (PNG transparent conseillé).
    Image { path: String },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualSpec {
    pub visual: Visual,
    #[serde(default = "default_ink")]
    pub color: String,
    /// Mention en petits caractères sous la signature.
    #[serde(default)]
    pub caption: Vec<String>,
}

fn default_ink() -> String {
    "#1F2A6B".into()
}

const PAD: f32 = 4.0;
const CAPTION_SIZE: f32 = 6.5;

/// Crée le flux d'apparence (formulaire XObject) pour `disp` (points
/// d'affichage) ; renvoie son identifiant et le rectangle en espace utilisateur.
pub fn build(doc: &mut Document, spec: &VisualSpec, g: &PageGeom, disp: &Rect) -> Result<(ObjectId, Rect), String> {
    let m = display_matrix(g);
    let user = g.rect_to_user(disp);
    let mut c = format!(
        "q {} {} {} {} {} {} cm 1 0 0 1 {} {} cm\n",
        n(m[0]),
        n(m[1]),
        n(m[2]),
        n(m[3]),
        n(m[4]),
        n(m[5]),
        n(disp.x),
        n(disp.y)
    )
    .into_bytes();
    let mut fonts = Dictionary::new();
    let mut xobjects = Dictionary::new();
    let (w, h) = (disp.w, disp.h);
    let caption_h = if spec.caption.is_empty() { 0.0 } else { spec.caption.len() as f32 * (CAPTION_SIZE * 1.2) + 2.0 };
    let (aw, ah) = ((w - 2.0 * PAD).max(1.0), (h - 2.0 * PAD - caption_h).max(1.0));
    let (r, gg, b) = rgb(&spec.color);
    let ink = format!("{} {} {}", n(r), n(gg), n(b));

    match &spec.visual {
        Visual::Draw { strokes } => {
            let pts: Vec<[f32; 2]> = strokes.iter().flatten().copied().collect();
            if pts.is_empty() {
                return Err("La signature dessinée est vide.".into());
            }
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for p in &pts {
                x0 = x0.min(p[0]);
                y0 = y0.min(p[1]);
                x1 = x1.max(p[0]);
                y1 = y1.max(p[1]);
            }
            let (bw, bh) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
            let s = (aw / bw).min(ah / bh);
            let ox = PAD + (aw - bw * s) / 2.0 - x0 * s;
            let oy = PAD + (ah - bh * s) / 2.0 - y0 * s;
            let lw = (2.6 * s).clamp(0.6, 3.0);
            c.extend_from_slice(format!("{ink} RG {} w 1 J 1 j\n", n(lw)).as_bytes());
            let t = |p: &[f32; 2]| (p[0] * s + ox, p[1] * s + oy);
            for st in strokes.iter().filter(|s| !s.is_empty()) {
                let (sx, sy) = t(&st[0]);
                c.extend_from_slice(format!("{} {} m\n", n(sx), n(sy)).as_bytes());
                if st.len() == 1 {
                    c.extend_from_slice(format!("{} {} l\n", n(sx + 0.01), n(sy)).as_bytes());
                }
                // Tracé lissé : courbes passant par les milieux des segments.
                for i in 1..st.len() {
                    let (cx, cy) = t(&st[i]);
                    if i + 1 < st.len() {
                        let (nx, ny) = t(&st[i + 1]);
                        let (mx, my) = ((cx + nx) / 2.0, (cy + ny) / 2.0);
                        let (px, py) = if i == 1 { (sx, sy) } else { let (a, b2) = t(&st[i - 1]); ((a + cx) / 2.0, (b2 + cy) / 2.0) };
                        // Quadratique (px,py) → (mx,my) de contrôle (cx,cy), écrite en cubique.
                        let c1 = (px + 2.0 / 3.0 * (cx - px), py + 2.0 / 3.0 * (cy - py));
                        let c2 = (mx + 2.0 / 3.0 * (cx - mx), my + 2.0 / 3.0 * (cy - my));
                        c.extend_from_slice(format!("{} {} {} {} {} {} c\n", n(c1.0), n(c1.1), n(c2.0), n(c2.1), n(mx), n(my)).as_bytes());
                    } else {
                        c.extend_from_slice(format!("{} {} l\n", n(cx), n(cy)).as_bytes());
                    }
                }
                c.extend_from_slice(b"S\n");
            }
        }
        Visual::Type { text, font } => {
            let text = text.trim();
            if text.is_empty() {
                return Err("Indiquez le nom à signer.".into());
            }
            let (_, family, data) = FONTS.iter().find(|f| f.0 == font).unwrap_or(&FONTS[0]);
            let f = crate::sysfonts::from_bytes(family, data).ok_or("police indisponible")?;
            let id = crate::sysfonts::embed(doc, &f, text);
            fonts.set("SigF", id);
            let w1 = f.text_width(text, 1.0).max(0.01);
            let size = (ah * 0.72).min(aw / w1);
            let tw = w1 * size;
            let x = PAD + (aw - tw) / 2.0;
            let y = PAD + ah / 2.0 + size * 0.3;
            let Object::String(bytes, _) = crate::sysfonts::encode(&f, text) else { unreachable!() };
            let hex: String = bytes.iter().map(|x| format!("{x:02X}")).collect();
            c.extend_from_slice(format!("BT {ink} rg /SigF {} Tf 1 0 0 -1 {} {} Tm <{hex}> Tj ET\n", n(size), n(x), n(y)).as_bytes());
        }
        Visual::Image { path } => {
            let img = image::open(path).map_err(|e| format!("Image illisible ({e})."))?;
            let (iw, ih) = (img.width(), img.height());
            let rgba = img.to_rgba8();
            let mut rgb_bytes = Vec::with_capacity((iw * ih * 3) as usize);
            let mut alpha = Vec::with_capacity((iw * ih) as usize);
            let mut transparent = false;
            for p in rgba.pixels() {
                rgb_bytes.extend_from_slice(&p.0[..3]);
                alpha.push(p.0[3]);
                transparent |= p.0[3] < 255;
            }
            let mut dict = dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => iw as i64, "Height" => ih as i64, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8 };
            if transparent {
                let mut sm = Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => iw as i64, "Height" => ih as i64, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 }, alpha);
                let _ = sm.compress();
                dict.set("SMask", doc.add_object(sm));
            }
            let mut st = Stream::new(dict, rgb_bytes);
            let _ = st.compress();
            xobjects.set("SigIm", doc.add_object(st));
            let s = (aw / iw as f32).min(ah / ih as f32);
            let (dw, dh) = (iw as f32 * s, ih as f32 * s);
            let x = PAD + (aw - dw) / 2.0;
            let y = PAD + (ah - dh) / 2.0;
            c.extend_from_slice(format!("q {} 0 0 {} {} {} cm /SigIm Do Q\n", n(dw), n(-dh), n(x), n(y + dh)).as_bytes());
        }
    }

    if !spec.caption.is_empty() {
        let helv = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" });
        fonts.set("SigH", helv);
        let mut y = h - PAD - caption_h + CAPTION_SIZE + 1.0;
        for line in &spec.caption {
            let mut t = line.clone();
            while text_width(&t, CAPTION_SIZE, false, 0.0) > w - 2.0 * PAD && t.chars().count() > 4 {
                t.pop();
                t.pop();
                t.push('…');
            }
            c.extend_from_slice(format!("BT 0.35 0.35 0.38 rg /SigH {} Tf 1 0 0 -1 {} {} Tm ", n(CAPTION_SIZE), n(PAD), n(y)).as_bytes());
            c.extend_from_slice(&pdf_string(&t));
            c.extend_from_slice(b" Tj ET\n");
            y += CAPTION_SIZE * 1.2;
        }
    }
    c.extend_from_slice(b"Q\n");

    let mut res = Dictionary::new();
    if !fonts.is_empty() {
        res.set("Font", fonts);
    }
    if !xobjects.is_empty() {
        res.set("XObject", xobjects);
    }
    let bbox = vec![Object::Real(user.x), Object::Real(user.y), Object::Real(user.x + user.w), Object::Real(user.y + user.h)];
    let mut st = Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => bbox, "Resources" => res }, c);
    let _ = st.compress();
    Ok((doc.add_object(st), user))
}
