//! Génération du dictionnaire et de l'apparence (/AP) d'une annotation.
//!
//! Le dessin est fait dans le repère d'affichage (origine en haut à gauche,
//! y vers le bas) ; une matrice `cm` le ramène dans l'espace utilisateur de la
//! page, quelle que soit sa rotation. Le texte reste ainsi droit à l'écran.

use lopdf::{dictionary, Dictionary, Object};

use super::geom::{PageGeom, Rect};
use super::model::{rgb, Annot, Kind, STAMPS};
use super::text::{pdf_string, text_width, text_width_in, wrap_in};

pub struct Built {
    pub subtype: &'static str,
    /// Rectangle de l'annotation dans l'espace utilisateur.
    pub rect: Rect,
    /// Entrées propres au type (QuadPoints, InkList, L, DA, Name…).
    pub dict: Dictionary,
    /// Flux d'apparence normale.
    pub content: Vec<u8>,
    pub resources: Dictionary,
}

/// Marge intérieure et interligne des zones de texte (identiques côté interface).
pub const TEXT_PAD: f32 = 4.0;
pub const LINE_HEIGHT: f32 = 1.25;

/// Format numérique compact pour les flux de contenu.
pub(crate) fn n(v: f32) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

/// Matrice affichage → utilisateur.
pub(crate) fn display_matrix(g: &PageGeom) -> [f32; 6] {
    match g.rot {
        1 => [0.0, 1.0, 1.0, 0.0, g.x0, g.y0],
        2 => [-1.0, 0.0, 0.0, 1.0, g.x1, g.y0],
        3 => [0.0, -1.0, -1.0, 0.0, g.x1, g.y1],
        _ => [1.0, 0.0, 0.0, -1.0, g.x0, g.y1],
    }
}

fn reals(v: &[f32]) -> Object {
    Object::Array(v.iter().map(|&x| Object::Real(x)).collect())
}

struct Path(Vec<u8>);

impl Path {
    fn new() -> Self {
        Path(Vec::new())
    }
    fn op(&mut self, s: impl AsRef<str>) -> &mut Self {
        self.0.extend_from_slice(s.as_ref().as_bytes());
        self.0.push(b'\n');
        self
    }
    fn raw(&mut self, b: &[u8]) -> &mut Self {
        self.0.extend_from_slice(b);
        self
    }
    fn rounded_rect(&mut self, r: &Rect, rad: f32) -> &mut Self {
        let k = 0.5523 * rad;
        let (x0, y0, x1, y1) = (r.x, r.y, r.x + r.w, r.y + r.h);
        self.op(format!("{} {} m", n(x0 + rad), n(y0)));
        self.op(format!("{} {} l", n(x1 - rad), n(y0)));
        self.op(format!("{} {} {} {} {} {} c", n(x1 - rad + k), n(y0), n(x1), n(y0 + rad - k), n(x1), n(y0 + rad)));
        self.op(format!("{} {} l", n(x1), n(y1 - rad)));
        self.op(format!("{} {} {} {} {} {} c", n(x1), n(y1 - rad + k), n(x1 - rad + k), n(y1), n(x1 - rad), n(y1)));
        self.op(format!("{} {} l", n(x0 + rad), n(y1)));
        self.op(format!("{} {} {} {} {} {} c", n(x0 + rad - k), n(y1), n(x0), n(y1 - rad + k), n(x0), n(y1 - rad)));
        self.op(format!("{} {} l", n(x0), n(y0 + rad)));
        self.op(format!("{} {} {} {} {} {} c", n(x0), n(y0 + rad - k), n(x0 + rad - k), n(y0), n(x0 + rad), n(y0)));
        self.op("h")
    }
    fn ellipse(&mut self, r: &Rect) -> &mut Self {
        let (cx, cy, rx, ry) = (r.x + r.w / 2.0, r.y + r.h / 2.0, r.w / 2.0, r.h / 2.0);
        let (kx, ky) = (0.5523 * rx, 0.5523 * ry);
        self.op(format!("{} {} m", n(cx + rx), n(cy)));
        self.op(format!("{} {} {} {} {} {} c", n(cx + rx), n(cy + ky), n(cx + kx), n(cy + ry), n(cx), n(cy + ry)));
        self.op(format!("{} {} {} {} {} {} c", n(cx - kx), n(cy + ry), n(cx - rx), n(cy + ky), n(cx - rx), n(cy)));
        self.op(format!("{} {} {} {} {} {} c", n(cx - rx), n(cy - ky), n(cx - kx), n(cy - ry), n(cx), n(cy - ry)));
        self.op(format!("{} {} {} {} {} {} c", n(cx + kx), n(cy - ry), n(cx + rx), n(cy - ky), n(cx + rx), n(cy)));
        self.op("h")
    }
}

fn font(base: &str) -> Object {
    Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => Object::Name(base.as_bytes().to_vec()),
        "Encoding" => "WinAnsiEncoding",
    })
}

/// Pointe de flèche à l'extrémité (x2, y2).
pub fn arrow_head(l: [f32; 4], width: f32) -> [[f32; 2]; 3] {
    let (dx, dy) = (l[2] - l[0], l[3] - l[1]);
    let a = dy.atan2(dx);
    let len = 9.0 + 3.0 * width;
    let s = 0.5_f32;
    [
        [l[2] - len * (a - s).cos(), l[3] - len * (a - s).sin()],
        [l[2], l[3]],
        [l[2] - len * (a + s).cos(), l[3] - len * (a + s).sin()],
    ]
}

/// Boîte d'un tampon pour un libellé (taille d'affichage).
pub fn stamp_size(label: &str) -> (f32, f32) {
    let fs = 14.0;
    (text_width(label, fs, true, fs * 0.12) + 30.0, fs + 18.0)
}

/// Boîte englobante (affichage) de l'annotation, épaisseur de trait comprise.
pub fn display_bounds(a: &Annot) -> Rect {
    let pad = a.width.max(1.0) + 1.0;
    let r = match a.kind {
        k if k.is_markup() => a.rects.iter().skip(1).fold(a.rects.first().copied().unwrap_or(Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }), |acc, r| acc.union(r)),
        Kind::Ink => {
            let pts: Vec<[f32; 2]> = a.ink.iter().flatten().copied().collect();
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for p in &pts {
                x0 = x0.min(p[0]);
                y0 = y0.min(p[1]);
                x1 = x1.max(p[0]);
                y1 = y1.max(p[1]);
            }
            if pts.is_empty() { Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 } } else { Rect::from_corners(x0, y0, x1, y1) }
        }
        Kind::Line | Kind::Arrow => {
            let l = a.line.unwrap_or([0.0; 4]);
            let mut r = Rect::from_corners(l[0], l[1], l[2], l[3]);
            if a.kind == Kind::Arrow {
                for p in arrow_head(l, a.width) {
                    r = r.union(&Rect { x: p[0], y: p[1], w: 0.0, h: 0.0 });
                }
            }
            r
        }
        Kind::Stamp => {
            // Inclinaison de −6° : marge pour les coins tournés (écrite dans /RD).
            let r = a.rect.unwrap_or(Rect { x: 0.0, y: 0.0, w: 100.0, h: 30.0 });
            return r.inflate(stamp_margin(&r));
        }
        // Boîte exacte (le trait est dessiné à l'intérieur) : relue à l'identique.
        _ => return a.rect.unwrap_or(Rect { x: 0.0, y: 0.0, w: 20.0, h: 20.0 }),
    };
    r.inflate(pad)
}

/// Marge autour d'un tampon incliné.
pub fn stamp_margin(r: &Rect) -> f32 {
    r.w * 0.06 + 2.0
}

pub fn build(a: &Annot, g: &PageGeom) -> Built {
    let (r, gc, b) = rgb(&a.color);
    let col = format!("{} {} {}", n(r), n(gc), n(b));
    let w = a.width.max(0.5);
    let bounds = display_bounds(a);
    let m = display_matrix(g);
    let mut p = Path::new();
    p.op(format!("q {} {} {} {} {} {} cm", n(m[0]), n(m[1]), n(m[2]), n(m[3]), n(m[4]), n(m[5])));
    p.op("/GS0 gs");
    let mut dict = Dictionary::new();
    let mut ext = dictionary! { "Type" => "ExtGState", "CA" => Object::Real(a.opacity), "ca" => Object::Real(a.opacity) };
    let mut fonts = Dictionary::new();

    let quads = |rects: &[Rect]| -> Object {
        let mut v = Vec::new();
        for rc in rects {
            // Ordre usuel : haut-gauche, haut-droit, bas-gauche, bas-droit (à l'écran).
            for (dx, dy) in [(rc.x, rc.y), (rc.x + rc.w, rc.y), (rc.x, rc.y + rc.h), (rc.x + rc.w, rc.y + rc.h)] {
                let (ux, uy) = g.to_user(dx, dy);
                v.push(ux);
                v.push(uy);
            }
        }
        reals(&v)
    };

    let subtype = match a.kind {
        Kind::Highlight => {
            ext.set("BM", "Multiply");
            p.op(format!("{col} rg"));
            for rc in &a.rects {
                p.op(format!("{} {} {} {} re f", n(rc.x), n(rc.y), n(rc.w), n(rc.h)));
            }
            dict.set("QuadPoints", quads(&a.rects));
            "Highlight"
        }
        Kind::Underline | Kind::Strike | Kind::Squiggly => {
            p.op(format!("{col} RG 1 J"));
            for rc in &a.rects {
                let lw = (rc.h * 0.07).max(1.0);
                p.op(format!("{} w", n(lw)));
                match a.kind {
                    Kind::Squiggly => {
                        let y = rc.y + rc.h - lw;
                        let step = (rc.h * 0.18).max(2.0);
                        let mut x = rc.x;
                        p.op(format!("{} {} m", n(x), n(y)));
                        let mut up = true;
                        while x < rc.x + rc.w {
                            x = (x + step).min(rc.x + rc.w);
                            p.op(format!("{} {} l", n(x), n(if up { y - step * 0.6 } else { y })));
                            up = !up;
                        }
                        p.op("S");
                    }
                    _ => {
                        let y = if a.kind == Kind::Strike { rc.y + rc.h * 0.55 } else { rc.y + rc.h * 0.93 };
                        p.op(format!("{} {} m {} {} l S", n(rc.x), n(y), n(rc.x + rc.w), n(y)));
                    }
                }
            }
            dict.set("QuadPoints", quads(&a.rects));
            match a.kind {
                Kind::Strike => "StrikeOut",
                Kind::Squiggly => "Squiggly",
                _ => "Underline",
            }
        }
        Kind::Rect | Kind::Ellipse => {
            let rc = a.rect.unwrap_or(bounds).inflate(-w / 2.0);
            p.op(format!("{col} RG {} w", n(w)));
            if a.kind == Kind::Rect {
                p.rounded_rect(&rc, 3.0_f32.min(rc.w / 2.0).min(rc.h / 2.0));
            } else {
                p.ellipse(&rc);
            }
            p.op("S");
            dict.set("BS", dictionary! { "W" => Object::Real(w), "S" => "S" });
            if a.kind == Kind::Rect { "Square" } else { "Circle" }
        }
        Kind::Line | Kind::Arrow => {
            let l = a.line.unwrap_or([0.0; 4]);
            p.op(format!("{col} RG {} w 1 J 1 j", n(w)));
            p.op(format!("{} {} m {} {} l S", n(l[0]), n(l[1]), n(l[2]), n(l[3])));
            if a.kind == Kind::Arrow {
                let h = arrow_head(l, w);
                p.op(format!("{} {} m {} {} l {} {} l S", n(h[0][0]), n(h[0][1]), n(h[1][0]), n(h[1][1]), n(h[2][0]), n(h[2][1])));
            }
            let (ux1, uy1) = g.to_user(l[0], l[1]);
            let (ux2, uy2) = g.to_user(l[2], l[3]);
            dict.set("L", reals(&[ux1, uy1, ux2, uy2]));
            dict.set("BS", dictionary! { "W" => Object::Real(w) });
            dict.set("LE", Object::Array(vec!["None".into(), if a.kind == Kind::Arrow { "OpenArrow".into() } else { "None".into() }]));
            "Line"
        }
        Kind::Ink => {
            p.op(format!("{col} RG {} w 1 J 1 j", n(w)));
            let mut list = Vec::new();
            for stroke in &a.ink {
                if stroke.is_empty() {
                    continue;
                }
                p.op(format!("{} {} m", n(stroke[0][0]), n(stroke[0][1])));
                if stroke.len() == 1 {
                    p.op(format!("{} {} l", n(stroke[0][0] + 0.1), n(stroke[0][1])));
                }
                // Lissage : courbes passant par le milieu des segments.
                for i in 1..stroke.len() {
                    let (a0, a1) = (stroke[i - 1], stroke[i]);
                    let mid = [(a0[0] + a1[0]) / 2.0, (a0[1] + a1[1]) / 2.0];
                    if i == stroke.len() - 1 {
                        p.op(format!("{} {} l", n(a1[0]), n(a1[1])));
                    } else {
                        p.op(format!("{} {} {} {} {} {} c", n(a0[0]), n(a0[1]), n(a0[0]), n(a0[1]), n(mid[0]), n(mid[1])));
                    }
                }
                p.op("S");
                let mut pts = Vec::new();
                for pt in stroke {
                    let (ux, uy) = g.to_user(pt[0], pt[1]);
                    pts.push(ux);
                    pts.push(uy);
                }
                list.push(reals(&pts));
            }
            dict.set("InkList", Object::Array(list));
            dict.set("BS", dictionary! { "W" => Object::Real(w) });
            "Ink"
        }
        Kind::Note => {
            // Icône de bulle (22 × 22) dans le coin haut-gauche de la boîte.
            let rc = a.rect.unwrap_or(Rect { x: 0.0, y: 0.0, w: 22.0, h: 22.0 });
            let bubble = Rect { x: rc.x + 1.0, y: rc.y + 1.0, w: 20.0, h: 16.0 };
            p.op(format!("{col} rg 0.35 0.3 0.2 RG 0.8 w"));
            p.rounded_rect(&bubble, 4.0);
            p.op(format!("{} {} m {} {} l {} {} l h", n(rc.x + 6.0), n(rc.y + 16.5), n(rc.x + 5.0), n(rc.y + 21.0), n(rc.x + 11.0), n(rc.y + 16.5)));
            p.op("B");
            p.op("0.2 0.18 0.12 RG 1 w 1 J");
            for k in 0..3 {
                let y = rc.y + 5.5 + k as f32 * 3.5;
                p.op(format!("{} {} m {} {} l S", n(rc.x + 5.5), n(y), n(rc.x + if k == 2 { 12.0 } else { 16.5 }), n(y)));
            }
            dict.set("Name", "Comment");
            dict.set("Open", false);
            "Text"
        }
        Kind::Text => {
            let rc = a.rect.unwrap_or(bounds);
            let fs = a.font_size.clamp(6.0, 144.0);
            // Fond et bordure : uniquement s'ils sont demandés.
            if let Some(fill) = &a.fill {
                let (fr, fg, fb) = rgb(fill);
                p.op(format!("{} {} {} rg", n(fr), n(fg), n(fb)));
                p.rounded_rect(&rc, 3.0);
                p.op("f");
            }
            let border = a.border_color.as_ref().filter(|_| a.width > 0.0);
            if let Some(bc) = border {
                let (br, bg, bb) = rgb(bc);
                p.op(format!("{} {} {} RG {} w", n(br), n(bg), n(bb), n(a.width)));
                p.rounded_rect(&rc.inflate(-a.width / 2.0), 3.0);
                p.op("S");
            }
            let (res, base) = super::model::base_font(&a.font, a.bold, a.italic);
            fonts.set(res, font(base));
            // Texte limité à la boîte.
            p.op(format!("q {} {} {} {} re W n", n(rc.x), n(rc.y), n(rc.w), n(rc.h)));
            p.op(format!("{col} rg BT /{res} {} Tf", n(fs)));
            let inner = rc.w - 2.0 * TEXT_PAD;
            let lines = wrap_in(&a.text, fs, inner, &a.font, a.bold);
            for (i, line) in lines.iter().enumerate() {
                let lw = text_width_in(line, fs, &a.font, a.bold);
                let x = match a.align {
                    1 => rc.x + TEXT_PAD + (inner - lw) / 2.0,
                    2 => rc.x + rc.w - TEXT_PAD - lw,
                    _ => rc.x + TEXT_PAD,
                };
                let y = rc.y + TEXT_PAD + fs * 0.86 + i as f32 * fs * LINE_HEIGHT;
                p.op(format!("1 0 0 -1 {} {} Tm", n(x), n(y)));
                p.raw(&pdf_string(line)).op(" Tj");
            }
            p.op("ET Q");
            dict.set("DA", Object::string_literal(format!("{col} rg /{res} {} Tf", n(fs))));
            dict.set("Q", a.align.min(2) as i64);
            dict.set("BS", dictionary! { "W" => Object::Real(if border.is_some() { a.width } else { 0.0 }) });
            if let Some(fill) = &a.fill {
                let (fr, fg, fb) = rgb(fill);
                dict.set("IC", reals(&[fr, fg, fb]));
            }
            "FreeText"
        }
        Kind::Stamp => {
            let rc = a.rect.unwrap_or(Rect { x: 0.0, y: 0.0, w: 120.0, h: 32.0 });
            let fs = 14.0;
            let (cx, cy) = (rc.x + rc.w / 2.0, rc.y + rc.h / 2.0);
            let ang = (-6.0_f32).to_radians();
            // Rotation autour du centre (repère y vers le bas).
            p.op(format!(
                "1 0 0 1 {} {} cm {} {} {} {} 0 0 cm 1 0 0 1 {} {} cm",
                n(cx),
                n(cy),
                n(ang.cos()),
                n(ang.sin()),
                n(-ang.sin()),
                n(ang.cos()),
                n(-cx),
                n(-cy)
            ));
            p.op(format!("{col} RG 2.5 w"));
            p.rounded_rect(&rc.inflate(-1.25), 6.0);
            p.op("S");
            fonts.set("HeBo", font("Helvetica-Bold"));
            let tw = text_width(&a.text, fs, true, fs * 0.12);
            p.op(format!("{col} rg BT /HeBo {} Tf {} Tc", n(fs), n(fs * 0.12)));
            p.op(format!("1 0 0 -1 {} {} Tm", n(cx - tw / 2.0), n(cy + fs * 0.36)));
            p.raw(&pdf_string(&a.text)).op(" Tj ET");
            let name = STAMPS.iter().find(|s| s.0 == a.text).map(|s| s.1).unwrap_or("Draft");
            dict.set("Name", Object::Name(name.as_bytes().to_vec()));
            let m = stamp_margin(&rc);
            dict.set("RD", reals(&[m, m, m, m]));
            "Stamp"
        }
    };
    p.op("Q");

    let mut resources = dictionary! { "ExtGState" => dictionary! { "GS0" => Object::Dictionary(ext) } };
    if !fonts.is_empty() {
        resources.set("Font", Object::Dictionary(fonts));
    }
    Built { subtype, rect: g.rect_to_user(&bounds), dict, content: p.0, resources }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geom() -> PageGeom {
        PageGeom::new(0.0, 0.0, 600.0, 800.0, 0)
    }

    #[test]
    fn surlignage_avec_quadpoints_en_espace_utilisateur() {
        let a = Annot { kind: Kind::Highlight, rects: vec![Rect { x: 10.0, y: 100.0, w: 50.0, h: 12.0 }], opacity: 0.45, ..Default::default() };
        let b = build(&a, &geom());
        assert_eq!(b.subtype, "Highlight");
        let q = b.dict.get(b"QuadPoints").unwrap().as_array().unwrap();
        // Premier point : haut-gauche à l'écran → y utilisateur = 800 − 100.
        assert_eq!(q[1].as_float().unwrap(), 700.0);
        assert!(String::from_utf8_lossy(&b.content).contains("re f"));
        assert!(b.rect.y < 700.0 && b.rect.y + b.rect.h > 700.0);
    }

    #[test]
    fn fleche_et_ligne() {
        let a = Annot { kind: Kind::Arrow, line: Some([10.0, 10.0, 110.0, 10.0]), width: 2.0, ..Default::default() };
        let b = build(&a, &geom());
        assert_eq!(b.subtype, "Line");
        assert!(b.dict.get(b"LE").is_ok());
        let h = arrow_head([0.0, 0.0, 100.0, 0.0], 2.0);
        // Deux branches symétriques, en retrait de la pointe.
        assert!(h[0][0] < 100.0 && h[2][0] < 100.0);
        assert!((h[0][1] + h[2][1]).abs() < 1e-4 && h[0][1].abs() > 1.0);
        assert_eq!(h[1], [100.0, 0.0]);
    }

    #[test]
    fn tampon_et_texte_avec_police() {
        let (w, h) = stamp_size("APPROUVÉ");
        let a = Annot { kind: Kind::Stamp, text: "APPROUVÉ".into(), rect: Some(Rect { x: 50.0, y: 50.0, w, h }), color: "#1F9D6A".into(), ..Default::default() };
        let b = build(&a, &geom());
        assert_eq!(b.subtype, "Stamp");
        assert!(b.resources.get(b"Font").is_ok());
        assert_eq!(b.dict.get(b"Name").unwrap().as_name().unwrap(), b"Approved");
        let t = Annot { kind: Kind::Text, text: "Bonjour".into(), rect: Some(Rect { x: 0.0, y: 0.0, w: 120.0, h: 30.0 }), ..Default::default() };
        let b = build(&t, &geom());
        assert!(String::from_utf8_lossy(&b.content).contains("(Bonjour) Tj"));
    }

    #[test]
    fn page_tournee() {
        let g = PageGeom::new(0.0, 0.0, 600.0, 800.0, 90);
        let a = Annot { kind: Kind::Rect, rect: Some(Rect { x: 0.0, y: 0.0, w: 100.0, h: 50.0 }), ..Default::default() };
        let b = build(&a, &g);
        // Haut-gauche de l'affichage = coin (0, 0) utilisateur sur une page tournée de 90°.
        assert!(b.rect.x <= 0.0 && b.rect.y <= 0.0);
        assert!(String::from_utf8_lossy(&b.content).starts_with("q 0 1 1 0 0 0 cm"));
    }
}
