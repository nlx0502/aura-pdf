//! Boucle du thread moteur : seul endroit du programme qui appelle PDFium.

use std::cmp::Ordering as CmpOrdering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crossbeam_channel::{Receiver, TryRecvError};
use parking_lot::RwLock;
use pdfium_render::prelude::*;
use serde_json::json;

use super::textsearch::{find_all, fold_char, fold_query, snippet};
use super::*;
use crate::error::{AppError, AppResult};

/// Facteur d'échelle utilisé pour convertir les coordonnées de page en points
/// d'affichage avec une précision sub-pixel.
const COORD_SCALE: f32 = 8.0;

struct OpenDoc {
    doc: PdfDocument<'static>,
    gen: Arc<AtomicU64>,
    path: PathBuf,
    /// Mot de passe utilisé à l'ouverture (copie privée pour l'impression).
    password: Option<String>,
    /// Annotations d'origine masquées dans le rendu (page → index dans /Annots).
    hidden: HashMap<u32, std::collections::HashSet<u32>>,
    /// Copie affichée à la place du fichier (visibilité des calques modifiée).
    render_path: Option<PathBuf>,
    /// Le contenu a été modifié en mémoire (module Éditer).
    content_dirty: bool,
}

#[path = "editing.rs"]
mod editing;
#[path = "extract.rs"]
mod extract;
pub use editing::RedactHit;

struct SearchState {
    doc: u32,
    token: u64,
    needle: Vec<char>,
    next_page: u32,
    page_count: u32,
    total: u32,
}

/// Élément de la file de priorité (priorité croissante, puis plus récent d'abord).
struct Queued {
    seq: u64,
    job: RenderJob,
}

impl PartialEq for Queued {
    fn eq(&self, other: &Self) -> bool {
        self.seq == other.seq
    }
}
impl Eq for Queued {}
impl PartialOrd for Queued {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}
impl Ord for Queued {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        // BinaryHeap est un tas max : la priorité la plus faible doit sortir d'abord.
        other
            .job
            .prio
            .cmp(&self.job.prio)
            .then(self.seq.cmp(&other.seq))
    }
}

struct Worker {
    pdfium: &'static Pdfium,
    docs: HashMap<u32, OpenDoc>,
    next_id: u32,
    gens: Arc<RwLock<HashMap<u32, Arc<AtomicU64>>>>,
    emit: Emitter,
    queue: BinaryHeap<Queued>,
    seq: u64,
    search: Option<SearchState>,
    print: Option<crate::print::PrintJob>,
    /// La tâche d'impression attend l'imprimante : ne pas boucler à vide.
    print_waiting: bool,
}

pub(crate) fn run(
    lib_dir: PathBuf,
    rx: Receiver<Msg>,
    gens: Arc<RwLock<HashMap<u32, Arc<AtomicU64>>>>,
    emit: Emitter,
) {
    let bindings = match Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&lib_dir))
        .or_else(|_| Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./")))
    {
        Ok(b) => b,
        Err(e) => {
            eprintln!("PDFium introuvable : {e}");
            emit("engine:fatal", json!({ "message": "Le moteur PDF (pdfium.dll) est introuvable. Réinstallez l’application." }));
            // Répondre en erreur à toutes les demandes futures.
            for msg in rx.iter() {
                fail(msg, AppError::internal("moteur PDF indisponible"));
            }
            return;
        }
    };
    let pdfium: &'static Pdfium = Box::leak(Box::new(Pdfium::new(bindings)));

    let mut w = Worker {
        pdfium,
        docs: HashMap::new(),
        next_id: 1,
        gens,
        emit,
        queue: BinaryHeap::new(),
        seq: 0,
        search: None,
        print: None,
        print_waiting: false,
    };

    loop {
        let idle = w.queue.is_empty() && w.search.is_none() && w.print.is_none();
        let only_waiting = w.queue.is_empty() && w.search.is_none() && w.print.is_some() && w.print_waiting;
        let msg = if idle {
            match rx.recv() {
                Ok(m) => Some(m),
                Err(_) => break,
            }
        } else if only_waiting {
            // L'imprimante travaille : on se réveille au plus tard dans 15 ms.
            w.print_waiting = false;
            match rx.recv_timeout(std::time::Duration::from_millis(15)) {
                Ok(m) => Some(m),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => None,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match rx.try_recv() {
                Ok(m) => Some(m),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => break,
            }
        };

        if let Some(m) = msg {
            w.handle(m);
            continue;
        }

        // Rien de nouveau : traiter une unité de travail.
        let top_is_visible = w.queue.peek().map(|q| q.job.prio == prio::VISIBLE).unwrap_or(false);
        if w.print.is_some() && !top_is_visible {
            w.print_step();
        } else if w.search.is_some() && !top_is_visible {
            w.search_step();
        } else if let Some(q) = w.queue.pop() {
            w.render(q.job);
        }
    }
}

fn fail(msg: Msg, err: AppError) {
    match msg {
        Msg::Open { reply, .. } => drop(reply.send(Err(err))),
        Msg::Render(job) => (job.responder)(RenderOutcome::Err(err.message)),
        Msg::PageText { reply, .. } => drop(reply.send(Err(err))),
        Msg::PageLinks { reply, .. } => drop(reply.send(Err(err))),
        Msg::SaveAttachment { reply, .. } => drop(reply.send(Err(err))),
        Msg::Thumbnail { reply, .. } => drop(reply.send(Err(err))),
        Msg::Create { reply, .. } => drop(reply.send(Err(err))),
        Msg::PrintStart { reply, .. } => drop(reply.send(Err(err))),
        Msg::AnnotLoad { reply, .. } => drop(reply.send(Err(err))),
        Msg::AnnotSave { reply, .. } => drop(reply.send(Err(err))),
        Msg::EditApply { reply, .. } => drop(reply.send(Err(err))),
        Msg::Reload { reply, .. } => drop(reply.send(Err(err))),
        Msg::Credentials { reply, .. } => drop(reply.send(Err(err))),
        Msg::Extract { reply, .. } => drop(reply.send(Err(err))),
        Msg::EditLinks { reply, .. } => drop(reply.send(Err(err))),
        Msg::RedactFind { reply, .. } => drop(reply.send(Err(err))),
        Msg::Layers { reply, .. } => drop(reply.send(Err(err))),
        Msg::LayersSet { reply, .. } => drop(reply.send(Err(err))),
        Msg::ContentList { reply, .. } => drop(reply.send(Err(err))),
        Msg::ContentApply { reply, .. } => drop(reply.send(Err(err))),
        Msg::Close { .. } | Msg::Search { .. } | Msg::CancelSearch { .. } | Msg::PrintCancel { .. } | Msg::AnnotHide { .. } => {}
    }
}

/// Traduit une erreur de chargement PDFium en message clair.
pub(crate) fn load_error(e: PdfiumError, had_password: bool) -> AppError {
    match e {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            if had_password {
                AppError::wrong_password()
            } else {
                AppError::password_required()
            }
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => AppError::corrupted(),
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FileError) => AppError::file("accès refusé"),
        PdfiumError::IoError(io) => AppError::file(io),
        other => AppError::internal(other),
    }
}

impl Worker {
    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Open { path, password, reply } => {
                let _ = reply.send(self.open(&path, password.as_deref()));
            }
            Msg::Close { doc } => {
                self.docs.remove(&doc);
                if self.search.as_ref().map(|s| s.doc == doc).unwrap_or(false) {
                    self.search = None;
                }
                // Les rendus en attente pour ce document répondront « obsolète ».
            }
            Msg::Render(job) => {
                self.seq += 1;
                self.queue.push(Queued { seq: self.seq, job });
            }
            Msg::Search { doc, query, token } => self.start_search(doc, &query, token),
            Msg::CancelSearch { doc } => {
                if self.search.as_ref().map(|s| s.doc == doc).unwrap_or(false) {
                    self.search = None;
                }
            }
            Msg::PageText { doc, page, reply } => {
                let _ = reply.send(self.page_text(doc, page));
            }
            Msg::PageLinks { doc, page, reply } => {
                let _ = reply.send(self.page_links(doc, page));
            }
            Msg::SaveAttachment { doc, index, dest, reply } => {
                let _ = reply.send(self.save_attachment(doc, index, &dest));
            }
            Msg::Thumbnail { path, out, width, reply } => {
                let _ = reply.send(self.thumbnail(&path, &out, width));
            }
            Msg::Create { spec, out, reply } => {
                let _ = reply.send(crate::create::create(self.pdfium, &spec, &out));
            }
            Msg::PrintStart { doc, job, settings, margin, target, reply } => {
                let _ = reply.send(self.print_start(doc, job, settings, margin, target));
            }
            Msg::AnnotLoad { doc, reply } => {
                let r = self.docs.get(&doc).ok_or_else(AppError::not_found).and_then(|o| crate::annot::load(&o.path, o.password.as_deref()));
                let _ = reply.send(r);
            }
            Msg::AnnotHide { doc, page, indices } => self.annot_hide(doc, page, indices),
            Msg::AnnotSave { doc, annots, reply } => {
                let _ = reply.send(self.annot_save(doc, &annots));
            }
            Msg::Reload { doc, password, reply } => {
                let _ = reply.send(self.reload(doc, password));
            }
            Msg::Extract { doc, page, dpi, text, reply } => {
                let r = self.get(doc).and_then(|d| extract::extract_page(d, page, dpi, text));
                let _ = reply.send(r);
            }
            Msg::Credentials { doc, reply } => {
                let r = self.docs.get(&doc).ok_or_else(AppError::not_found).map(|o| (o.path.clone(), o.password.clone()));
                let _ = reply.send(r);
            }
            Msg::EditApply { doc, plan, out, reply } => {
                let _ = reply.send(self.edit_apply(doc, &plan, out));
            }
            Msg::EditLinks { doc, reply } => {
                let _ = reply.send(self.edit_links(doc));
            }
            Msg::RedactFind { doc, patterns, terms, reply } => {
                let _ = reply.send(self.redact_find(doc, &patterns, &terms));
            }
            Msg::Layers { doc, reply } => {
                let _ = reply.send(self.layers(doc));
            }
            Msg::LayersSet { doc, hidden, reply } => {
                let _ = reply.send(self.layers_set(doc, &hidden));
            }
            Msg::ContentList { doc, page, reply } => {
                let _ = reply.send(self.content_list(doc, page));
            }
            Msg::ContentApply { doc, ops, reset, reply } => {
                let _ = reply.send(self.content_apply(doc, &ops, reset));
            }
            Msg::PrintCancel { job } => {
                if self.print.as_ref().map(|p| p.id == job).unwrap_or(false) {
                    // L'abandon du document GDI se fait à la destruction de la tâche.
                    self.print = None;
                    (self.emit)("print:done", json!({ "job": job, "ok": false, "cancelled": true }));
                }
            }
        }
    }

    // ---------------------------------------------------------------- annotations

    fn annot_hide(&mut self, doc: u32, page: u32, indices: Vec<u32>) {
        let Some(open) = self.docs.get_mut(&doc) else { return };
        let wanted: HashSet<u32> = indices.into_iter().collect();
        let prev = open.hidden.remove(&page).unwrap_or_default();
        if let Ok(mut p) = open.doc.pages().get(page as PdfPageIndex) {
            // Réafficher celles qu'on avait masquées et qui ne le sont plus, masquer les nouvelles.
            for i in prev.difference(&wanted).chain(wanted.difference(&prev)) {
                if let Ok(mut a) = p.annotations_mut().get(*i as PdfPageAnnotationIndex) {
                    let _ = a.set_is_hidden(wanted.contains(i));
                }
            }
        }
        if !wanted.is_empty() {
            open.hidden.insert(page, wanted);
        }
    }

    /// Enregistre les annotations dans le fichier puis recharge le document.
    fn annot_save(&mut self, doc: u32, annots: &[crate::annot::Annot]) -> AppResult<(DocInfo, Vec<crate::annot::Annot>)> {
        let (path, password) = {
            let open = self.docs.get(&doc).ok_or_else(AppError::not_found)?;
            (open.path.clone(), open.password.clone())
        };
        crate::annot::save(&path, password.as_deref(), annots)?;
        // Le fichier a changé : rouvrir le document sous le même identifiant.
        let gen = self.docs.remove(&doc).map(|o| o.gen).unwrap_or_default();
        let meta = std::fs::metadata(&path)?;
        let pdf = self
            .pdfium
            .load_pdf_from_file(&path, password.as_deref())
            .map_err(|e| load_error(e, password.is_some()))?;
        let info = build_info(doc, &path, meta.len(), &pdf)?;
        self.docs.insert(doc, OpenDoc { doc: pdf, gen, path: path.clone(), password: password.clone(), hidden: HashMap::new(), render_path: None, content_dirty: false });
        let fresh = crate::annot::load(&path, password.as_deref())?;
        Ok((info, fresh))
    }

    /// Relit le fichier sous le même identifiant (après une signature, par exemple).
    fn reload(&mut self, doc: u32, new_password: Option<Option<String>>) -> AppResult<DocInfo> {
        let (path, password, gen) = {
            let open = self.docs.get(&doc).ok_or_else(AppError::not_found)?;
            (open.path.clone(), new_password.unwrap_or_else(|| open.password.clone()), open.gen.clone())
        };
        let meta = std::fs::metadata(&path)?;
        let pdf = self.pdfium.load_pdf_from_file(&path, password.as_deref()).map_err(|e| load_error(e, password.is_some()))?;
        let mut info = build_info(doc, &path, meta.len(), &pdf)?;
        with_permissions(&mut info, &path, password.as_deref());
        if let Some(old) = self.docs.remove(&doc) {
            if let Some(r) = old.render_path {
                let _ = std::fs::remove_file(r);
            }
        }
        self.docs.insert(doc, OpenDoc { doc: pdf, gen, path, password, hidden: HashMap::new(), render_path: None, content_dirty: false });
        Ok(info)
    }

    // ---------------------------------------------------------------- impression

    fn print_start(
        &mut self,
        doc: u32,
        job: u64,
        settings: crate::print::layout::PrintSettings,
        margin: f32,
        target: crate::print::PrintTarget,
    ) -> AppResult<()> {
        if self.print.is_some() {
            return Err(AppError::new("busy", "Une impression est déjà en cours."));
        }
        let open = self.docs.get(&doc).ok_or_else(AppError::not_found)?;
        let sizes: Vec<(f32, f32)> = open
            .doc
            .pages()
            .page_sizes()
            .map_err(AppError::internal)?
            .into_iter()
            .map(|r| (r.width().value, r.height().value))
            .collect();
        let layout = crate::print::layout::compute(&settings, &sizes, margin);
        let name = open.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let path = open.path.clone();
        let password = open.password.clone();
        self.print = Some(crate::print::PrintJob::new(job, doc, self.pdfium, &path, password.as_deref(), name, settings, layout, target)?);
        Ok(())
    }

    fn print_step(&mut self) {
        let Some(job) = self.print.as_mut() else { return };
        let id = job.id;
        match job.step() {
            Ok(crate::print::StepResult::Progress(v)) => (self.emit)("print:progress", v),
            Ok(crate::print::StepResult::Continue) => {}
            Ok(crate::print::StepResult::Wait) => self.print_waiting = true,
            Ok(crate::print::StepResult::Done(v)) => {
                self.print = None;
                (self.emit)("print:done", v);
            }
            Err(e) => {
                self.print = None;
                (self.emit)("print:done", json!({ "job": id, "ok": false, "message": e.message }));
            }
        }
    }

    fn get(&self, doc: u32) -> AppResult<&PdfDocument<'static>> {
        self.docs.get(&doc).map(|d| &d.doc).ok_or_else(AppError::not_found)
    }

    // ---------------------------------------------------------------- ouverture

    fn open(&mut self, path: &Path, password: Option<&str>) -> AppResult<DocInfo> {
        let meta = std::fs::metadata(path)?;
        let doc = self
            .pdfium
            .load_pdf_from_file(path, password)
            .map_err(|e| load_error(e, password.is_some()))?;

        let id = self.next_id;
        self.next_id += 1;
        let mut info = build_info(id, path, meta.len(), &doc)?;
        with_permissions(&mut info, path, password);

        let gen = Arc::new(AtomicU64::new(0));
        self.gens.write().insert(id, gen.clone());
        self.docs.insert(id, OpenDoc { doc, gen, path: path.to_path_buf(), password: password.map(str::to_string), hidden: HashMap::new(), render_path: None, content_dirty: false });
        Ok(info)
    }

    // ---------------------------------------------------------------- rendu

    fn render(&mut self, job: RenderJob) {
        let Some(open) = self.docs.get(&job.doc) else {
            (job.responder)(RenderOutcome::Stale);
            return;
        };
        if job.prio != prio::THUMB && job.gen < open.gen.load(Ordering::Relaxed) {
            (job.responder)(RenderOutcome::Stale);
            return;
        }
        let outcome = match render_page(&open.doc, &job) {
            Ok(bmp) => RenderOutcome::Ok(bmp),
            Err(e) => RenderOutcome::Err(e.to_string()),
        };
        (job.responder)(outcome);
    }

    // ---------------------------------------------------------------- recherche

    fn start_search(&mut self, doc: u32, query: &str, token: u64) {
        let needle = fold_query(query);
        let Ok(d) = self.get(doc) else { return };
        let page_count = d.pages().len() as u32;
        if needle.is_empty() {
            self.search = None;
            return;
        }
        self.search = Some(SearchState { doc, token, needle, next_page: 0, page_count, total: 0 });
    }

    fn search_step(&mut self) {
        let Some(mut s) = self.search.take() else { return };
        let Some(open) = self.docs.get(&s.doc) else { return };
        let page_index = s.next_page;
        let hits = search_page(&open.doc, page_index, &s.needle).unwrap_or_default();
        s.total += hits.len() as u32;
        s.next_page += 1;

        let done = s.next_page >= s.page_count;
        if !hits.is_empty() || s.next_page % 16 == 0 || done {
            (self.emit)(
                "search:progress",
                json!({
                    "doc": s.doc,
                    "token": s.token,
                    "hits": hits,
                    "scanned": s.next_page,
                    "pageCount": s.page_count,
                    "total": s.total,
                }),
            );
        }
        if done {
            (self.emit)("search:done", json!({ "doc": s.doc, "token": s.token, "total": s.total }));
        } else {
            self.search = Some(s);
        }
    }

    // ---------------------------------------------------------------- texte / liens

    fn page_text(&self, doc: u32, page: u32) -> AppResult<Vec<TextWord>> {
        let d = self.get(doc)?;
        let p = d.pages().get(page as PdfPageIndex).map_err(AppError::internal)?;
        Ok(extract_words(&p))
    }

    fn page_links(&self, doc: u32, page: u32) -> AppResult<Vec<PageLink>> {
        let d = self.get(doc)?;
        let p = d.pages().get(page as PdfPageIndex).map_err(AppError::internal)?;
        let cfg = coord_config();
        let mut out = Vec::new();
        for link in p.links().iter() {
            let Ok(r) = link.rect() else { continue };
            let rect = to_display_rect(&p, &cfg, r);
            let mut target_page = link.destination().and_then(|d| d.page_index().ok()).map(|i| i as u32);
            let mut uri = None;
            if let Some(action) = link.action() {
                if let Some(local) = action.as_local_destination_action() {
                    if let Ok(dest) = local.destination() {
                        target_page = dest.page_index().ok().map(|i| i as u32);
                    }
                } else if let Some(u) = action.as_uri_action() {
                    uri = u.uri().ok();
                }
            }
            if target_page.is_some() || uri.is_some() {
                out.push(PageLink { rect, page: target_page, uri });
            }
        }
        Ok(out)
    }

    fn save_attachment(&self, doc: u32, index: u32, dest: &Path) -> AppResult<()> {
        let d = self.get(doc)?;
        let a = d
            .attachments()
            .get(index as PdfAttachmentIndex)
            .map_err(AppError::internal)?;
        a.save_to_file(dest).map_err(AppError::internal)
    }

    // ---------------------------------------------------------------- miniatures de l'accueil

    fn thumbnail(&self, path: &Path, out: &Path, width: u32) -> AppResult<FileSummary> {
        let doc = self
            .pdfium
            .load_pdf_from_file(path, None)
            .map_err(|e| load_error(e, false))?;
        let page_count = doc.pages().len() as u32;
        let summary = FileSummary {
            page_count,
            signed: doc.signatures().len() > 0,
            form: doc.form().is_some(),
            scanned: looks_scanned(&doc),
        };
        if !out.exists() {
            let page = doc.pages().get(0).map_err(AppError::internal)?;
            let cfg = PdfRenderConfig::new()
                .set_target_width(width as Pixels)
                .set_maximum_height((width * 2) as Pixels)
                .render_form_data(true)
                .render_annotations(true);
            let bmp = page.render_with_config(&cfg).map_err(AppError::internal)?;
            let img = bmp.as_image().map_err(AppError::internal)?;
            if let Some(dir) = out.parent() {
                std::fs::create_dir_all(dir)?;
            }
            img.into_rgb8()
                .save_with_format(out, image::ImageFormat::Jpeg)
                .map_err(AppError::internal)?;
        }
        Ok(summary)
    }
}

// -------------------------------------------------------------------- fonctions utilitaires

fn coord_config() -> PdfRenderConfig {
    PdfRenderConfig::new().scale_page_by_factor(COORD_SCALE)
}

/// Convertit un rectangle de l'espace page (origine en bas à gauche, non tourné)
/// en points d'affichage (origine en haut à gauche, rotation de la page appliquée).
fn to_display_rect(page: &PdfPage, cfg: &PdfRenderConfig, r: PdfRect) -> Rect {
    let a = page.points_to_pixels(r.left(), r.top(), cfg).unwrap_or((0, 0));
    let b = page.points_to_pixels(r.right(), r.bottom(), cfg).unwrap_or((0, 0));
    let (x0, x1) = (a.0.min(b.0) as f32, a.0.max(b.0) as f32);
    let (y0, y1) = (a.1.min(b.1) as f32, a.1.max(b.1) as f32);
    Rect {
        x: x0 / COORD_SCALE,
        y: y0 / COORD_SCALE,
        w: (x1 - x0) / COORD_SCALE,
        h: (y1 - y0) / COORD_SCALE,
    }
}

fn render_page(doc: &PdfDocument, job: &RenderJob) -> Result<RenderedBitmap, PdfiumError> {
    let page = doc.pages().get(job.page as PdfPageIndex)?;
    let rotation = match job.rotation % 4 {
        1 => PdfPageRenderRotation::Degrees90,
        2 => PdfPageRenderRotation::Degrees180,
        3 => PdfPageRenderRotation::Degrees270,
        _ => PdfPageRenderRotation::None,
    };
    if let Some([tx, ty, tw, th]) = job.tile {
        // Tuile d'une page rendue en très grand (zoom élevé) : seul le morceau
        // visible est calculé et alloué.
        let width = job.width_px.clamp(16, 60_000) as Pixels;
        let (tw, th) = (tw.clamp(1, 4096) as Pixels, th.clamp(1, 4096) as Pixels);
        let mut bmp = PdfBitmap::empty(tw, th, PdfBitmapFormat::BGRA)?;
        let cfg = PdfRenderConfig::new()
            .set_target_width(width)
            .set_maximum_height(1_000_000)
            .rotate(rotation, true)
            .render_annotations(job.annotations)
            .render_form_data(job.forms)
            .set_reverse_byte_order(true)
            .clear_before_rendering(true)
            .set_origin(-(tx as Pixels), -(ty as Pixels));
        page.render_into_bitmap_with_config(&mut bmp, &cfg)?;
        return Ok(RenderedBitmap { width: tw as u32, height: th as u32, rgba: bmp.as_raw_bytes() });
    }

    // Page entière : plafond de sécurité (les grands zooms passent par les tuiles).
    let width = job.width_px.clamp(16, 8_000) as Pixels;
    let cfg = PdfRenderConfig::new()
        .set_target_width(width)
        .set_maximum_height(12_000)
        .rotate(rotation, true)
        .render_annotations(job.annotations)
        .render_form_data(job.forms)
        .set_reverse_byte_order(true)
        .clear_before_rendering(true);
    let bmp = page.render_with_config(&cfg)?;
    Ok(RenderedBitmap {
        width: bmp.width() as u32,
        height: bmp.height() as u32,
        rgba: bmp.as_raw_bytes(),
    })
}

/// Caractères d'une page et leurs boîtes (en points d'affichage).
fn page_chars(page: &PdfPage) -> (Vec<char>, Vec<Option<Rect>>) {
    let Ok(text) = page.text() else { return (Vec::new(), Vec::new()) };
    let cfg = coord_config();
    let chars = text.chars();
    let mut out_c = Vec::with_capacity(chars.len() as usize);
    let mut out_r = Vec::with_capacity(chars.len() as usize);
    for ch in chars.iter() {
        let c = ch.unicode_char().unwrap_or('\u{FFFD}');
        out_c.push(c);
        let rect = if c.is_whitespace() {
            None
        } else {
            ch.loose_bounds()
                .ok()
                .map(|r| to_display_rect(page, &cfg, r))
                .filter(|r| r.w > 0.0 && r.h > 0.0)
        };
        out_r.push(rect);
    }
    (out_c, out_r)
}

/// Fusionne les boîtes de caractères consécutifs d'une même ligne.
fn merge_line_rects(rects: impl Iterator<Item = Rect>) -> Vec<Rect> {
    let mut out: Vec<Rect> = Vec::new();
    for r in rects {
        if let Some(last) = out.last_mut() {
            let same_line = (r.y - last.y).abs() < last.h * 0.5 && r.x >= last.x - 1.0;
            if same_line {
                let x1 = (last.x + last.w).max(r.x + r.w);
                let y0 = last.y.min(r.y);
                let y1 = (last.y + last.h).max(r.y + r.h);
                last.w = x1 - last.x;
                last.y = y0;
                last.h = y1 - y0;
                continue;
            }
        }
        out.push(r);
    }
    out
}

fn search_page(doc: &PdfDocument, index: u32, needle: &[char]) -> Result<Vec<SearchHit>, PdfiumError> {
    let page = doc.pages().get(index as PdfPageIndex)?;
    let (chars, rects) = page_chars(&page);
    let folded: Vec<char> = chars.iter().map(|&c| fold_char(c)).collect();
    let hits = find_all(&folded, needle)
        .into_iter()
        .map(|(s, e)| {
            let (before, matched, after) = snippet(&chars, s, e, 48);
            SearchHit {
                page: index,
                rects: merge_line_rects(rects[s..e].iter().flatten().copied()),
                before,
                matched,
                after,
            }
        })
        .collect();
    Ok(hits)
}

fn extract_words(page: &PdfPage) -> Vec<TextWord> {
    let (chars, rects) = page_chars(page);
    let mut words: Vec<TextWord> = Vec::new();
    let mut cur = String::new();
    let mut cur_rect: Option<Rect> = None;

    let flush = |words: &mut Vec<TextWord>, cur: &mut String, cur_rect: &mut Option<Rect>, sp: bool, nl: bool| {
        if let Some(r) = cur_rect.take() {
            if !cur.is_empty() {
                words.push(TextWord { t: std::mem::take(cur), x: r.x, y: r.y, w: r.w, h: r.h, sp, nl });
            }
        }
        cur.clear();
    };

    for (i, &c) in chars.iter().enumerate() {
        if c == '\r' || c == '\n' {
            flush(&mut words, &mut cur, &mut cur_rect, false, true);
            if let Some(last) = words.last_mut() {
                last.nl = true;
            }
            continue;
        }
        if c.is_whitespace() {
            flush(&mut words, &mut cur, &mut cur_rect, true, false);
            continue;
        }
        let Some(r) = rects[i] else { continue };
        if let Some(cr) = cur_rect.as_mut() {
            // Nouvelle ligne implicite (le caractère revient en arrière ou change de ligne).
            if (r.y - cr.y).abs() > cr.h * 0.6 || r.x < cr.x - 1.0 {
                flush(&mut words, &mut cur, &mut cur_rect, false, true);
            }
        }
        match cur_rect.as_mut() {
            Some(cr) => {
                let x1 = (cr.x + cr.w).max(r.x + r.w);
                let y0 = cr.y.min(r.y);
                let y1 = (cr.y + cr.h).max(r.y + r.h);
                cr.w = x1 - cr.x;
                cr.y = y0;
                cr.h = y1 - y0;
            }
            None => cur_rect = Some(r),
        }
        cur.push(c);
    }
    flush(&mut words, &mut cur, &mut cur_rect, false, false);
    words
}

fn looks_scanned(doc: &PdfDocument) -> bool {
    let pages = doc.pages();
    let n = pages.len().min(3);
    if n == 0 {
        return false;
    }
    let mut chars = 0;
    for i in 0..n {
        if let Ok(p) = pages.get(i) {
            if let Ok(t) = p.text() {
                chars += t.len();
            }
        }
    }
    chars < 8
}

fn version_label(v: PdfDocumentVersion) -> String {
    match v {
        PdfDocumentVersion::Pdf1_0 => "PDF 1.0",
        PdfDocumentVersion::Pdf1_1 => "PDF 1.1",
        PdfDocumentVersion::Pdf1_2 => "PDF 1.2",
        PdfDocumentVersion::Pdf1_3 => "PDF 1.3",
        PdfDocumentVersion::Pdf1_4 => "PDF 1.4",
        PdfDocumentVersion::Pdf1_5 => "PDF 1.5",
        PdfDocumentVersion::Pdf1_6 => "PDF 1.6",
        PdfDocumentVersion::Pdf1_7 => "PDF 1.7",
        PdfDocumentVersion::Pdf2_0 => "PDF 2.0",
        _ => "PDF",
    }
    .to_string()
}

fn non_empty(s: Option<PdfDocumentMetadataTag>) -> Option<String> {
    s.map(|t| t.value().trim().to_string()).filter(|v| !v.is_empty())
}

/// Droits effectifs d'un document chiffré (lus par lopdf : pdfium-render ne
/// connaît pas les révisions AES-256).
fn with_permissions(info: &mut DocInfo, path: &Path, password: Option<&str>) {
    if info.encrypted {
        match crate::security::inspect(path, password) {
            Ok(s) => info.permissions = s.effective(),
            Err(e) => eprintln!("permissions illisibles : {e}"),
        }
    }
}

fn build_info(id: u32, path: &Path, file_size: u64, doc: &PdfDocument) -> AppResult<DocInfo> {
    let pages: Vec<PageSize> = doc
        .pages()
        .page_sizes()
        .map_err(AppError::internal)?
        .into_iter()
        .map(|r| PageSize { w: r.width().value, h: r.height().value })
        .collect();

    let md = doc.metadata();
    use PdfDocumentMetadataTagType as T;
    let metadata = Metadata {
        title: non_empty(md.get(T::Title)),
        author: non_empty(md.get(T::Author)),
        subject: non_empty(md.get(T::Subject)),
        keywords: non_empty(md.get(T::Keywords)),
        creator: non_empty(md.get(T::Creator)),
        producer: non_empty(md.get(T::Producer)),
        created: non_empty(md.get(T::CreationDate)),
        modified: non_empty(md.get(T::ModificationDate)),
    };

    // Table des matières (signets du document), bornée pour les documents géants.
    let mut outline = Vec::new();
    fn walk(b: PdfBookmark, depth: u32, out: &mut Vec<OutlineItem>) {
        if out.len() >= 5_000 || depth > 12 {
            return;
        }
        let page = b
            .destination()
            .and_then(|d| d.page_index().ok())
            .or_else(|| {
                b.action().and_then(|a| {
                    a.as_local_destination_action()
                        .and_then(|l| l.destination().ok())
                        .and_then(|d| d.page_index().ok())
                })
            })
            .map(|p| p as u32);
        out.push(OutlineItem { title: b.title().unwrap_or_default().trim().to_string(), depth, page });
        for child in b.iter_direct_children() {
            walk(child, depth + 1, out);
        }
    }
    // (`iter_siblings` omet le signet de départ : on parcourt explicitement.)
    let mut cur = doc.bookmarks().root();
    while let Some(b) = cur {
        cur = b.next_sibling();
        walk(b, 0, &mut outline);
        if outline.len() >= 5_000 {
            break;
        }
    }

    let attachments = doc
        .attachments()
        .iter()
        .enumerate()
        .map(|(i, a)| AttachmentInfo { index: i as u32, name: a.name(), size: a.len() as u64 })
        .collect();

    // (Révision 5 ou 6, AES-256 : non reconnue par pdfium-render, donc `Err`.)
    let encrypted = !matches!(doc.permissions().security_handler_revision(), Ok(PdfSecurityHandlerRevision::Unprotected));

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Document.pdf".into());

    Ok(DocInfo {
        id,
        path: path.to_string_lossy().to_string(),
        name,
        file_size,
        page_count: pages.len() as u32,
        pages,
        version: version_label(doc.version()),
        metadata,
        outline,
        attachments,
        has_form: doc.form().is_some(),
        signature_count: doc.signatures().len() as u32,
        encrypted,
        permissions: crate::security::Perms::ALL,
        scanned: looks_scanned(doc),
        tagged: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fusion_des_boites_sur_une_ligne() {
        let r = |x: f32, y: f32| Rect { x, y, w: 5.0, h: 10.0 };
        let merged = merge_line_rects(vec![r(0.0, 0.0), r(5.0, 0.0), r(10.0, 0.5), r(0.0, 20.0)].into_iter());
        assert_eq!(merged.len(), 2);
        assert!((merged[0].w - 15.0).abs() < 0.01);
    }
}
