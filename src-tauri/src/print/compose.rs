//! Composition du PDF imposé : chaque feuille devient une page vectorielle où
//! les pages sources sont placées (formulaires XObject) selon l'imposition.
//! Ce PDF sert tel quel pour « Enregistrer au format PDF », et il est rendu
//! feuille par feuille pour l'impression.

use std::collections::HashSet;
use std::path::Path;

use pdfium_render::prelude::*;

use super::layout::{Comments, Sheet};
use crate::error::{AppError, AppResult};

/// Un commentaire du document, pour le résumé imprimé.
#[derive(Debug, Clone)]
pub struct CommentEntry {
    pub page: u32,
    pub kind: &'static str,
    pub author: String,
    pub text: String,
}

pub struct Composer {
    src: PdfDocument<'static>,
    pub out: PdfDocument<'static>,
    prepared: HashSet<u32>,
    comments: Comments,
}

fn internal(e: impl std::fmt::Display) -> AppError {
    AppError::new("print", format!("Erreur pendant la préparation de l’impression ({e})."))
}

fn kind_label(t: PdfPageAnnotationType) -> &'static str {
    use PdfPageAnnotationType as T;
    match t {
        T::Text => "Note",
        T::FreeText => "Zone de texte",
        T::Highlight => "Surlignage",
        T::Underline => "Soulignement",
        T::Strikeout => "Barré",
        T::Squiggly => "Soulignement ondulé",
        T::Stamp => "Tampon",
        T::Ink => "Dessin",
        T::Square => "Rectangle",
        T::Circle => "Ellipse",
        T::Line => "Ligne",
        T::Polygon | T::Polyline => "Forme",
        _ => "Annotation",
    }
}

/// L'annotation doit-elle apparaître à l'impression, selon le choix « Commentaires et formulaires » ?
fn keep_annotation(t: PdfPageAnnotationType, mode: Comments) -> bool {
    use PdfPageAnnotationType as T;
    match t {
        T::Popup | T::Link => false,
        T::Widget | T::XfaWidget => true,
        T::Stamp => mode != Comments::Doc,
        _ => mode == Comments::Docann,
    }
}

impl Composer {
    /// Ouvre une copie privée du document (modifiable sans toucher à l'original).
    pub fn new(pdfium: &'static Pdfium, path: &Path, password: Option<&str>, comments: Comments) -> AppResult<Self> {
        let src = pdfium.load_pdf_from_file(path, password).map_err(|e| crate::engine::load_error_pub(e, password.is_some()))?;
        let out = pdfium.create_new_pdf().map_err(internal)?;
        Ok(Self { src, out, prepared: HashSet::new(), comments })
    }

    /// Commentaires des pages sélectionnées (avant l'aplatissement, qui les supprime).
    pub fn collect_comments(&self, pages: &[u32]) -> Vec<CommentEntry> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for &p in pages {
            if !seen.insert(p) {
                continue;
            }
            let Ok(page) = self.src.pages().get(p as PdfPageIndex) else { continue };
            for a in page.annotations().iter() {
                let t = a.annotation_type();
                if matches!(t, PdfPageAnnotationType::Popup | PdfPageAnnotationType::Link | PdfPageAnnotationType::Widget) {
                    continue;
                }
                let text = a.contents().unwrap_or_default().trim().to_string();
                if text.is_empty() && t != PdfPageAnnotationType::Stamp {
                    continue;
                }
                out.push(CommentEntry { page: p, kind: kind_label(t), author: a.creator().unwrap_or_default().trim().to_string(), text });
            }
        }
        out
    }

    /// Filtre les annotations puis les aplatit dans le contenu de la page (une fois par page).
    fn prepare(&mut self, idx: u32) -> AppResult<()> {
        if !self.prepared.insert(idx) {
            return Ok(());
        }
        let mut page = self.src.pages().get(idx as PdfPageIndex).map_err(internal)?;
        let n = page.annotations().len();
        for i in 0..n {
            if let Ok(mut a) = page.annotations_mut().get(i) {
                let keep = keep_annotation(a.annotation_type(), self.comments);
                let _ = a.set_is_printed(keep);
                if !keep {
                    let _ = a.set_is_hidden(true);
                }
            }
        }
        if n > 0 {
            page.flatten().map_err(internal)?;
        }
        Ok(())
    }

    /// Matrice qui place la page source `idx` dans le rectangle (x, y, w, h)
    /// d'une feuille de hauteur `sheet_h` (coordonnées « haut-gauche »).
    fn matrix_for(&self, idx: u32, x: f32, y: f32, w: f32, h: f32, sheet_h: f32) -> AppResult<PdfMatrix> {
        let page = self.src.pages().get(idx as PdfPageIndex).map_err(internal)?;
        let crop = page.boundaries().crop().or_else(|_| page.boundaries().media()).map(|b| b.bounds).map_err(internal)?;
        let (x0, y0) = (crop.left().value, crop.bottom().value);
        let (cw, ch) = (crop.width().value, crop.height().value);
        let r = match page.rotation().unwrap_or(PdfPageRenderRotation::None) {
            PdfPageRenderRotation::None => 0,
            PdfPageRenderRotation::Degrees90 => 1,
            PdfPageRenderRotation::Degrees180 => 2,
            PdfPageRenderRotation::Degrees270 => 3,
        };
        // Le XObject créé par PDFium applique déjà la rotation propre de la page (/Rotate) :
        // il suffit de le mettre à l'échelle et de le positionner (vérifié par le test
        // `impression_brochure_et_pages_tournees`).
        let pw = if r % 2 == 1 { ch } else { cw };
        let s = if pw > 0.0 { w / pw } else { 1.0 };
        let (tx, ty) = (x, sheet_h - y - h);
        Ok(PdfMatrix::new(s, 0.0, 0.0, s, tx - s * x0, ty - s * y0))
    }

    /// Ajoute une feuille au PDF imposé.
    pub fn add_sheet(&mut self, sheet: &Sheet) -> AppResult<()> {
        // 1. Préparer les pages sources (filtrage des annotations, aplatissement) et leurs matrices.
        let mut matrices = Vec::new();
        for it in &sheet.items {
            self.prepare(it.page)?;
            matrices.push(self.matrix_for(it.page, it.x, it.y, it.w, it.h, sheet.h)?);
        }
        // 2. Créer la feuille et y déposer les pages (formulaires XObject vectoriels).
        let mut page = self
            .out
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::new_custom(PdfPoints::new(sheet.w), PdfPoints::new(sheet.h)))
            .map_err(internal)?;
        for (it, m) in sheet.items.iter().zip(matrices) {
            let src_page = self.src.pages().get(it.page as PdfPageIndex).map_err(internal)?;
            // SAFETY : conversion de durée de vie uniquement. pdfium-render lie l'emprunt de la
            // page source à la durée de vie du document de destination (`'static`), alors que
            // l'objet créé n'appartient qu'au document de destination et ne référence plus la
            // page source (FPDF_NewXObjectFromPage copie le contenu ; le XObject est refermé).
            // Les deux documents vivent dans ce `Composer`, sur le seul thread PDFium.
            let objects: &PdfPageObjects<'static> = unsafe { std::mem::transmute(src_page.objects()) };
            let mut obj = objects.copy_into_x_object_form_object(&mut self.out).map_err(internal)?;
            obj.apply_matrix(m).map_err(internal)?;
            page.objects_mut().add_object(obj).map_err(internal)?;
        }
        let gray = PdfColor::new(150, 150, 150, 255);
        for f in &sheet.frames {
            let rect = PdfRect::new_from_values(sheet.h - f[1] - f[3], f[0], sheet.h - f[1], f[0] + f[2]);
            page.objects_mut().create_path_object_rect(rect, Some(gray), Some(PdfPoints::new(0.5)), None).map_err(internal)?;
        }
        let black = PdfColor::new(0, 0, 0, 255);
        for l in &sheet.marks {
            page.objects_mut()
                .create_path_object_line(
                    PdfPoints::new(l.x1),
                    PdfPoints::new(sheet.h - l.y1),
                    PdfPoints::new(l.x2),
                    PdfPoints::new(sheet.h - l.y2),
                    black,
                    PdfPoints::new(0.5),
                )
                .map_err(internal)?;
        }
        Ok(())
    }

    /// Ajoute les pages « Résumé des commentaires ». Renvoie le nombre de pages ajoutées.
    pub fn add_summary(&mut self, entries: &[CommentEntry], paper: (f32, f32), doc_name: &str) -> AppResult<u32> {
        let (w, h) = paper;
        let margin = 56.0;
        let regular = self.out.fonts_mut().helvetica();
        let bold = self.out.fonts_mut().helvetica_bold();
        // Lignes à écrire : (texte, gras, taille, espace avant).
        let mut lines: Vec<(String, bool, f32, f32)> = vec![
            ("Résumé des commentaires".into(), true, 18.0, 0.0),
            (doc_name.to_string(), false, 10.0, 6.0),
        ];
        if entries.is_empty() {
            lines.push(("Aucun commentaire dans les pages imprimées.".into(), false, 11.0, 18.0));
        }
        let max_chars = ((w - 2.0 * margin) / 5.2) as usize;
        for e in entries {
            let mut head = format!("Page {} · {}", e.page + 1, e.kind);
            if !e.author.is_empty() {
                head.push_str(&format!(" · {}", e.author));
            }
            lines.push((head, true, 10.5, 14.0));
            for l in wrap(&e.text, max_chars) {
                lines.push((l, false, 10.0, 2.0));
            }
        }
        let mut added = 0;
        let mut i = 0;
        while i < lines.len() {
            let mut page = self
                .out
                .pages_mut()
                .create_page_at_end(PdfPagePaperSize::new_custom(PdfPoints::new(w), PdfPoints::new(h)))
                .map_err(internal)?;
            added += 1;
            let mut y = h - margin;
            while i < lines.len() {
                let (text, is_bold, size, before) = &lines[i];
                let need = before + size * 1.35;
                if y - need < margin && y < h - margin {
                    break;
                }
                y -= need;
                page.objects_mut()
                    .create_text_object(PdfPoints::new(margin), PdfPoints::new(y), text, if *is_bold { bold } else { regular }, PdfPoints::new(*size))
                    .map_err(internal)?;
                i += 1;
            }
        }
        Ok(added)
    }

    pub fn page_count(&self) -> u32 {
        self.out.pages().len() as u32
    }
}

/// Découpe un texte en lignes d'au plus `max` caractères, sur les espaces.
pub fn wrap(text: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.lines() {
        let mut line = String::new();
        for word in para.split_whitespace() {
            if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > max {
                out.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoupage_en_lignes() {
        let l = wrap("un deux trois quatre cinq", 9);
        assert_eq!(l, vec!["un deux", "trois", "quatre", "cinq"]);
    }

    #[test]
    fn filtrage_des_annotations() {
        use PdfPageAnnotationType as T;
        assert!(keep_annotation(T::Widget, Comments::Doc));
        assert!(!keep_annotation(T::Highlight, Comments::Doc));
        assert!(keep_annotation(T::Stamp, Comments::Stamps));
        assert!(!keep_annotation(T::Ink, Comments::Stamps));
        assert!(keep_annotation(T::Ink, Comments::Docann));
        assert!(!keep_annotation(T::Popup, Comments::Docann));
    }
}
