//! Conversion entre l'espace utilisateur PDF (origine en bas à gauche, page non
//! tournée) et les points d'affichage de l'interface (origine en haut à gauche
//! de la CropBox, rotation propre de la page `/Rotate` appliquée).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn from_corners(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Rect { x: x1.min(x2), y: y1.min(y2), w: (x2 - x1).abs(), h: (y2 - y1).abs() }
    }
    pub fn union(&self, o: &Rect) -> Rect {
        Rect::from_corners(self.x.min(o.x), self.y.min(o.y), (self.x + self.w).max(o.x + o.w), (self.y + self.h).max(o.y + o.h))
    }
    pub fn inflate(&self, d: f32) -> Rect {
        Rect { x: self.x - d, y: self.y - d, w: self.w + 2.0 * d, h: self.h + 2.0 * d }
    }
}

/// Géométrie d'une page : CropBox (espace utilisateur) et rotation en quarts de tour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageGeom {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub rot: u8,
}

impl PageGeom {
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32, rotate: i64) -> Self {
        let rot = ((rotate.rem_euclid(360)) / 90) as u8;
        PageGeom { x0: x0.min(x1), y0: y0.min(y1), x1: x0.max(x1), y1: y0.max(y1), rot }
    }

    /// Point utilisateur → point d'affichage.
    pub fn to_display(&self, ux: f32, uy: f32) -> (f32, f32) {
        match self.rot {
            1 => (uy - self.y0, ux - self.x0),
            2 => (self.x1 - ux, uy - self.y0),
            3 => (self.y1 - uy, self.x1 - ux),
            _ => (ux - self.x0, self.y1 - uy),
        }
    }

    /// Point d'affichage → point utilisateur.
    pub fn to_user(&self, dx: f32, dy: f32) -> (f32, f32) {
        match self.rot {
            1 => (dy + self.x0, dx + self.y0),
            2 => (self.x1 - dx, dy + self.y0),
            3 => (self.x1 - dy, self.y1 - dx),
            _ => (dx + self.x0, self.y1 - dy),
        }
    }

    pub fn rect_to_display(&self, r: &Rect) -> Rect {
        let (ax, ay) = self.to_display(r.x, r.y);
        let (bx, by) = self.to_display(r.x + r.w, r.y + r.h);
        Rect::from_corners(ax, ay, bx, by)
    }

    pub fn rect_to_user(&self, r: &Rect) -> Rect {
        let (ax, ay) = self.to_user(r.x, r.y);
        let (bx, by) = self.to_user(r.x + r.w, r.y + r.h);
        Rect::from_corners(ax, ay, bx, by)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aller_retour_pour_chaque_rotation() {
        for rot in [0, 90, 180, 270] {
            let g = PageGeom::new(10.0, 20.0, 610.0, 820.0, rot);
            for (x, y) in [(10.0, 20.0), (300.0, 400.0), (610.0, 820.0)] {
                let (dx, dy) = g.to_display(x, y);
                let (ux, uy) = g.to_user(dx, dy);
                assert!((ux - x).abs() < 1e-3 && (uy - y).abs() < 1e-3, "rotation {rot}");
            }
        }
    }

    #[test]
    fn coin_haut_gauche() {
        let g = PageGeom::new(0.0, 0.0, 600.0, 800.0, 0);
        assert_eq!(g.to_display(0.0, 800.0), (0.0, 0.0));
        // Quart de tour horaire : le coin haut-gauche passe en haut à droite.
        let g = PageGeom::new(0.0, 0.0, 600.0, 800.0, 90);
        assert_eq!(g.to_display(0.0, 800.0), (800.0, 0.0));
        assert_eq!(g.to_display(0.0, 0.0), (0.0, 0.0));
    }
}
