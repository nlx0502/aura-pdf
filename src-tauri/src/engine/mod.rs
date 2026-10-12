//! Moteur PDF : un thread dédié possède PDFium et tous les documents ouverts.
//!
//! PDFium n'est pas réentrant ; plutôt que de verrouiller chaque appel, toutes
//! les opérations passent par un canal vers ce thread unique. Les demandes de
//! rendu sont placées dans une file de priorité (page visible d'abord, puis
//! pages voisines, puis miniatures) et les demandes devenues obsolètes après un
//! défilement ou un zoom sont abandonnées sans être calculées.

pub mod content;
pub mod textsearch;
pub mod types;
mod worker;
pub(crate) use worker::load_error as load_error_pub;
pub use worker::RedactHit;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_edit;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crossbeam_channel::{unbounded, Sender};
use parking_lot::RwLock;
use tokio::sync::oneshot;

use crate::error::{AppError, AppResult};
pub use types::*;

/// Fonction d'émission d'événements vers l'interface (nom, charge utile JSON).
pub type Emitter = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;
/// Réponse différée d'une demande de rendu.
pub type RenderResponder = Box<dyn FnOnce(RenderOutcome) + Send>;

/// Priorités de rendu (plus petit = plus urgent).
pub mod prio {
    pub const VISIBLE: u8 = 0;
    pub const NEAR: u8 = 1;
    pub const THUMB: u8 = 2;
}

pub struct RenderJob {
    pub doc: u32,
    pub page: u32,
    /// Largeur cible du bitmap en pixels (zoom et densité d'écran inclus).
    pub width_px: u32,
    /// Rotation d'affichage supplémentaire, en quarts de tour horaires.
    pub rotation: u8,
    pub annotations: bool,
    pub forms: bool,
    /// Tuile à rendre (x, y, largeur, hauteur en pixels du rendu complet), ou page entière.
    pub tile: Option<[u32; 4]>,
    pub prio: u8,
    /// Génération de vue au moment de la demande (voir [`Engine::set_view_gen`]).
    pub gen: u64,
    pub responder: RenderResponder,
}

pub(crate) enum Msg {
    Open {
        path: PathBuf,
        password: Option<String>,
        reply: oneshot::Sender<AppResult<DocInfo>>,
    },
    Close {
        doc: u32,
    },
    Render(RenderJob),
    Search {
        doc: u32,
        query: String,
        token: u64,
    },
    CancelSearch {
        doc: u32,
    },
    PageText {
        doc: u32,
        page: u32,
        reply: oneshot::Sender<AppResult<Vec<TextWord>>>,
    },
    PageLinks {
        doc: u32,
        page: u32,
        reply: oneshot::Sender<AppResult<Vec<PageLink>>>,
    },
    SaveAttachment {
        doc: u32,
        index: u32,
        dest: PathBuf,
        reply: oneshot::Sender<AppResult<()>>,
    },
    Thumbnail {
        path: PathBuf,
        out: PathBuf,
        width: u32,
        reply: oneshot::Sender<AppResult<FileSummary>>,
    },
    Create {
        spec: crate::create::CreateSpec,
        out: PathBuf,
        reply: oneshot::Sender<AppResult<()>>,
    },
    PrintStart {
        doc: u32,
        job: u64,
        settings: crate::print::layout::PrintSettings,
        margin: f32,
        target: crate::print::PrintTarget,
        reply: oneshot::Sender<AppResult<()>>,
    },
    PrintCancel {
        job: u64,
    },
    AnnotLoad {
        doc: u32,
        reply: oneshot::Sender<AppResult<Vec<crate::annot::Annot>>>,
    },
    AnnotHide {
        doc: u32,
        page: u32,
        indices: Vec<u32>,
    },
    AnnotSave {
        doc: u32,
        annots: Vec<crate::annot::Annot>,
        reply: oneshot::Sender<AppResult<(DocInfo, Vec<crate::annot::Annot>)>>,
    },
    /// Relit le fichier (modifié par un outil) sous le même identifiant.
    Reload {
        doc: u32,
        /// Nouveau mot de passe (après une modification de la sécurité).
        password: Option<Option<String>>,
        reply: oneshot::Sender<AppResult<DocInfo>>,
    },
    /// Contenu d'une page pour la conversion (texte mis en forme, image).
    Extract {
        doc: u32,
        page: u32,
        dpi: Option<f32>,
        text: bool,
        reply: oneshot::Sender<AppResult<crate::convert::model::PageData>>,
    },
    /// Chemin et mot de passe d'un document ouvert (le mot de passe ne quitte
    /// jamais le processus Rust).
    Credentials {
        doc: u32,
        reply: oneshot::Sender<AppResult<(PathBuf, Option<String>)>>,
    },
    EditApply {
        doc: u32,
        plan: crate::edit::Plan,
        out: Option<PathBuf>,
        reply: oneshot::Sender<AppResult<Option<DocInfo>>>,
    },
    EditLinks {
        doc: u32,
        reply: oneshot::Sender<AppResult<Vec<Vec<crate::edit::links::LinkInfo>>>>,
    },
    RedactFind {
        doc: u32,
        patterns: Vec<String>,
        terms: Vec<String>,
        reply: oneshot::Sender<AppResult<Vec<RedactHit>>>,
    },
    Layers {
        doc: u32,
        reply: oneshot::Sender<AppResult<Vec<crate::edit::layers::Layer>>>,
    },
    LayersSet {
        doc: u32,
        hidden: Vec<String>,
        reply: oneshot::Sender<AppResult<()>>,
    },
    ContentList {
        doc: u32,
        page: u32,
        reply: oneshot::Sender<AppResult<Vec<content::ContentItem>>>,
    },
    ContentApply {
        doc: u32,
        ops: Vec<content::ContentOp>,
        reset: bool,
        reply: oneshot::Sender<AppResult<()>>,
    },
}

/// Résumé léger d'un fichier (pour la liste des récents).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSummary {
    pub page_count: u32,
    pub signed: bool,
    pub form: bool,
    pub scanned: bool,
}

#[derive(Clone)]
pub struct Engine {
    tx: Sender<Msg>,
    gens: Arc<RwLock<HashMap<u32, Arc<AtomicU64>>>>,
}

impl Engine {
    /// Démarre le thread du moteur. `lib_dir` contient `pdfium.dll`.
    pub fn start(lib_dir: PathBuf, emitter: Emitter) -> Self {
        let (tx, rx) = unbounded();
        let gens: Arc<RwLock<HashMap<u32, Arc<AtomicU64>>>> = Default::default();
        let worker_gens = gens.clone();
        std::thread::Builder::new()
            .name("aura-pdf-engine".into())
            .spawn(move || worker::run(lib_dir, rx, worker_gens, emitter))
            .expect("impossible de démarrer le moteur PDF");
        Self { tx, gens }
    }

    fn send(&self, msg: Msg) -> AppResult<()> {
        self.tx
            .send(msg)
            .map_err(|_| AppError::internal("moteur PDF arrêté"))
    }

    async fn ask<T>(&self, make: impl FnOnce(oneshot::Sender<AppResult<T>>) -> Msg) -> AppResult<T> {
        let (tx, rx) = oneshot::channel();
        self.send(make(tx))?;
        rx.await
            .map_err(|_| AppError::internal("le moteur PDF n’a pas répondu"))?
    }

    pub async fn open(&self, path: PathBuf, password: Option<String>) -> AppResult<DocInfo> {
        self.ask(|reply| Msg::Open { path, password, reply }).await
    }

    pub fn close(&self, doc: u32) -> AppResult<()> {
        self.gens.write().remove(&doc);
        self.send(Msg::Close { doc })
    }

    /// Signale un changement de vue (défilement, zoom) : les demandes de rendu
    /// visibles plus anciennes seront abandonnées.
    pub fn set_view_gen(&self, doc: u32, gen: u64) {
        if let Some(g) = self.gens.read().get(&doc) {
            g.fetch_max(gen, Ordering::Relaxed);
        }
    }

    pub fn render(&self, job: RenderJob) {
        if let Err(Msg::Render(job)) = self.tx.send(Msg::Render(job)).map_err(|e| e.0) {
            (job.responder)(RenderOutcome::Err("moteur arrêté".into()));
        }
    }

    pub fn search(&self, doc: u32, query: String, token: u64) -> AppResult<()> {
        self.send(Msg::Search { doc, query, token })
    }

    pub fn cancel_search(&self, doc: u32) -> AppResult<()> {
        self.send(Msg::CancelSearch { doc })
    }

    pub async fn page_text(&self, doc: u32, page: u32) -> AppResult<Vec<TextWord>> {
        self.ask(|reply| Msg::PageText { doc, page, reply }).await
    }

    pub async fn page_links(&self, doc: u32, page: u32) -> AppResult<Vec<PageLink>> {
        self.ask(|reply| Msg::PageLinks { doc, page, reply }).await
    }

    pub async fn save_attachment(&self, doc: u32, index: u32, dest: PathBuf) -> AppResult<()> {
        self.ask(|reply| Msg::SaveAttachment { doc, index, dest, reply }).await
    }

    pub async fn thumbnail(&self, path: PathBuf, out: PathBuf, width: u32) -> AppResult<FileSummary> {
        self.ask(|reply| Msg::Thumbnail { path, out, width, reply }).await
    }

    pub async fn print_start(&self, doc: u32, job: u64, settings: crate::print::layout::PrintSettings, margin: f32, target: crate::print::PrintTarget) -> AppResult<()> {
        self.ask(|reply| Msg::PrintStart { doc, job, settings, margin, target, reply }).await
    }

    pub fn print_cancel(&self, job: u64) -> AppResult<()> {
        self.send(Msg::PrintCancel { job })
    }

    pub async fn annot_load(&self, doc: u32) -> AppResult<Vec<crate::annot::Annot>> {
        self.ask(|reply| Msg::AnnotLoad { doc, reply }).await
    }

    /// Masque dans le rendu PDFium les annotations d'origine en cours d'édition
    /// (elles sont alors dessinées par l'interface).
    pub fn annot_hide(&self, doc: u32, page: u32, indices: Vec<u32>) -> AppResult<()> {
        self.send(Msg::AnnotHide { doc, page, indices })
    }

    pub async fn annot_save(&self, doc: u32, annots: Vec<crate::annot::Annot>) -> AppResult<(DocInfo, Vec<crate::annot::Annot>)> {
        self.ask(|reply| Msg::AnnotSave { doc, annots, reply }).await
    }

    pub async fn reload(&self, doc: u32) -> AppResult<DocInfo> {
        self.ask(|reply| Msg::Reload { doc, password: None, reply }).await
    }

    pub async fn reload_with(&self, doc: u32, password: Option<String>) -> AppResult<DocInfo> {
        self.ask(|reply| Msg::Reload { doc, password: Some(password.filter(|p| !p.is_empty())), reply }).await
    }

    pub async fn extract(&self, doc: u32, page: u32, dpi: Option<f32>, text: bool) -> AppResult<crate::convert::model::PageData> {
        self.ask(|reply| Msg::Extract { doc, page, dpi, text, reply }).await
    }

    pub async fn credentials(&self, doc: u32) -> AppResult<(PathBuf, Option<String>)> {
        self.ask(|reply| Msg::Credentials { doc, reply }).await
    }

    pub async fn edit_apply(&self, doc: u32, plan: crate::edit::Plan, out: Option<PathBuf>) -> AppResult<Option<DocInfo>> {
        self.ask(|reply| Msg::EditApply { doc, plan, out, reply }).await
    }

    pub async fn edit_links(&self, doc: u32) -> AppResult<Vec<Vec<crate::edit::links::LinkInfo>>> {
        self.ask(|reply| Msg::EditLinks { doc, reply }).await
    }

    pub async fn redact_find(&self, doc: u32, patterns: Vec<String>, terms: Vec<String>) -> AppResult<Vec<RedactHit>> {
        self.ask(|reply| Msg::RedactFind { doc, patterns, terms, reply }).await
    }

    pub async fn layers(&self, doc: u32) -> AppResult<Vec<crate::edit::layers::Layer>> {
        self.ask(|reply| Msg::Layers { doc, reply }).await
    }

    pub async fn layers_set(&self, doc: u32, hidden: Vec<String>) -> AppResult<()> {
        self.ask(|reply| Msg::LayersSet { doc, hidden, reply }).await
    }

    pub async fn content_list(&self, doc: u32, page: u32) -> AppResult<Vec<content::ContentItem>> {
        self.ask(|reply| Msg::ContentList { doc, page, reply }).await
    }

    pub async fn content_apply(&self, doc: u32, ops: Vec<content::ContentOp>, reset: bool) -> AppResult<()> {
        self.ask(|reply| Msg::ContentApply { doc, ops, reset, reply }).await
    }

    pub async fn create(&self, spec: crate::create::CreateSpec, out: PathBuf) -> AppResult<()> {
        self.ask(|reply| Msg::Create { spec, out, reply }).await
    }
}
