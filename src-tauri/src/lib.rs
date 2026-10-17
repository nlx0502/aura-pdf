//! Aura PDF — point d'entrée de l'application Tauri.
//!
//! - `engine` : moteur PDFium (thread dédié, rendu, recherche, texte) ;
//! - `library` : récents, préférences, emplacements ;
//! - `create` : création de nouveaux PDF ;
//! - `protocol` : protocole interne `aura://` qui sert les bitmaps de pages ;
//! - `platform` : code spécifique au système (à compléter pour macOS).

mod annot;
mod commands;
mod convert;
mod ocr;
mod create;
mod edit;
mod engine;
mod forms;
mod error;
mod library;
mod platform;
mod security;
mod sign;
mod sysfonts;
mod tools;
mod print;
mod protocol;

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use tauri::{Emitter, Manager};

use engine::Engine;
use library::Library;

pub struct AppState {
    pub engine: Engine,
    /// Émission d'événements vers l'interface (progression des outils).
    pub emit: engine::Emitter,
    pub library: Library,
    /// Fichiers passés en ligne de commande (association .pdf, « Ouvrir avec »).
    pub startup_files: Mutex<Vec<String>>,
}

/// Extrait les chemins de fichiers existants d'une ligne de commande.
fn files_from_args(args: impl IntoIterator<Item = String>) -> Vec<String> {
    args.into_iter()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .filter(|a| std::path::Path::new(a).is_file())
        .collect()
}

/// Dossier contenant pdfium.dll : ressources installées, sinon dossier du projet (développement).
fn pdfium_dir(app: &tauri::App) -> PathBuf {
    if let Ok(res) = app.path().resource_dir() {
        let p = res.join("resources").join("pdfium");
        if p.join("pdfium.dll").exists() || p.join("libpdfium.dylib").exists() {
            return p;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join("pdfium")
}

/// Menu de l'application sur Mac. Le menu par défaut associe ⌘W à la fermeture
/// de la fenêtre et ⌘Z à l'annulation native : ici ⌘W ferme l'onglet et ⌘Z
/// annule dans Aura (raccourcis gérés par l'interface). Couper, copier, coller
/// et tout sélectionner restent ceux du système (champs de saisie).
#[cfg(target_os = "macos")]
fn mac_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{AboutMetadata, Menu, PredefinedMenuItem as P, Submenu};
    let about = AboutMetadata { name: Some("Aura PDF".into()), version: Some(app.package_info().version.to_string()), copyright: Some("© Aura PDF contributors — licence MIT".into()), ..Default::default() };
    let app_menu = Submenu::with_items(
        app,
        "Aura PDF",
        true,
        &[
            &P::about(app, Some("À propos d’Aura PDF"), Some(about))?,
            &P::separator(app)?,
            &P::services(app, Some("Services"))?,
            &P::separator(app)?,
            &P::hide(app, Some("Masquer Aura PDF"))?,
            &P::hide_others(app, Some("Masquer les autres"))?,
            &P::show_all(app, Some("Tout afficher"))?,
            &P::separator(app)?,
            &P::quit(app, Some("Quitter Aura PDF"))?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Édition",
        true,
        &[&P::cut(app, Some("Couper"))?, &P::copy(app, Some("Copier"))?, &P::paste(app, Some("Coller"))?, &P::select_all(app, Some("Tout sélectionner"))?],
    )?;
    let window = Submenu::with_items(
        app,
        "Fenêtre",
        true,
        &[&P::minimize(app, Some("Placer dans le Dock"))?, &P::maximize(app, Some("Réduire/agrandir"))?, &P::separator(app)?, &P::fullscreen(app, Some("Plein écran"))?],
    )?;
    Menu::with_items(app, &[&app_menu, &edit, &window])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.menu(mac_menu);
    builder
        // Une seule instance : un double-clic sur un PDF l'ouvre dans un nouvel onglet.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let files = files_from_args(argv);
            if !files.is_empty() {
                let _ = app.emit("open-files", files);
            }
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .register_asynchronous_uri_scheme_protocol("aura", protocol::handle)
        .setup(|app| {
            let handle = app.handle().clone();
            let emitter: engine::Emitter = Arc::new(move |name: &str, payload: serde_json::Value| {
                let _ = handle.emit(name, payload);
            });
            if let Ok(res) = app.path().resource_dir() {
                let t = res.join("resources").join("tessdata");
                if t.join("fra.traineddata").exists() {
                    ocr::set_data_dir(t);
                }
            }
            let engine = Engine::start(pdfium_dir(app), emitter.clone());
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("AuraPDF"));
            let library = Library::load(data_dir);
            app.manage(AppState {
                engine,
                emit: emitter,
                library,
                startup_files: Mutex::new(files_from_args(std::env::args())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::open_document,
            commands::close_document,
            commands::set_view_gen,
            commands::search_start,
            commands::search_cancel,
            commands::page_text,
            commands::page_links,
            commands::save_attachment,
            commands::create_pdf,
            commands::recents_list,
            commands::recents_touch,
            commands::recents_set_page,
            commands::recents_set_favorite,
            commands::recents_set_marks,
            commands::recents_remove,
            commands::places_list,
            commands::prefs_get,
            commands::prefs_set,
            commands::take_startup_files,
            commands::user_info,
            commands::file_exists,
            commands::copy_file,
            commands::print_layout,
            commands::list_printers,
            commands::printer_margins,
            commands::print_start,
            commands::print_cancel,
            commands::annot_load,
            commands::annot_hide,
            commands::annot_save,
            commands::edit_apply,
            commands::edit_links,
            commands::redact_find,
            commands::layers_list,
            commands::layers_set,
            commands::content_list,
            commands::content_apply,
            commands::system_fonts,
            commands::tool_inspect,
            commands::tool_analyze,
            commands::split_by_size,
            commands::merge_start,
            commands::split_start,
            commands::compress_start,
            commands::tool_cancel,
            commands::reload_document,
            commands::sign_report,
            commands::cert_inspect,
            commands::cert_store_list,
            commands::cert_create,
            commands::sign_start,
            commands::security_info,
            commands::pdfa_check,
            commands::security_apply,
            commands::security_reload,
            commands::forms_list,
            commands::forms_save,
            commands::forms_flatten,
            commands::forms_restore,
            commands::convert_info,
            commands::convert_start,
            commands::document_pdf,
            commands::ocr_start,
            commands::create_pdf_ocr,
            commands::scan_acquire,
            commands::clipboard_pdf,
        ])
        .build(tauri::generate_context!())
        .expect("erreur au lancement d’Aura PDF")
        .run(|_app, _event| {
            // macOS : les documents ouverts depuis le Finder (double-clic, « Ouvrir
            // avec », glisser sur l'icône du Dock) arrivent par cet événement et
            // non par la ligne de commande.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = &_event {
                let files: Vec<String> = urls.iter().filter_map(|u| u.to_file_path().ok()).filter(|p| p.is_file()).map(|p| p.to_string_lossy().to_string()).collect();
                if !files.is_empty() {
                    // Au lancement, l'interface n'écoute pas encore : elle les
                    // récupérera avec les fichiers de démarrage.
                    if let Some(state) = _app.try_state::<AppState>() {
                        state.startup_files.lock().extend(files.iter().cloned());
                    }
                    let _ = _app.emit("open-files", files);
                }
            }
        });
}
