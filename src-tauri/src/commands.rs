//! Commandes invoquées depuis l'interface (`invoke("…")`).

use std::path::PathBuf;

use tauri::State;

use crate::create::CreateSpec;
use crate::engine::{DocInfo, PageLink, TextWord};
use crate::error::{AppError, AppResult};
use crate::library::{list_place, FileEntry, RecentView, TouchInfo, UserMark};
use crate::print::layout::PrintSettings;
use crate::print::PrintTarget;
use crate::AppState;

// ------------------------------------------------------------------ documents

#[tauri::command]
pub async fn open_document(
    state: State<'_, AppState>,
    path: String,
    password: Option<String>,
) -> AppResult<DocInfo> {
    let p = PathBuf::from(&path);
    if !p.is_file() {
        return Err(AppError::file("fichier introuvable"));
    }
    state.engine.open(p, password.filter(|s| !s.is_empty())).await
}

#[tauri::command]
pub fn close_document(state: State<'_, AppState>, doc: u32) -> AppResult<()> {
    state.engine.close(doc)
}

#[tauri::command]
pub fn set_view_gen(state: State<'_, AppState>, doc: u32, gen: u64) {
    state.engine.set_view_gen(doc, gen);
}

#[tauri::command]
pub fn search_start(state: State<'_, AppState>, doc: u32, query: String, token: u64) -> AppResult<()> {
    state.engine.search(doc, query, token)
}

#[tauri::command]
pub fn search_cancel(state: State<'_, AppState>, doc: u32) -> AppResult<()> {
    state.engine.cancel_search(doc)
}

#[tauri::command]
pub async fn page_text(state: State<'_, AppState>, doc: u32, page: u32) -> AppResult<Vec<TextWord>> {
    state.engine.page_text(doc, page).await
}

#[tauri::command]
pub async fn page_links(state: State<'_, AppState>, doc: u32, page: u32) -> AppResult<Vec<PageLink>> {
    state.engine.page_links(doc, page).await
}

#[tauri::command]
pub async fn save_attachment(state: State<'_, AppState>, doc: u32, index: u32, dest: String) -> AppResult<()> {
    state.engine.save_attachment(doc, index, PathBuf::from(dest)).await
}

#[tauri::command]
pub async fn create_pdf(state: State<'_, AppState>, spec: CreateSpec, out: String) -> AppResult<()> {
    state.engine.create(spec, PathBuf::from(out)).await
}

// ------------------------------------------------------------------ impression

/// Imposition (aperçu) : calcul pur, sans PDFium.
#[tauri::command]
pub fn print_layout(settings: PrintSettings, sizes: Vec<(f32, f32)>, margin: f32) -> crate::print::layout::Layout {
    crate::print::layout::compute(&settings, &sizes, margin)
}

#[tauri::command]
pub async fn list_printers() -> Vec<serde_json::Value> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(|| {
            crate::print::windows::list_printers()
                .into_iter()
                .map(|p| serde_json::to_value(p).unwrap_or_default())
                .collect()
        })
        .await
        .unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

#[tauri::command]
pub async fn printer_margins(name: String, paper: crate::print::layout::Paper, landscape: bool) -> AppResult<serde_json::Value> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || crate::print::windows::margins(&name, paper, landscape).map(|m| serde_json::to_value(m).unwrap_or_default()))
            .await
            .map_err(AppError::internal)?
    }
    #[cfg(not(windows))]
    {
        let _ = (name, paper, landscape);
        Ok(serde_json::json!({ "margin": 18.0, "dpi": 300 }))
    }
}

#[tauri::command]
pub async fn print_start(state: State<'_, AppState>, doc: u32, job: u64, settings: PrintSettings, margin: f32, target: PrintTarget) -> AppResult<()> {
    state.engine.print_start(doc, job, settings, margin, target).await
}

#[tauri::command]
pub fn print_cancel(state: State<'_, AppState>, job: u64) -> AppResult<()> {
    state.engine.print_cancel(job)
}

// ------------------------------------------------------------------ annotations

#[tauri::command]
pub async fn annot_load(state: State<'_, AppState>, doc: u32) -> AppResult<Vec<crate::annot::Annot>> {
    state.engine.annot_load(doc).await
}

#[tauri::command]
pub fn annot_hide(state: State<'_, AppState>, doc: u32, page: u32, indices: Vec<u32>) -> AppResult<()> {
    state.engine.annot_hide(doc, page, indices)
}

#[derive(serde::Serialize)]
pub struct AnnotSaved {
    info: DocInfo,
    annots: Vec<crate::annot::Annot>,
}

#[tauri::command]
pub async fn annot_save(state: State<'_, AppState>, doc: u32, annots: Vec<crate::annot::Annot>) -> AppResult<AnnotSaved> {
    let (info, annots) = state.engine.annot_save(doc, annots).await?;
    Ok(AnnotSaved { info, annots })
}

// ------------------------------------------------------------------ édition

#[tauri::command]
pub async fn edit_apply(state: State<'_, AppState>, doc: u32, plan: crate::edit::Plan, out: Option<String>) -> AppResult<Option<DocInfo>> {
    state.engine.edit_apply(doc, plan, out.map(PathBuf::from)).await
}

#[tauri::command]
pub async fn edit_links(state: State<'_, AppState>, doc: u32) -> AppResult<Vec<Vec<crate::edit::links::LinkInfo>>> {
    state.engine.edit_links(doc).await
}

#[tauri::command]
pub async fn redact_find(state: State<'_, AppState>, doc: u32, patterns: Vec<String>, terms: Vec<String>) -> AppResult<Vec<crate::engine::RedactHit>> {
    state.engine.redact_find(doc, patterns, terms).await
}

#[tauri::command]
pub async fn layers_list(state: State<'_, AppState>, doc: u32) -> AppResult<Vec<crate::edit::layers::Layer>> {
    state.engine.layers(doc).await
}

#[tauri::command]
pub async fn layers_set(state: State<'_, AppState>, doc: u32, hidden: Vec<String>) -> AppResult<()> {
    state.engine.layers_set(doc, hidden).await
}

#[tauri::command]
pub async fn content_list(state: State<'_, AppState>, doc: u32, page: u32) -> AppResult<Vec<crate::engine::content::ContentItem>> {
    state.engine.content_list(doc, page).await
}

#[tauri::command]
pub async fn content_apply(state: State<'_, AppState>, doc: u32, ops: Vec<crate::engine::content::ContentOp>, reset: bool) -> AppResult<()> {
    state.engine.content_apply(doc, ops, reset).await
}

// ------------------------------------------------------------------ récents

#[tauri::command]
pub fn recents_list(state: State<'_, AppState>) -> Vec<RecentView> {
    state.library.recents()
}

/// Enregistre l'ouverture et renvoie la dernière page lue.
#[tauri::command]
pub fn recents_touch(state: State<'_, AppState>, info: TouchInfo) -> AppResult<u32> {
    state.library.touch(info)
}

#[tauri::command]
pub fn recents_set_page(state: State<'_, AppState>, path: String, page: u32) -> AppResult<()> {
    state.library.update(&path, |e| e.last_page = page)
}

#[tauri::command]
pub fn recents_set_favorite(state: State<'_, AppState>, path: String, favorite: bool) -> AppResult<()> {
    state.library.update(&path, |e| e.favorite = favorite)
}

#[tauri::command]
pub fn recents_set_marks(state: State<'_, AppState>, path: String, marks: Vec<UserMark>) -> AppResult<()> {
    state.library.update(&path, |e| e.marks = marks)
}

#[tauri::command]
pub fn recents_remove(state: State<'_, AppState>, path: String) -> AppResult<()> {
    state.library.remove(&path)
}

#[tauri::command]
pub async fn places_list(place: String) -> Vec<FileEntry> {
    tauri::async_runtime::spawn_blocking(move || list_place(&place))
        .await
        .unwrap_or_default()
}

// ------------------------------------------------------------------ préférences et divers

#[tauri::command]
pub fn prefs_get(state: State<'_, AppState>) -> serde_json::Map<String, serde_json::Value> {
    state.library.prefs()
}

#[tauri::command]
pub fn prefs_set(state: State<'_, AppState>, key: String, value: serde_json::Value) -> AppResult<()> {
    state.library.set_pref(key, value)
}

#[tauri::command]
pub fn take_startup_files(state: State<'_, AppState>) -> Vec<String> {
    std::mem::take(&mut *state.startup_files.lock())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInfo {
    pub display_name: String,
    pub documents_dir: Option<String>,
}

#[tauri::command]
pub fn user_info() -> UserInfo {
    UserInfo {
        display_name: crate::platform::display_name(),
        documents_dir: dirs::document_dir().map(|p| p.to_string_lossy().to_string()),
    }
}

#[tauri::command]
pub async fn copy_file(src: String, dest: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        if crate::library::same_path(&src, &dest) {
            return Ok(());
        }
        std::fs::copy(&src, &dest)
            .map(|_| ())
            .map_err(|e| AppError::new("save", format!("Impossible d’enregistrer la copie ({e}).")))
    })
    .await
    .map_err(AppError::internal)?
}

#[tauri::command]
pub fn file_exists(path: String) -> bool {
    std::path::Path::new(&path).is_file()
}
