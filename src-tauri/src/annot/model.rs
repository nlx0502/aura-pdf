//! Modèle d'annotation échangé avec l'interface (coordonnées d'affichage, en points).

use serde::{Deserialize, Serialize};

use super::geom::Rect;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Highlight,
    Underline,
    Strike,
    Squiggly,
    Note,
    Text,
    Ink,
    Arrow,
    Rect,
    Ellipse,
    Line,
    Stamp,
}

impl Kind {
    pub fn is_markup(self) -> bool {
        matches!(self, Kind::Highlight | Kind::Underline | Kind::Strike | Kind::Squiggly)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Reply {
    pub author: String,
    pub text: String,
    pub date: Option<String>,
    /// Réponse déjà présente dans le fichier (ne pas la réécrire).
    pub saved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Annot {
    /// Identifiant unique (clé /NM du PDF).
    pub id: String,
    pub page: u32,
    pub kind: Kind,
    /// Couleur « #RRGGBB ».
    pub color: String,
    pub opacity: f32,
    pub width: f32,
    /// Zones de texte balisées (surlignage, soulignement, barré).
    pub rects: Vec<Rect>,
    /// Boîte englobante (note, zone de texte, tampon, rectangle, ellipse).
    pub rect: Option<Rect>,
    /// Traits du dessin libre.
    pub ink: Vec<Vec<[f32; 2]>>,
    /// Segment (ligne, flèche) : x1, y1, x2, y2.
    pub line: Option<[f32; 4]>,
    /// Texte affiché (zone de texte), libellé (tampon) ou texte balisé (citation).
    pub text: String,
    pub font_size: f32,
    /// Zone de texte : famille (« sans », « serif », « mono »).
    pub font: String,
    pub bold: bool,
    pub italic: bool,
    /// Alignement : 0 gauche, 1 centré, 2 droite (/Q).
    pub align: u8,
    /// Couleur de fond (aucune si absent).
    pub fill: Option<String>,
    /// Couleur de bordure (aucune bordure si absent ; épaisseur = `width`).
    pub border_color: Option<String>,
    /// Commentaire associé.
    pub contents: String,
    pub author: String,
    pub modified: Option<String>,
    pub replies: Vec<Reply>,
    pub resolved: bool,
    /// État « résolu » lu dans le fichier (pour n'écrire qu'un changement).
    pub saved_resolved: bool,
    /// Position de l'annotation d'origine dans le tableau /Annots de la page.
    pub source: Option<u32>,
    /// Forme, position ou style modifiés : l'apparence doit être régénérée.
    pub geom_dirty: bool,
    /// Commentaire modifié.
    pub meta_dirty: bool,
    /// Annotation d'origine dont l'apparence ne peut pas être reproduite
    /// (ex. tampon image) : déplacement et changement de style désactivés.
    pub locked: bool,
}

impl Default for Annot {
    fn default() -> Self {
        Self {
            id: String::new(),
            page: 0,
            kind: Kind::Note,
            color: "#F2B84B".into(),
            opacity: 1.0,
            width: 1.5,
            rects: Vec::new(),
            rect: None,
            ink: Vec::new(),
            line: None,
            text: String::new(),
            font_size: 12.0,
            font: "sans".into(),
            bold: false,
            italic: false,
            align: 0,
            fill: None,
            border_color: None,
            contents: String::new(),
            author: String::new(),
            modified: None,
            replies: Vec::new(),
            resolved: false,
            saved_resolved: false,
            source: None,
            geom_dirty: false,
            meta_dirty: false,
            locked: false,
        }
    }
}

/// Couleur « #RRGGBB » → composantes 0..1.
pub fn rgb(hex: &str) -> (f32, f32, f32) {
    let h = hex.trim_start_matches('#');
    let p = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0) as f32 / 255.0;
    (p(0), p(2), p(4))
}

/// Composantes 0..1 → « #RRGGBB ».
pub fn hex(r: f32, g: f32, b: f32) -> String {
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", c(r), c(g), c(b))
}

/// Police standard PDF (14 polices de base) : (nom de ressource, BaseFont).
pub fn base_font(family: &str, bold: bool, italic: bool) -> (&'static str, &'static str) {
    match (family, bold, italic) {
        ("serif", false, false) => ("TiRo", "Times-Roman"),
        ("serif", true, false) => ("TiBo", "Times-Bold"),
        ("serif", false, true) => ("TiIt", "Times-Italic"),
        ("serif", true, true) => ("TiBI", "Times-BoldItalic"),
        ("mono", false, false) => ("Cour", "Courier"),
        ("mono", true, false) => ("CoBo", "Courier-Bold"),
        ("mono", false, true) => ("CoOb", "Courier-Oblique"),
        ("mono", true, true) => ("CoBO", "Courier-BoldOblique"),
        (_, false, false) => ("Helv", "Helvetica"),
        (_, true, false) => ("HeBo", "Helvetica-Bold"),
        (_, false, true) => ("HeOb", "Helvetica-Oblique"),
        (_, true, true) => ("HeBO", "Helvetica-BoldOblique"),
    }
}

/// Inverse de [`base_font`] à partir du nom de ressource (ou d'un nom approchant).
pub fn font_from_resource(name: &str) -> (String, bool, bool) {
    let n = name.to_lowercase();
    let family = if n.starts_with("ti") || n.contains("times") || n.contains("serif") {
        "serif"
    } else if n.starts_with("co") || n.contains("courier") || n.contains("mono") {
        "mono"
    } else {
        "sans"
    };
    let bold = name.ends_with("Bo") || name.ends_with("BO") || name.ends_with("BI") || n.contains("bold");
    let italic = name.ends_with("It") || name.ends_with("Ob") || name.ends_with("BO") || name.ends_with("BI") || n.contains("italic") || n.contains("oblique");
    (family.into(), bold, italic)
}

/// Libellés de tampon : (texte affiché, nom PDF standard).
pub const STAMPS: [(&str, &str); 4] = [("APPROUVÉ", "Approved"), ("CONFIDENTIEL", "Confidential"), ("BROUILLON", "Draft"), ("À REVOIR", "NotApproved")];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn couleurs() {
        let (r, g, b) = rgb("#F2B84B");
        assert_eq!(hex(r, g, b), "#F2B84B");
    }
}
