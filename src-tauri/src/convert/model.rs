//! Contenu d'une page prêt à convertir : lignes de texte (segments mis en
//! forme, en points d'affichage) et, si demandé, l'image de la page.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Span {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Taille de police (points).
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    /// « rrggbb ».
    pub color: String,
    /// sans, serif, mono.
    pub family: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Line {
    pub spans: Vec<Span>,
}

impl Line {
    pub fn x(&self) -> f32 {
        self.spans.iter().map(|s| s.x).fold(f32::MAX, f32::min)
    }
    pub fn y(&self) -> f32 {
        self.spans.iter().map(|s| s.y).fold(f32::MAX, f32::min)
    }
    pub fn bottom(&self) -> f32 {
        self.spans.iter().map(|s| s.y + s.h).fold(f32::MIN, f32::max)
    }
    pub fn right(&self) -> f32 {
        self.spans.iter().map(|s| s.x + s.w).fold(f32::MIN, f32::max)
    }
    pub fn size(&self) -> f32 {
        // Taille dominante (pondérée par la longueur du texte).
        let mut best = (0.0f32, 0usize);
        for s in &self.spans {
            let n = s.text.chars().count();
            if n > best.1 {
                best = (s.size, n);
            }
        }
        best.0
    }
    pub fn text(&self) -> String {
        let mut out = String::new();
        for (i, s) in self.spans.iter().enumerate() {
            if i > 0 {
                let prev = &self.spans[i - 1];
                let gap = s.x - (prev.x + prev.w);
                if gap > prev.size * 0.15 && !out.ends_with(' ') && !s.text.starts_with(' ') {
                    out.push(if gap > prev.size * 1.5 { '\t' } else { ' ' });
                }
            }
            out += &s.text;
        }
        out.trim_end().to_string()
    }
}

/// Image d'une page (RVBA).
#[derive(Debug, Clone)]
pub struct PageImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Illustration de la page (image ou graphique vectoriel), rendue en PNG.
#[derive(Debug, Clone)]
pub struct Figure {
    /// Position et taille en points (repère d'affichage, origine en haut à gauche).
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub png: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct PageData {
    /// Taille de la page en points (rotation appliquée).
    pub w: f32,
    pub h: f32,
    pub lines: Vec<Line>,
    pub image: Option<PageImage>,
    /// Illustrations (export Word) ; les lignes qu'elles contiennent en sont retirées.
    pub figures: Vec<Figure>,
}

impl PageData {
    /// Assez de texte pour se passer d'OCR.
    pub fn has_text(&self) -> bool {
        self.lines.iter().map(|l| l.text().trim().chars().count()).sum::<usize>() >= 8
    }
}
