//! Données locales de l'utilisateur : documents récents, préférences et
//! emplacements (Documents, Bureau, Téléchargements).
//!
//! Tout est stocké en JSON dans le dossier de données de l'application
//! (`%APPDATA%\fr.aurapdf.app`). Aucune donnée ne quitte la machine.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UserMark {
    pub title: String,
    pub page: u32,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentEntry {
    pub path: String,
    pub name: String,
    pub page_count: u32,
    pub file_size: u64,
    /// Dernière page lue (index à partir de 0).
    pub last_page: u32,
    pub opened_at: DateTime<Utc>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub signed: bool,
    #[serde(default)]
    pub form: bool,
    #[serde(default)]
    pub scanned: bool,
    #[serde(default)]
    pub annotated: bool,
    #[serde(default)]
    pub marks: Vec<UserMark>,
}

/// Entrée renvoyée à l'interface (champs calculés en plus).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentView {
    #[serde(flatten)]
    pub entry: RecentEntry,
    pub folder: String,
    pub place: Option<&'static str>,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TouchInfo {
    pub path: String,
    pub page_count: u32,
    pub file_size: u64,
    pub signed: bool,
    pub form: bool,
    pub scanned: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub folder: String,
    pub file_size: u64,
    pub modified: Option<DateTime<Utc>>,
}

const MAX_RECENTS: usize = 200;

pub struct Library {
    dir: PathBuf,
    recents: Mutex<Vec<RecentEntry>>,
    prefs: Mutex<serde_json::Map<String, serde_json::Value>>,
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Écriture atomique : fichier temporaire puis renommage.
fn write_json<T: Serialize>(path: &Path, value: &T) -> AppResult<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value).unwrap_or_default())?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Emplacements connus : (identifiant, libellé, dossier).
pub fn places() -> Vec<(&'static str, &'static str, Option<PathBuf>)> {
    vec![
        ("docs", "Documents", dirs::document_dir()),
        ("desk", "Bureau", dirs::desktop_dir()),
        ("dl", "Téléchargements", dirs::download_dir()),
    ]
}

/// Libellé de dossier lisible (« Documents › Rapports ») et emplacement associé.
pub fn describe_folder(path: &Path) -> (String, Option<&'static str>) {
    let parent = path.parent().unwrap_or(Path::new(""));
    for (id, label, dir) in places() {
        if let Some(dir) = dir {
            if let Ok(rel) = parent.strip_prefix(&dir) {
                let mut parts = vec![label.to_string()];
                parts.extend(rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()));
                return (parts.join(" › "), Some(id));
            }
        }
    }
    let name = parent
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| parent.to_string_lossy().to_string());
    (name, None)
}

impl Library {
    pub fn load(dir: PathBuf) -> Self {
        let recents: Vec<RecentEntry> = read_json(&dir.join("recents.json"));
        let prefs = read_json(&dir.join("prefs.json"));
        Self { dir, recents: Mutex::new(recents), prefs: Mutex::new(prefs) }
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.dir.join("cache")
    }

    fn save_recents(&self, list: &[RecentEntry]) -> AppResult<()> {
        write_json(&self.dir.join("recents.json"), &list)
    }

    pub fn recents(&self) -> Vec<RecentView> {
        let mut list = self.recents.lock().clone();
        list.sort_by(|a, b| b.opened_at.cmp(&a.opened_at));
        list.into_iter()
            .map(|entry| {
                let p = Path::new(&entry.path);
                let (folder, place) = describe_folder(p);
                let exists = p.exists();
                RecentView { entry, folder, place, exists }
            })
            .collect()
    }

    /// Enregistre l'ouverture d'un document et renvoie sa dernière page lue.
    pub fn touch(&self, info: TouchInfo) -> AppResult<u32> {
        let mut list = self.recents.lock();
        let now = Utc::now();
        let last_page;
        if let Some(e) = list.iter_mut().find(|e| same_path(&e.path, &info.path)) {
            e.opened_at = now;
            e.page_count = info.page_count;
            e.file_size = info.file_size;
            e.signed = info.signed;
            e.form = info.form;
            e.scanned = info.scanned;
            if e.last_page >= info.page_count {
                e.last_page = 0;
            }
            last_page = e.last_page;
        } else {
            let name = Path::new(&info.path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            list.push(RecentEntry {
                path: info.path,
                name,
                page_count: info.page_count,
                file_size: info.file_size,
                last_page: 0,
                opened_at: now,
                favorite: false,
                signed: info.signed,
                form: info.form,
                scanned: info.scanned,
                annotated: false,
                marks: Vec::new(),
            });
            last_page = 0;
        }
        // Garder les plus récents, mais jamais retirer un favori.
        if list.len() > MAX_RECENTS {
            list.sort_by(|a, b| b.favorite.cmp(&a.favorite).then(b.opened_at.cmp(&a.opened_at)));
            list.truncate(MAX_RECENTS);
        }
        self.save_recents(&list)?;
        Ok(last_page)
    }

    pub fn update(&self, path: &str, f: impl FnOnce(&mut RecentEntry)) -> AppResult<()> {
        let mut list = self.recents.lock();
        if let Some(e) = list.iter_mut().find(|e| same_path(&e.path, path)) {
            f(e);
            self.save_recents(&list)?;
        }
        Ok(())
    }

    pub fn remove(&self, path: &str) -> AppResult<()> {
        let mut list = self.recents.lock();
        list.retain(|e| !same_path(&e.path, path));
        self.save_recents(&list)
    }

    pub fn prefs(&self) -> serde_json::Map<String, serde_json::Value> {
        self.prefs.lock().clone()
    }

    pub fn set_pref(&self, key: String, value: serde_json::Value) -> AppResult<()> {
        let mut prefs = self.prefs.lock();
        prefs.insert(key, value);
        write_json(&self.dir.join("prefs.json"), &*prefs)
    }
}

/// Comparaison de chemins insensible à la casse et aux séparateurs (Windows).
pub fn same_path(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('/', "\\").to_lowercase();
    norm(a) == norm(b)
}

/// Liste les PDF d'un emplacement (deux niveaux de profondeur), du plus récent au plus ancien.
pub fn list_place(id: &str) -> Vec<FileEntry> {
    let Some(dir) = places().into_iter().find(|p| p.0 == id).and_then(|p| p.2) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    collect_pdfs(&dir, 0, &mut out);
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out.truncate(300);
    out
}

fn collect_pdfs(dir: &Path, depth: u32, out: &mut Vec<FileEntry>) {
    if depth > 2 || out.len() > 2_000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name.starts_with('$') {
            continue;
        }
        if ft.is_dir() {
            collect_pdfs(&path, depth + 1, out);
        } else if name.to_lowercase().ends_with(".pdf") {
            let meta = entry.metadata().ok();
            let (folder, _) = describe_folder(&path);
            out.push(FileEntry {
                path: path.to_string_lossy().to_string(),
                name,
                folder,
                file_size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                modified: meta.and_then(|m| m.modified().ok()).map(DateTime::<Utc>::from),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_lib() -> Library {
        let dir = std::env::temp_dir().join(format!("aura-test-{}", std::process::id() as u64 * 1000 + rand_suffix()));
        Library::load(dir)
    }

    fn rand_suffix() -> u64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos() as u64 % 1000
    }

    fn info(path: &str, pages: u32) -> TouchInfo {
        TouchInfo { path: path.into(), page_count: pages, file_size: 10, signed: false, form: false, scanned: false }
    }

    #[test]
    fn reprise_a_la_derniere_page() {
        let lib = tmp_lib();
        assert_eq!(lib.touch(info("C:\\a.pdf", 10)).unwrap(), 0);
        lib.update("c:/a.pdf", |e| e.last_page = 6).unwrap();
        assert_eq!(lib.touch(info("C:\\A.pdf", 10)).unwrap(), 6);
        // Le document a été raccourci : on repart du début.
        assert_eq!(lib.touch(info("C:\\a.pdf", 4)).unwrap(), 0);
        assert_eq!(lib.recents().len(), 1);
    }

    #[test]
    fn retrait_de_la_liste() {
        let lib = tmp_lib();
        lib.touch(info("C:\\a.pdf", 1)).unwrap();
        lib.touch(info("C:\\b.pdf", 1)).unwrap();
        lib.remove("C:\\a.pdf").unwrap();
        let r = lib.recents();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].entry.name, "b.pdf");
    }

    #[test]
    fn libelle_de_dossier_hors_emplacements() {
        let (label, place) = describe_folder(Path::new("Z:\\Projets\\Clients\\x.pdf"));
        assert_eq!(label, "Clients");
        assert!(place.is_none());
    }
}
