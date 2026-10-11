//! Imposition : quelles pages imprimer et comment les placer sur les feuilles.
//!
//! Fonctions pures (aucun appel à PDFium), utilisées à la fois par l'aperçu
//! de la fenêtre d'impression et par la tâche d'impression réelle : l'aperçu
//! est donc fidèle par construction.
//!
//! Coordonnées : points PDF, origine en haut à gauche de la feuille.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Paper {
    A4,
    A3,
    Letter,
}

impl Paper {
    /// Dimensions portrait en points.
    pub fn size(self) -> (f32, f32) {
        match self {
            Paper::A4 => (595.28, 841.89),
            Paper::A3 => (841.89, 1190.55),
            Paper::Letter => (612.0, 792.0),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Orient {
    Auto,
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Size,
    Poster,
    Multi,
    Booklet,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    Fit,
    Actual,
    Shrink,
    Custom,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RangeKind {
    All,
    Current,
    Custom,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Subset {
    All,
    Odd,
    Even,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Comments {
    /// Document seul (champs de formulaire compris).
    Doc,
    /// Document et annotations.
    Docann,
    /// Document et tampons uniquement.
    Stamps,
}

/// Réglages de la fenêtre d'impression (miroir de l'état de l'interface).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintSettings {
    pub copies: u32,
    pub collate: bool,
    pub range: RangeKind,
    /// Page courante (index 0), pour « Page actuelle ».
    pub current: u32,
    pub custom: String,
    pub subset: Subset,
    pub reverse: bool,
    pub mode: Mode,
    pub fit: Fit,
    /// Échelle personnalisée en %.
    pub scale: u32,
    pub per_sheet: u32,
    pub borders: bool,
    /// Reliure à droite (brochure).
    pub bind_right: bool,
    /// Agrandissement de l'affiche en %.
    pub tile: u32,
    pub cut_marks: bool,
    pub paper: Paper,
    pub orient: Orient,
    pub duplex: bool,
    /// Retournement sur le bord court (sinon bord long).
    pub short_edge: bool,
    pub gray: bool,
    pub comments: Comments,
    pub summary: bool,
}

impl Default for PrintSettings {
    fn default() -> Self {
        Self {
            copies: 1,
            collate: true,
            range: RangeKind::All,
            current: 0,
            custom: String::new(),
            subset: Subset::All,
            reverse: false,
            mode: Mode::Size,
            fit: Fit::Fit,
            scale: 100,
            per_sheet: 4,
            borders: true,
            bind_right: false,
            tile: 200,
            cut_marks: true,
            paper: Paper::A4,
            orient: Orient::Auto,
            duplex: false,
            short_edge: false,
            gray: false,
            comments: Comments::Docann,
            summary: false,
        }
    }
}

/// Placement d'une page sur une feuille.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct Placement {
    /// Index de page (0) dans le document source.
    pub page: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Cadre ou trait à dessiner (bordure, repère de découpe).
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct Line {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sheet {
    pub w: f32,
    pub h: f32,
    pub items: Vec<Placement>,
    /// Cadres autour des pages (mode « Multiple » avec bordures).
    pub frames: Vec<[f32; 4]>,
    pub marks: Vec<Line>,
    /// Feuille vierge ajoutée (brochure) : aucune page.
    pub blank: bool,
    /// Affiche : (colonne, rangée, colonnes, rangées) de la tuile.
    pub tile: Option<[u32; 4]>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    /// Pages sélectionnées (index 0), dans l'ordre d'impression.
    pub pages: Vec<u32>,
    pub sheets: Vec<Sheet>,
    /// Faces imprimées (une par élément de `sheets`).
    pub sides: u32,
    /// Feuilles physiques pour une copie (recto verso : deux faces par feuille).
    pub physical_per_copy: u32,
    pub error: Option<String>,
}

/// Interprète une plage personnalisée (« 1-5, 8, 11-13 », pages numérotées à partir de 1).
/// Renvoie les index (à partir de 0) ou un message d'erreur en français.
pub fn parse_range(spec: &str, count: u32) -> Result<Vec<u32>, String> {
    let mut out = Vec::new();
    let invalid = || format!("Plage invalide — pages 1 à {count}.");
    for part in spec.split([',', ';']).map(str::trim).filter(|s| !s.is_empty()) {
        let (a, b) = match part.split_once(['-', '–']) {
            Some((a, b)) => {
                let a: u32 = a.trim().parse().map_err(|_| invalid())?;
                // « 5- » : jusqu'à la fin.
                let b: u32 = if b.trim().is_empty() { count } else { b.trim().parse().map_err(|_| invalid())? };
                (a, b)
            }
            None => {
                let n: u32 = part.parse().map_err(|_| invalid())?;
                (n, n)
            }
        };
        if a < 1 || b > count || a > b {
            return Err(invalid());
        }
        out.extend((a - 1)..b);
    }
    if out.is_empty() {
        return Err(invalid());
    }
    Ok(out)
}

/// Liste des pages à imprimer selon la plage, le sous-ensemble et l'ordre.
pub fn select_pages(s: &PrintSettings, count: u32) -> Result<Vec<u32>, String> {
    let mut list: Vec<u32> = match s.range {
        RangeKind::All => (0..count).collect(),
        RangeKind::Current => vec![s.current.min(count.saturating_sub(1))],
        RangeKind::Custom => parse_range(&s.custom, count)?,
    };
    match s.subset {
        Subset::Odd => list.retain(|p| (p + 1) % 2 == 1),
        Subset::Even => list.retain(|p| (p + 1) % 2 == 0),
        Subset::All => {}
    }
    if s.reverse {
        list.reverse();
    }
    if list.is_empty() {
        return Err("Aucune page ne correspond à cette sélection.".into());
    }
    Ok(list)
}

fn sheet_dims(paper: Paper, landscape: bool) -> (f32, f32) {
    let (w, h) = paper.size();
    if landscape {
        (h, w)
    } else {
        (w, h)
    }
}

/// Place une page (pw × ph) dans une case, centrée, avec l'échelle donnée.
fn place(page: u32, pw: f32, ph: f32, cx: f32, cy: f32, cw: f32, ch: f32, scale: f32) -> Placement {
    let (w, h) = (pw * scale, ph * scale);
    Placement { page, x: cx + (cw - w) / 2.0, y: cy + (ch - h) / 2.0, w, h }
}

fn fit_scale(pw: f32, ph: f32, cw: f32, ch: f32) -> f32 {
    (cw / pw).min(ch / ph)
}

fn empty_sheet(w: f32, h: f32) -> Sheet {
    Sheet { w, h, items: Vec::new(), frames: Vec::new(), marks: Vec::new(), blank: false, tile: None }
}

/// Grille du mode « Multiple » : (colonnes, rangées) et orientation automatique.
pub fn multi_grid(per: u32, landscape: bool) -> (u32, u32) {
    match per {
        2 => if landscape { (2, 1) } else { (1, 2) },
        6 => if landscape { (3, 2) } else { (2, 3) },
        9 => (3, 3),
        _ => (2, 2),
    }
}

/// Calcule l'imposition complète.
///
/// `sizes` : taille affichée de chaque page du document (rotation propre incluse) ;
/// `margin` : marge non imprimable de l'imprimante, en points (0 pour un fichier PDF).
pub fn compute(s: &PrintSettings, sizes: &[(f32, f32)], margin: f32) -> Layout {
    let count = sizes.len() as u32;
    let pages = match select_pages(s, count) {
        Ok(p) => p,
        Err(e) => return Layout { pages: Vec::new(), sheets: Vec::new(), sides: 0, physical_per_copy: 0, error: Some(e) },
    };
    let m = margin.max(0.0);
    let mut sheets = Vec::new();
    let landscape_for = |auto: bool| match s.orient {
        Orient::Portrait => false,
        Orient::Landscape => true,
        Orient::Auto => auto,
    };

    match s.mode {
        Mode::Size => {
            for &p in &pages {
                let (pw, ph) = sizes[p as usize];
                let (w, h) = sheet_dims(s.paper, landscape_for(pw > ph));
                let (cw, ch) = (w - 2.0 * m, h - 2.0 * m);
                let fit = fit_scale(pw, ph, cw, ch);
                let scale = match s.fit {
                    Fit::Fit => fit,
                    Fit::Actual => 1.0,
                    Fit::Shrink => fit.min(1.0),
                    Fit::Custom => (s.scale.clamp(10, 400) as f32) / 100.0,
                };
                let mut sheet = empty_sheet(w, h);
                sheet.items.push(place(p, pw, ph, m, m, cw, ch, scale));
                sheets.push(sheet);
            }
        }
        Mode::Multi => {
            let per = match s.per_sheet { 2 | 4 | 6 | 9 => s.per_sheet, _ => 4 };
            let land = landscape_for(per == 2 || per == 6);
            let (w, h) = sheet_dims(s.paper, land);
            let (cols, rows) = multi_grid(per, land);
            let gap = 10.0;
            let cw = (w - 2.0 * m - gap * (cols as f32 - 1.0)) / cols as f32;
            let ch = (h - 2.0 * m - gap * (rows as f32 - 1.0)) / rows as f32;
            for chunk in pages.chunks(per as usize) {
                let mut sheet = empty_sheet(w, h);
                for (i, &p) in chunk.iter().enumerate() {
                    let (pw, ph) = sizes[p as usize];
                    let (c, r) = (i as u32 % cols, i as u32 / cols);
                    let cx = m + c as f32 * (cw + gap);
                    let cy = m + r as f32 * (ch + gap);
                    let pl = place(p, pw, ph, cx, cy, cw, ch, fit_scale(pw, ph, cw, ch));
                    if s.borders {
                        sheet.frames.push([pl.x, pl.y, pl.w, pl.h]);
                    }
                    sheet.items.push(pl);
                }
                sheets.push(sheet);
            }
        }
        Mode::Booklet => {
            // Imposition : pages complétées à un multiple de 4 ; chaque face porte
            // (dernière, première), (deuxième, avant-dernière), etc.
            let land = landscape_for(true);
            let (w, h) = sheet_dims(s.paper, land);
            let mut list: Vec<Option<u32>> = pages.iter().map(|&p| Some(p)).collect();
            while list.len() % 4 != 0 {
                list.push(None);
            }
            let n = list.len();
            let (cols, rows) = if land { (2u32, 1u32) } else { (1, 2) };
            let cw = (w - 2.0 * m) / cols as f32;
            let ch = (h - 2.0 * m) / rows as f32;
            for i in 0..n / 2 {
                let pair = if i % 2 == 0 { [list[n - 1 - i], list[i]] } else { [list[i], list[n - 1 - i]] };
                let pair = if s.bind_right { [pair[1], pair[0]] } else { pair };
                let mut sheet = empty_sheet(w, h);
                for (k, p) in pair.iter().enumerate() {
                    if let Some(p) = *p {
                        let (pw, ph) = sizes[p as usize];
                        let (c, r) = if land { (k as u32, 0) } else { (0, k as u32) };
                        let cx = m + c as f32 * cw;
                        let cy = m + r as f32 * ch;
                        sheet.items.push(place(p, pw, ph, cx, cy, cw, ch, fit_scale(pw, ph, cw, ch)));
                    }
                }
                sheet.blank = sheet.items.is_empty();
                sheets.push(sheet);
            }
        }
        Mode::Poster => {
            let k = (s.tile.clamp(100, 800) as f32) / 100.0;
            for &p in &pages {
                let (pw, ph) = sizes[p as usize];
                let (bw, bh) = (pw * k, ph * k);
                let (w, h) = sheet_dims(s.paper, landscape_for(pw > ph));
                let (aw, ah) = (w - 2.0 * m, h - 2.0 * m);
                let cols = (bw / aw - 0.001).ceil().max(1.0) as u32;
                let rows = (bh / ah - 0.001).ceil().max(1.0) as u32;
                for r in 0..rows {
                    for c in 0..cols {
                        let mut sheet = empty_sheet(w, h);
                        sheet.items.push(Placement { page: p, x: m - c as f32 * aw, y: m - r as f32 * ah, w: bw, h: bh });
                        sheet.tile = Some([c, r, cols, rows]);
                        if s.cut_marks {
                            sheet.marks = crop_marks(m, m, aw, ah);
                        }
                        sheets.push(sheet);
                    }
                }
            }
        }
    }

    let sides = sheets.len() as u32;
    let physical_per_copy = if s.duplex { sides.div_ceil(2) } else { sides };
    Layout { pages, sheets, sides, physical_per_copy, error: None }
}

/// Repères de découpe aux quatre coins d'un rectangle.
fn crop_marks(x: f32, y: f32, w: f32, h: f32) -> Vec<Line> {
    let l = 12.0;
    let mut v = Vec::new();
    for (cx, cy, dx, dy) in [(x, y, 1.0, 1.0), (x + w, y, -1.0, 1.0), (x, y + h, 1.0, -1.0), (x + w, y + h, -1.0, -1.0)] {
        v.push(Line { x1: cx, y1: cy, x2: cx + dx * l, y2: cy });
        v.push(Line { x1: cx, y1: cy, x2: cx, y2: cy + dy * l });
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    const A4: (f32, f32) = (595.0, 842.0);

    fn s() -> PrintSettings {
        PrintSettings::default()
    }

    #[test]
    fn plages_personnalisees() {
        assert_eq!(parse_range("1-3, 5", 10).unwrap(), vec![0, 1, 2, 4]);
        assert_eq!(parse_range("8-", 10).unwrap(), vec![7, 8, 9]);
        assert_eq!(parse_range("2–3", 10).unwrap(), vec![1, 2]);
        assert!(parse_range("0-2", 10).is_err());
        assert!(parse_range("4-2", 10).is_err());
        assert!(parse_range("11", 10).is_err());
        assert!(parse_range("abc", 10).is_err());
        assert_eq!(parse_range("12", 10).unwrap_err(), "Plage invalide — pages 1 à 10.");
    }

    #[test]
    fn paires_impaires_et_ordre_inverse() {
        let mut st = s();
        st.subset = Subset::Even;
        st.reverse = true;
        assert_eq!(select_pages(&st, 6).unwrap(), vec![5, 3, 1]);
        st.subset = Subset::Odd;
        st.range = RangeKind::Custom;
        st.custom = "2".into();
        assert!(select_pages(&st, 6).is_err());
    }

    #[test]
    fn ajuster_remplit_la_zone_imprimable() {
        let l = compute(&s(), &[A4], 10.0);
        let it = l.sheets[0].items[0];
        assert!((it.w - (595.28 - 20.0)).abs() < 1.0 || (it.h - (841.89 - 20.0)).abs() < 1.0);
        let mut st = s();
        st.fit = Fit::Actual;
        let it = compute(&st, &[A4], 10.0).sheets[0].items[0];
        assert_eq!((it.w, it.h), A4);
    }

    #[test]
    fn orientation_automatique() {
        let l = compute(&s(), &[(842.0, 595.0)], 0.0);
        assert!(l.sheets[0].w > l.sheets[0].h, "page paysage → feuille paysage");
    }

    #[test]
    fn plusieurs_pages_par_feuille() {
        let mut st = s();
        st.mode = Mode::Multi;
        st.per_sheet = 6;
        let l = compute(&st, &vec![A4; 13], 0.0);
        assert_eq!(l.sides, 3);
        assert_eq!(l.sheets[2].items.len(), 1);
        assert_eq!(l.sheets[0].frames.len(), 6);
        // 6 par feuille → paysage, 3 colonnes × 2 rangées.
        assert!(l.sheets[0].w > l.sheets[0].h);
        assert!(l.sheets[0].items[1].x > l.sheets[0].items[0].x);
        assert!(l.sheets[0].items[3].y > l.sheets[0].items[0].y);
    }

    #[test]
    fn brochure_imposee() {
        let mut st = s();
        st.mode = Mode::Booklet;
        let l = compute(&st, &vec![A4; 8], 0.0);
        let pairs: Vec<Vec<u32>> = l.sheets.iter().map(|sh| sh.items.iter().map(|i| i.page).collect()).collect();
        assert_eq!(pairs, vec![vec![7, 0], vec![1, 6], vec![5, 2], vec![3, 4]]);
        st.bind_right = true;
        let l = compute(&st, &vec![A4; 8], 0.0);
        assert_eq!(l.sheets[0].items[0].page, 0);
        // 6 pages → complétées à 8 : la première face a une page vierge.
        st.bind_right = false;
        let l = compute(&st, &vec![A4; 6], 0.0);
        assert_eq!(l.sides, 4);
        assert_eq!(l.sheets[0].items.len(), 1);
    }

    #[test]
    fn affiche_en_tuiles() {
        let mut st = s();
        st.mode = Mode::Poster;
        st.tile = 200;
        let l = compute(&st, &[A4], 0.0);
        assert_eq!(l.sides, 4, "200 % d'une A4 sur A4 → 2 × 2 tuiles");
        assert_eq!(l.sheets[3].tile, Some([1, 1, 2, 2]));
        assert!(l.sheets[3].items[0].x < 0.0);
        assert_eq!(l.sheets[0].marks.len(), 8);
    }

    #[test]
    fn feuilles_physiques_en_recto_verso() {
        let mut st = s();
        st.duplex = true;
        let l = compute(&st, &vec![A4; 5], 0.0);
        assert_eq!(l.sides, 5);
        assert_eq!(l.physical_per_copy, 3);
    }

    #[test]
    fn erreur_de_plage_remontee() {
        let mut st = s();
        st.range = RangeKind::Custom;
        st.custom = "1-50".into();
        let l = compute(&st, &[A4], 0.0);
        assert!(l.error.is_some());
        assert!(l.sheets.is_empty());
    }
}
