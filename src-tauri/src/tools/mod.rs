//! Outils (menu Outils) : fusion, division, compression.
//!
//! Chaque traitement tourne dans son propre thread (lopdf, sans PDFium) ; il
//! émet sa progression (`tool:progress`) puis son résultat (`tool:done`) et
//! peut être annulé entre deux étapes.

pub mod compress;
pub mod inspect;
pub mod merge;
pub mod split;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use serde::Serialize;
use serde_json::json;

use crate::engine::Emitter;

/// Fichier produit par un outil.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    pub path: String,
    pub size: u64,
    pub pages: u32,
}

/// Contexte d'une tâche : progression et annulation.
pub struct Job {
    pub id: u64,
    cancel: Arc<AtomicBool>,
    emit: Option<Emitter>,
}

impl Job {
    /// Tâche sans interface (tests).
    #[cfg(test)]
    pub fn silent() -> Self {
        Job { id: 0, cancel: Arc::new(AtomicBool::new(false)), emit: None }
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Erreur à renvoyer si l'utilisateur a annulé.
    pub fn check(&self) -> Result<(), String> {
        if self.cancelled() {
            Err(CANCELLED.into())
        } else {
            Ok(())
        }
    }

    /// Étape `step`, avancement `done` / `total`, détail libre.
    pub fn progress(&self, step: usize, done: usize, total: usize, detail: &str) {
        if let Some(e) = &self.emit {
            e("tool:progress", json!({ "job": self.id, "step": step, "done": done, "total": total, "detail": detail }));
        }
    }
}

pub const CANCELLED: &str = "annulé";

fn registry() -> &'static Mutex<HashMap<u64, Arc<AtomicBool>>> {
    static R: OnceLock<Mutex<HashMap<u64, Arc<AtomicBool>>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

/// Lance `work` dans un thread ; le résultat est émis dans `tool:done`.
pub fn spawn<F>(id: u64, emit: Emitter, work: F)
where
    F: FnOnce(&Job) -> Result<Vec<Output>, String> + Send + 'static,
{
    let cancel = Arc::new(AtomicBool::new(false));
    registry().lock().insert(id, cancel.clone());
    let job = Job { id, cancel, emit: Some(emit.clone()) };
    std::thread::Builder::new()
        .name(format!("aura-tool-{id}"))
        .spawn(move || {
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&job))).unwrap_or_else(|_| Err("erreur interne".into()));
            registry().lock().remove(&id);
            let payload = match res {
                Ok(outputs) => json!({ "job": id, "ok": true, "outputs": outputs }),
                Err(e) if e == CANCELLED => json!({ "job": id, "ok": false, "cancelled": true }),
                Err(e) => json!({ "job": id, "ok": false, "message": e }),
            };
            emit("tool:done", payload);
        })
        .expect("thread d'outil");
}

pub fn cancel(id: u64) {
    if let Some(c) = registry().lock().get(&id) {
        c.store(true, Ordering::Relaxed);
    }
}

/// Chemin libre : « nom.pdf », sinon « nom (2).pdf », « nom (3).pdf »…
pub fn free_path(dir: &Path, stem: &str) -> PathBuf {
    let clean: String = stem.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) { '-' } else { c }).collect();
    let clean = clean.trim().trim_end_matches('.').to_string();
    let clean = if clean.is_empty() { "Document".to_string() } else { clean };
    let first = dir.join(format!("{clean}.pdf"));
    if !first.exists() {
        return first;
    }
    (2..).map(|n| dir.join(format!("{clean} ({n}).pdf"))).find(|p| !p.exists()).unwrap()
}

/// Écrit un document (fichier temporaire puis remplacement) et décrit le résultat.
pub fn write(doc: &mut lopdf::Document, out: &Path, modern: bool) -> Result<Output, String> {
    let mut bytes = Vec::new();
    if modern {
        doc.save_modern(&mut bytes).map_err(|e| format!("écriture impossible ({e})"))?;
    } else {
        doc.save_to(&mut bytes).map_err(|e| format!("écriture impossible ({e})"))?;
    }
    crate::edit::write_atomic(out, &bytes).map_err(|e| e.message)?;
    Ok(Output { path: out.to_string_lossy().into(), size: bytes.len() as u64, pages: doc.get_pages().len() as u32 })
}

/// Charge un PDF pour un outil (les fichiers protégés sont refusés avec un message clair).
pub fn load(path: &Path) -> Result<lopdf::Document, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let doc = lopdf::Document::load(path).map_err(|e| format!("« {name} » est illisible ({e})."))?;
    if doc.is_encrypted() || doc.trailer.has(b"Encrypt") {
        return Err(format!("« {name} » est protégé par mot de passe : retirez d’abord la protection."));
    }
    Ok(doc)
}
