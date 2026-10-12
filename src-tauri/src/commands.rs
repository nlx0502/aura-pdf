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
    let (path, password) = state.engine.credentials(doc).await?;
    if !blocking(move || crate::security::rights(&path, password.as_deref())).await?.print {
        return Err(AppError::new("print", "Le propriétaire de ce document n’autorise pas son impression."));
    }
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

/// Polices installées utilisables dans les zones de texte et le contenu.
#[tauri::command]
pub async fn system_fonts() -> AppResult<Vec<crate::sysfonts::FamilyInfo>> {
    tauri::async_runtime::spawn_blocking(crate::sysfonts::list).await.map_err(AppError::internal)
}

// ------------------------------------------------------------------ outils

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(AppError::internal)?.map_err(|e| AppError::new("tool", e))
}

#[tauri::command]
pub async fn tool_inspect(path: String) -> AppResult<crate::tools::inspect::FileInfo> {
    blocking(move || crate::tools::inspect::inspect(std::path::Path::new(&path))).await
}

#[tauri::command]
pub async fn tool_analyze(path: String) -> AppResult<crate::tools::compress::Analysis> {
    blocking(move || crate::tools::compress::analyze(std::path::Path::new(&path))).await
}

#[tauri::command]
pub async fn split_by_size(path: String, max_bytes: u64) -> AppResult<Vec<[u32; 2]>> {
    blocking(move || crate::tools::split::by_size(std::path::Path::new(&path), max_bytes)).await
}

const IMAGE_EXT: [&str; 8] = ["png", "jpg", "jpeg", "gif", "bmp", "tif", "tiff", "webp"];

/// Fusion : les images sont d'abord converties en PDF (une page A4 par image).
#[tauri::command]
pub async fn merge_start(state: State<'_, AppState>, job: u64, inputs: Vec<String>, opts: crate::tools::merge::MergeOptions, out_dir: String, out_name: String) -> AppResult<String> {
    let mut paths = Vec::new();
    for (i, p) in inputs.iter().enumerate() {
        let path = PathBuf::from(p);
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if IMAGE_EXT.contains(&ext.as_str()) {
            let tmp = std::env::temp_dir().join(format!("aura-merge-{job}-{i}.pdf"));
            let spec = CreateSpec::Images { paths: vec![p.clone()], format: crate::create::PaperFormat::A4, landscape: false, one_per_page: true };
            state.engine.create(spec, tmp.clone()).await?;
            paths.push(tmp);
        } else if ext != "pdf" {
            return Err(AppError::new("tool", format!("« {} » : seuls les PDF et les images peuvent être fusionnés pour l’instant (Word au jalon 8).", path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default())));
        } else {
            paths.push(path);
        }
    }
    let stem = out_name.trim().trim_end_matches(".pdf").trim_end_matches(".PDF").to_string();
    let out = crate::tools::free_path(std::path::Path::new(&out_dir), &stem);
    let out_s = out.to_string_lossy().to_string();
    crate::tools::spawn(job, state.emit.clone(), move |j| {
        let r = crate::tools::merge::merge(&paths, &opts, &out, j).map(|o| vec![o]);
        for p in &paths {
            if p.file_name().map(|n| n.to_string_lossy().starts_with("aura-merge-")).unwrap_or(false) {
                let _ = std::fs::remove_file(p);
            }
        }
        r
    });
    Ok(out_s)
}

#[tauri::command]
pub fn split_start(state: State<'_, AppState>, job: u64, path: String, parts: Vec<crate::tools::split::Part>, out_dir: String) {
    crate::tools::spawn(job, state.emit.clone(), move |j| crate::tools::split::split(std::path::Path::new(&path), &parts, std::path::Path::new(&out_dir), j));
}

/// Compression vers « nom (compressé).pdf » à côté de l'original.
#[tauri::command]
pub fn compress_start(state: State<'_, AppState>, job: u64, path: String, opts: crate::tools::compress::CompressOptions) -> String {
    let src = PathBuf::from(&path);
    let dir = src.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let out = crate::tools::free_path(&dir, &format!("{stem} (compressé)"));
    let out_s = out.to_string_lossy().to_string();
    crate::tools::spawn(job, state.emit.clone(), move |j| crate::tools::compress::compress(&src, &opts, &out, j).map(|o| vec![o]));
    out_s
}

#[tauri::command]
pub fn tool_cancel(job: u64) {
    crate::tools::cancel(job);
}

// ------------------------------------------------------------------ signature

#[tauri::command]
pub async fn reload_document(state: State<'_, AppState>, doc: u32) -> AppResult<DocInfo> {
    state.engine.reload(doc).await
}

#[tauri::command]
pub async fn sign_report(path: String) -> AppResult<crate::sign::verify::Report> {
    blocking(move || crate::sign::verify::report(std::path::Path::new(&path))).await
}

#[tauri::command]
pub async fn cert_inspect(path: String, password: String) -> AppResult<crate::sign::crypto::CertInfo> {
    blocking(move || crate::sign::crypto::load_p12(std::path::Path::new(&path), &password).map(|s| crate::sign::crypto::cert_info(&s.chain[0]))).await
}

#[tauri::command]
pub async fn cert_create(name: String, email: String, organization: String, password: String, out: String) -> AppResult<crate::sign::crypto::CertInfo> {
    blocking(move || crate::sign::crypto::create_self_signed(&name, &email, &organization, &password, std::path::Path::new(&out))).await
}

/// Signe le fichier `path` (tâche annulable, progression par étapes).
#[tauri::command]
pub fn sign_start(state: State<'_, AppState>, job: u64, path: String, req: crate::sign::SignRequest) {
    crate::tools::spawn(job, state.emit.clone(), move |j| crate::sign::sign(std::path::Path::new(&path), &req, j).map(|o| vec![o]));
}

// ------------------------------------------------------------------ sécurité

#[tauri::command]
pub async fn security_info(state: State<'_, AppState>, doc: u32) -> AppResult<crate::security::SecurityInfo> {
    let (path, password) = state.engine.credentials(doc).await?;
    blocking(move || crate::security::inspect(&path, password.as_deref())).await
}

/// Contrôle de conformité PDF/A du document tel qu'il est enregistré.
#[tauri::command]
pub async fn pdfa_check(state: State<'_, AppState>, doc: u32, level: String) -> AppResult<Vec<crate::security::pdfa::Check>> {
    let (path, password) = state.engine.credentials(doc).await?;
    blocking(move || {
        let info = crate::security::inspect(&path, password.as_deref())?;
        let d = crate::security::load(&path, password.as_deref())?;
        crate::security::pdfa::check(&d, &level, info.algorithm != "none")
    })
    .await
}

/// Applique la sécurité au fichier du document (tâche avec progression) ;
/// l'interface rouvre ensuite le document avec `security_reload`.
#[tauri::command]
pub async fn security_apply(state: State<'_, AppState>, job: u64, doc: u32, mut spec: crate::security::SecuritySpec) -> AppResult<()> {
    let (path, password) = state.engine.credentials(doc).await?;
    // Mot de passe d'ouverture laissé vide : celui du document est conservé
    // (s'il a été ouvert avec le mot de passe utilisateur, pas propriétaire).
    if spec.protect && spec.user_password.is_empty() {
        if let Some(pw) = &password {
            let (p, pw2) = (path.clone(), pw.clone());
            let info = blocking(move || crate::security::inspect(&p, Some(&pw2))).await?;
            if info.open_password && !info.owner {
                spec.user_password = pw.clone();
            }
        }
    }
    // Retirer ou modifier la sécurité exige les droits du propriétaire.
    let (p, pw) = (path.clone(), password.clone());
    let info = blocking(move || crate::security::inspect(&p, pw.as_deref())).await?;
    if info.algorithm != "none" && !info.owner && info.permissions != crate::security::Perms::ALL {
        return Err(AppError::new("security", "Saisissez le mot de passe propriétaire pour modifier la sécurité de ce document."));
    }
    crate::tools::spawn(job, state.emit.clone(), move |j| crate::security::apply(&path, password.as_deref(), &spec, j).map(|o| vec![o]));
    Ok(())
}

/// Rouvre le document après `security_apply` : avec le mot de passe
/// propriétaire s'il a été choisi (l'auteur garde tous les droits), sinon
/// avec le mot de passe d'ouverture.
#[tauri::command]
pub async fn security_reload(state: State<'_, AppState>, doc: u32, password: Option<String>) -> AppResult<DocInfo> {
    state.engine.reload_with(doc, password).await
}

// ------------------------------------------------------------------ formulaires

#[tauri::command]
pub async fn forms_list(state: State<'_, AppState>, doc: u32) -> AppResult<Vec<crate::forms::Field>> {
    let (path, password) = state.engine.credentials(doc).await?;
    blocking(move || crate::forms::read(&path, password.as_deref())).await
}

/// Enregistre valeurs, propriétés, nouveaux champs et suppressions, puis relit le document.
#[tauri::command]
pub async fn forms_save(state: State<'_, AppState>, doc: u32, plan: crate::forms::FormPlan) -> AppResult<DocInfo> {
    let (path, password) = state.engine.credentials(doc).await?;
    blocking(move || {
        let perms = crate::security::rights(&path, password.as_deref())?;
        // Remplir exige le droit « formulaires » ; modifier la structure (champs, style, position), le droit « modifier ».
        let structure = !plan.delete.is_empty() || {
            let current = crate::forms::read(&path, password.as_deref())?;
            plan.upsert.iter().any(|f| {
                let Some(o) = current.iter().find(|o| o.name == f.name).filter(|_| !f.is_new) else { return true };
                let mut a = serde_json::to_value(o).unwrap_or_default();
                let mut b = serde_json::to_value(f).unwrap_or_default();
                for v in [&mut a, &mut b] {
                    if let Some(m) = v.as_object_mut() {
                        m.remove("value");
                    }
                }
                a != b
            })
        };
        if !perms.forms || (structure && !perms.edit) {
            return Err("Le propriétaire du document n’autorise pas cette modification du formulaire.".into());
        }
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let out = crate::forms::save(bytes, password.as_deref(), &plan)?;
        crate::edit::write_atomic(&path, &out).map_err(|e| e.message)
    })
    .await?;
    state.engine.reload(doc).await
}

/// Aplatit le formulaire (les valeurs deviennent du contenu fixe), puis relit
/// le document. Une copie de l'original est gardée pour « Annuler ».
#[tauri::command]
pub async fn forms_flatten(state: State<'_, AppState>, doc: u32) -> AppResult<(DocInfo, String)> {
    let (path, password) = state.engine.credentials(doc).await?;
    let backup = blocking(move || {
        if !crate::security::rights(&path, password.as_deref())?.edit {
            return Err("Le propriétaire du document n’autorise pas sa modification.".into());
        }
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let backup = std::env::temp_dir().join(format!("aura-formulaire-{doc}-{nanos}.pdf"));
        std::fs::write(&backup, &bytes).map_err(|e| e.to_string())?;
        let out = crate::forms::flatten(&bytes, password.as_deref())?;
        crate::edit::write_atomic(&path, &out).map_err(|e| e.message)?;
        Ok(backup.to_string_lossy().to_string())
    })
    .await?;
    Ok((state.engine.reload(doc).await?, backup))
}

/// Annule un aplatissement : l'original est remis en place.
#[tauri::command]
pub async fn forms_restore(state: State<'_, AppState>, doc: u32, backup: String) -> AppResult<DocInfo> {
    let (path, _) = state.engine.credentials(doc).await?;
    let backup = std::path::PathBuf::from(backup);
    if !backup.file_name().map(|n| n.to_string_lossy().starts_with("aura-formulaire-")).unwrap_or(false) || backup.parent() != Some(std::env::temp_dir().as_path()) {
        return Err(AppError::new("forms", "Copie de sauvegarde introuvable."));
    }
    blocking(move || {
        let bytes = std::fs::read(&backup).map_err(|e| e.to_string())?;
        crate::edit::write_atomic(&path, &bytes).map_err(|e| e.message)?;
        let _ = std::fs::remove_file(&backup);
        Ok(())
    })
    .await?;
    state.engine.reload(doc).await
}

// ------------------------------------------------------------------ conversion

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertInfo {
    pub word: bool,
    pub excel: bool,
    pub powerpoint: bool,
}

/// Applications Office disponibles pour la conversion.
#[tauri::command]
pub async fn convert_info() -> ConvertInfo {
    tauri::async_runtime::spawn_blocking(|| ConvertInfo {
        word: crate::convert::office_available("Word"),
        excel: crate::convert::office_available("Excel"),
        powerpoint: crate::convert::office_available("PowerPoint"),
    })
    .await
    .unwrap_or(ConvertInfo { word: false, excel: false, powerpoint: false })
}

/// Convertit des fichiers (tâche annulable, progression par fichier et par page).
#[tauri::command]
pub fn convert_start(state: State<'_, AppState>, job: u64, files: Vec<String>, spec: crate::convert::ConvertSpec) {
    let engine = state.engine.clone();
    crate::tools::spawn(job, state.emit.clone(), move |j| crate::convert::run(&engine, &files, &spec, ocr_fn(), j));
}

/// Reconnaissance de texte pour la conversion (pages numérisées).
fn ocr_fn() -> Option<crate::convert::OcrFn<'static>> {
    Some(&crate::ocr::convert_ocr)
}

/// Reconnaissance de texte d'un document ouvert (tâche avec aperçu en direct).
#[tauri::command]
pub fn ocr_start(state: State<'_, AppState>, job: u64, doc: u32, spec: crate::ocr::job::OcrSpec) {
    let engine = state.engine.clone();
    crate::tools::spawn(job, state.emit.clone(), move |j| crate::ocr::job::run(&engine, doc, &spec, j).map(|o| vec![o]));
}

/// Crée un PDF (images, numérisations) puis le rend recherchable, sur place.
#[tauri::command]
pub fn create_pdf_ocr(state: State<'_, AppState>, job: u64, spec: CreateSpec, out: String, langs: Vec<String>) {
    let engine = state.engine.clone();
    crate::tools::spawn(job, state.emit.clone(), move |j| {
        use tauri::async_runtime::block_on as block;
        let path = std::path::PathBuf::from(&out);
        block(engine.create(spec, path.clone())).map_err(|e| e.message)?;
        let info = block(engine.open(path, None)).map_err(|e| e.message)?;
        let spec = crate::ocr::job::OcrSpec { langs, searchable: true, deskew: true, all_pages: false, in_place: true };
        let r = crate::ocr::job::run(&engine, info.id, &spec, j);
        let _ = engine.close(info.id);
        r.map(|o| vec![o])
    });
}

/// Document Office (Word, Excel, PowerPoint) en PDF, vers `out`.
#[tauri::command]
pub fn document_pdf(state: State<'_, AppState>, job: u64, input: String, out: String) {
    crate::tools::spawn(job, state.emit.clone(), move |j| crate::convert::document_to_pdf(std::path::Path::new(&input), std::path::Path::new(&out), j).map(|o| vec![o]));
}

/// Numérise une page avec le scanner (boîte de dialogue Windows) ; renvoie l'image, ou rien si annulé.
#[tauri::command]
pub async fn scan_acquire() -> AppResult<Option<String>> {
    blocking(crate::convert::scan::acquire).await
}

/// Crée un PDF à partir du contenu du presse-papiers (image ou texte).
#[tauri::command]
pub async fn clipboard_pdf(state: State<'_, AppState>, out: String) -> AppResult<()> {
    let image = blocking(crate::convert::scan::clipboard).await?;
    match image {
        crate::convert::scan::Clip::Image(path) => {
            let spec = CreateSpec::Scans { paths: vec![path.clone()] };
            let r = state.engine.create(spec, std::path::PathBuf::from(&out)).await;
            let _ = std::fs::remove_file(&path);
            r
        }
        crate::convert::scan::Clip::Text(text) => blocking(move || crate::convert::text_to_pdf(&text, std::path::Path::new(&out))).await,
    }
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
