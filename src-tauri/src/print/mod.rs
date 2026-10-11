//! Impression : imposition (`layout`), composition du PDF imposé (`compose`)
//! et envoi à une imprimante Windows (`windows`).
//!
//! Une tâche d'impression avance par petites étapes exécutées par le thread
//! moteur entre deux rendus de page : l'interface reste fluide, la progression
//! est publiée en continu et la tâche peut être annulée à tout moment.

pub mod compose;
pub mod layout;
#[cfg(windows)]
pub mod windows;

use std::path::PathBuf;

use pdfium_render::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use compose::{CommentEntry, Composer};
use layout::{Layout, PrintSettings};

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PrintTarget {
    /// « Enregistrer au format PDF ».
    Pdf { path: String },
    /// Imprimante Windows. `file` : sortie dans un fichier (imprimantes virtuelles, tests).
    Printer {
        name: String,
        #[serde(default)]
        file: Option<String>,
    },
}

enum Phase {
    Compose,
    Summary,
    Output,
}

pub enum StepResult {
    Progress(Value),
    /// En attente de l'imprimante (pilote, file pleine) : rien à faire pour l'instant.
    Wait,
    /// Étape faite, sans progression à publier.
    Continue,
    Done(Value),
}

/// État de l'envoi à l'imprimante (thread GDI séparé).
#[cfg(windows)]
struct Sending {
    gdi: windows::GdiThread,
    dpi: Option<u32>,
    /// Face déjà rendue, en attente de place dans la file d'envoi.
    pending: Option<windows::GdiCmd>,
    finishing: bool,
    reported: usize,
}

pub struct PrintJob {
    pub id: u64,
    pub doc: u32,
    doc_name: String,
    settings: PrintSettings,
    layout: Layout,
    target: PrintTarget,
    composer: Composer,
    comments: Vec<CommentEntry>,
    phase: Phase,
    next_sheet: usize,
    /// Ordre d'envoi des faces (index de page du PDF imposé, ou face blanche).
    sequence: Vec<Option<u32>>,
    pos: usize,
    #[cfg(windows)]
    sending: Option<Sending>,
}

/// Ordre d'impression des faces selon les copies, l'assemblage et le recto verso.
///
/// En recto verso, les faces vont par paires (recto, verso) : une paire n'est
/// jamais séparée, et une face blanche complète une copie impaire.
pub fn sequence(sides: u32, copies: u32, collate: bool, duplex: bool) -> Vec<Option<u32>> {
    let unit = if duplex { 2 } else { 1 };
    let mut units: Vec<Vec<Option<u32>>> = Vec::new();
    let mut i = 0;
    while i < sides {
        let mut u: Vec<Option<u32>> = (i..(i + unit).min(sides)).map(Some).collect();
        while u.len() < unit as usize {
            u.push(None);
        }
        units.push(u);
        i += unit;
    }
    let copies = copies.clamp(1, 999);
    let mut out = Vec::new();
    if collate {
        for _ in 0..copies {
            for u in &units {
                out.extend(u.iter().copied());
            }
        }
    } else {
        for u in &units {
            for _ in 0..copies {
                out.extend(u.iter().copied());
            }
        }
    }
    out
}

impl PrintJob {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: u64,
        doc: u32,
        pdfium: &'static Pdfium,
        path: &std::path::Path,
        password: Option<&str>,
        doc_name: String,
        settings: PrintSettings,
        layout: Layout,
        target: PrintTarget,
    ) -> AppResult<Self> {
        if let Some(e) = &layout.error {
            return Err(AppError::new("range", e.clone()));
        }
        let composer = Composer::new(pdfium, path, password, settings.comments)?;
        let comments = if settings.summary { composer.collect_comments(&layout.pages) } else { Vec::new() };
        Ok(Self {
            id,
            doc,
            doc_name,
            settings,
            layout,
            target,
            composer,
            comments,
            phase: Phase::Compose,
            next_sheet: 0,
            sequence: Vec::new(),
            pos: 0,
            #[cfg(windows)]
            sending: None,
        })
    }

    fn progress(&self, phase: &str, done: usize, total: usize) -> StepResult {
        StepResult::Progress(json!({ "job": self.id, "phase": phase, "done": done, "total": total }))
    }

    /// Fait avancer la tâche d'une étape.
    pub fn step(&mut self) -> AppResult<StepResult> {
        match self.phase {
            Phase::Compose => {
                let total = self.layout.sheets.len();
                if self.next_sheet < total {
                    let sheet = self.layout.sheets[self.next_sheet].clone();
                    self.composer.add_sheet(&sheet)?;
                    self.next_sheet += 1;
                    return Ok(self.progress("compose", self.next_sheet, total));
                }
                self.phase = if self.settings.summary { Phase::Summary } else { Phase::Output };
                Ok(self.progress("compose", total, total))
            }
            Phase::Summary => {
                let paper = self.settings.paper.size();
                self.composer.add_summary(&self.comments, paper, &self.doc_name)?;
                self.phase = Phase::Output;
                Ok(self.progress("compose", self.layout.sheets.len(), self.layout.sheets.len()))
            }
            Phase::Output => match self.target.clone() {
                PrintTarget::Pdf { path } => {
                    self.composer
                        .out
                        .save_to_file(&PathBuf::from(&path))
                        .map_err(|e| AppError::new("save", format!("Impossible d’enregistrer le fichier PDF ({e}).")))?;
                    Ok(StepResult::Done(json!({ "job": self.id, "ok": true, "path": path })))
                }
                PrintTarget::Printer { name, file } => self.send_step(&name, file.as_deref()),
            },
        }
    }

    #[cfg(windows)]
    fn send_step(&mut self, printer: &str, file: Option<&str>) -> AppResult<StepResult> {
        use crossbeam_channel::{TryRecvError, TrySendError};
        use std::sync::atomic::Ordering;

        // Démarrage : le thread GDI ouvre le document (le pilote peut afficher une fenêtre).
        if self.sending.is_none() {
            let sides = self.composer.page_count();
            self.sequence = sequence(sides, self.settings.copies, self.settings.collate, self.settings.duplex);
            let first_landscape = self.layout.sheets.first().map(|s| s.w > s.h).unwrap_or(false);
            let gdi = windows::spawn(
                printer.to_string(),
                self.doc_name.clone(),
                windows::DriverSettings {
                    paper: self.settings.paper,
                    landscape: first_landscape,
                    duplex: self.settings.duplex,
                    short_edge: self.settings.short_edge,
                    gray: self.settings.gray,
                },
                file.map(str::to_string),
            );
            self.sending = Some(Sending { gdi, dpi: None, pending: None, finishing: false, reported: 0 });
            return Ok(self.progress("send", 0, self.sequence.len()));
        }
        let total = self.sequence.len();
        let st = self.sending.as_mut().expect("envoi démarré");
        if st.dpi.is_none() {
            match st.gdi.ready.try_recv() {
                Ok(Ok(dpi)) => st.dpi = Some(dpi),
                Ok(Err(e)) => return Err(e),
                Err(TryRecvError::Empty) => return Ok(StepResult::Wait),
                Err(TryRecvError::Disconnected) => return Err(AppError::new("printer", "L’imprimante a interrompu l’impression.")),
            }
        }
        // Progression réelle : faces acceptées par le pilote.
        let sent = st.gdi.sent.load(Ordering::Relaxed);
        if sent != st.reported {
            st.reported = sent;
            return Ok(self.progress("send", sent, total));
        }
        // Fin : attendre la clôture du document par le pilote.
        if st.finishing {
            return match st.gdi.done.try_recv() {
                Ok(Ok(())) => Ok(StepResult::Done(json!({ "job": self.id, "ok": true, "printer": printer }))),
                Ok(Err(e)) => Err(e),
                Err(TryRecvError::Empty) => Ok(StepResult::Wait),
                Err(TryRecvError::Disconnected) => Err(AppError::new("printer", "L’imprimante a interrompu l’impression.")),
            };
        }
        // Erreur remontée par le thread GDI en cours de route.
        if let Ok(Err(e)) = st.gdi.done.try_recv() {
            return Err(e);
        }
        // Face en attente de place dans la file.
        if let Some(cmd) = st.pending.take() {
            match st.gdi.tx.try_send(cmd) {
                Ok(()) => {}
                Err(TrySendError::Full(cmd)) => {
                    st.pending = Some(cmd);
                    return Ok(StepResult::Wait);
                }
                Err(TrySendError::Disconnected(_)) => return Err(AppError::new("printer", "L’imprimante a interrompu l’impression.")),
            }
        }
        if self.pos >= total {
            st.finishing = true;
            let _ = st.gdi.tx.send(windows::GdiCmd::Finish);
            return Ok(StepResult::Wait);
        }
        let dpi = st.dpi.unwrap_or(300) as f32;
        let side = self.sequence[self.pos];
        // Une face blanche prend les dimensions de la face précédente.
        let ref_index = side.or_else(|| self.sequence[..self.pos].iter().rev().flatten().next().copied()).unwrap_or(0);
        let page = self.composer.out.pages().get(ref_index as PdfPageIndex).map_err(|e| AppError::new("print", e.to_string()))?;
        let (wpt, hpt) = (page.width().value, page.height().value);
        let (w, h) = (((wpt / 72.0) * dpi).round() as i32, ((hpt / 72.0) * dpi).round() as i32);
        let landscape = wpt > hpt;
        let bytes = if side.is_some() {
            let cfg = PdfRenderConfig::new()
                .set_target_size(w, h)
                .render_annotations(false)
                .render_form_data(false)
                .use_print_quality(true)
                .use_grayscale_rendering(self.settings.gray)
                .clear_before_rendering(true);
            let bmp = page.render_with_config(&cfg).map_err(|e| AppError::new("print", e.to_string()))?;
            bmp.as_raw_bytes()
        } else {
            vec![255u8; (w * h * 4) as usize]
        };
        self.pos += 1;
        let st = self.sending.as_mut().expect("envoi démarré");
        st.pending = Some(windows::GdiCmd::Page { landscape, bgra: bytes, w: w as u32, h: h as u32 });
        Ok(StepResult::Continue)
    }

    #[cfg(not(windows))]
    fn send_step(&mut self, _printer: &str, _file: Option<&str>) -> AppResult<StepResult> {
        Err(AppError::new("printer", "L’impression directe n’est pas encore disponible sur ce système."))
    }
}

#[cfg(test)]
mod tests {
    use super::sequence;

    #[test]
    fn ordre_assemble() {
        assert_eq!(sequence(3, 2, true, false), vec![Some(0), Some(1), Some(2), Some(0), Some(1), Some(2)]);
    }

    #[test]
    fn ordre_non_assemble() {
        assert_eq!(sequence(2, 2, false, false), vec![Some(0), Some(0), Some(1), Some(1)]);
    }

    #[test]
    fn recto_verso_impair_complete_par_une_face_blanche() {
        assert_eq!(sequence(3, 2, true, true), vec![Some(0), Some(1), Some(2), None, Some(0), Some(1), Some(2), None]);
        // Non assemblé : les paires recto/verso restent ensemble.
        assert_eq!(sequence(2, 2, false, true), vec![Some(0), Some(1), Some(0), Some(1)]);
    }
}
