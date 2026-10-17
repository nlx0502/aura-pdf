//! Conversion des documents Office en PDF par une suite bureautique installée :
//! Microsoft Office sous Windows (automatisation COM : fidélité parfaite),
//! sinon LibreOffice (Windows, macOS, Linux ; mode sans interface). Le
//! processus est annulable.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::tools::Job;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Application Office capable d'ouvrir une extension.
pub fn app_for(ext: &str) -> Option<&'static str> {
    match ext {
        "doc" | "docx" | "docm" | "rtf" | "odt" | "dot" | "dotx" => Some("Word"),
        "xls" | "xlsx" | "xlsm" | "ods" | "csv" => Some("Excel"),
        "ppt" | "pptx" | "pptm" | "odp" | "pps" | "ppsx" => Some("PowerPoint"),
        _ => None,
    }
}

/// Suite capable de convertir ce type de document (« Microsoft Office »,
/// « LibreOffice »), ou aucune.
pub fn engine(app: &str) -> Option<&'static str> {
    if ms_available(app) {
        Some("Microsoft Office")
    } else if libreoffice().is_some() {
        Some("LibreOffice")
    } else {
        None
    }
}

pub fn available(app: &str) -> bool {
    engine(app).is_some()
}

/// Programme `soffice` de LibreOffice, s'il est installé.
pub fn libreoffice() -> Option<&'static PathBuf> {
    static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let mut c: Vec<PathBuf> = Vec::new();
            if cfg!(target_os = "macos") {
                c.push("/Applications/LibreOffice.app/Contents/MacOS/soffice".into());
                if let Some(h) = std::env::var_os("HOME") {
                    c.push(PathBuf::from(h).join("Applications/LibreOffice.app/Contents/MacOS/soffice"));
                }
            } else if cfg!(windows) {
                for v in ["ProgramFiles", "ProgramFiles(x86)"] {
                    if let Some(d) = std::env::var_os(v) {
                        c.push(PathBuf::from(d).join("LibreOffice").join("program").join("soffice.exe"));
                    }
                }
            } else {
                c.extend(["/usr/bin/soffice", "/usr/bin/libreoffice", "/usr/local/bin/soffice", "/snap/bin/libreoffice"].map(PathBuf::from));
            }
            c.into_iter().find(|p| p.exists())
        })
        .as_ref()
}

#[cfg(not(windows))]
fn ms_available(_app: &str) -> bool {
    false
}

/// Application Microsoft Office installée (ProgID COM enregistré).
#[cfg(windows)]
fn ms_available(app: &str) -> bool {
    static CACHE: OnceLock<[bool; 3]> = OnceLock::new();
    let c = CACHE.get_or_init(|| {
        let has = |prog: &str| {
            let mut cmd = Command::new("reg");
            cmd.args(["query", &format!("HKCR\\{prog}.Application\\CLSID")]).stdout(Stdio::null()).stderr(Stdio::null());
            #[cfg(windows)]
            cmd.creation_flags(CREATE_NO_WINDOW);
            cmd.status().map(|s| s.success()).unwrap_or(false)
        };
        [has("Word"), has("Excel"), has("PowerPoint")]
    });
    match app {
        "Word" => c[0],
        "Excel" => c[1],
        "PowerPoint" => c[2],
        _ => false,
    }
}

pub fn word_available() -> bool {
    available("Word")
}

const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$in = $env:AURA_IN; $out = $env:AURA_OUT
switch ($env:AURA_APP) {
  'Word' {
    $app = New-Object -ComObject Word.Application
    $app.Visible = $false; $app.DisplayAlerts = 0
    try {
      $d = $app.Documents.Open($in, $false, $true, $false)
      $d.ExportAsFixedFormat($out, 17)
      $d.Close(0)
    } finally { $app.Quit() }
  }
  'Excel' {
    $app = New-Object -ComObject Excel.Application
    $app.Visible = $false; $app.DisplayAlerts = $false
    try {
      $b = $app.Workbooks.Open($in, 0, $true)
      $b.ExportAsFixedFormat(0, $out)
      $b.Close($false)
    } finally { $app.Quit() }
  }
  'PowerPoint' {
    $app = New-Object -ComObject PowerPoint.Application
    try {
      $p = $app.Presentations.Open($in, -1, 0, 0)
      $p.SaveAs($out, 32)
      $p.Close()
    } finally { $app.Quit() }
  }
}
"#;

/// Convertit un document Office en PDF avec la suite installée.
pub fn to_pdf(input: &Path, out: &Path, job: &Job) -> Result<(), String> {
    let ext = input.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let app = app_for(&ext).ok_or("format non pris en charge")?;
    let abs_in = std::fs::canonicalize(input).map_err(|e| e.to_string())?;
    let abs_in = abs_in.to_string_lossy().trim_start_matches(r"\\?\").to_string();
    let abs_out = if out.is_absolute() { out.to_path_buf() } else { std::env::current_dir().map_err(|e| e.to_string())?.join(out) };
    match engine(app) {
        Some("Microsoft Office") => ms_office(app, &abs_in, &abs_out, job),
        Some(_) => libreoffice_to_pdf(Path::new(&abs_in), &abs_out, job),
        None => Err(format!("Aucune suite bureautique (Microsoft Office ou LibreOffice) n’est installée pour ouvrir ce document ({app}).")),
    }
}

/// LibreOffice sans interface, avec un profil dédié (une fenêtre LibreOffice
/// déjà ouverte par l'utilisateur ne bloque pas la conversion).
fn libreoffice_to_pdf(input: &Path, out: &Path, job: &Job) -> Result<(), String> {
    let soffice = libreoffice().ok_or("LibreOffice n’est pas installé.")?;
    let work = std::env::temp_dir().join(format!("aura-lo-{}-{}", std::process::id(), job.id));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let profile = std::env::temp_dir().join("aura-lo-profil");
    let profile_url = format!("file:///{}", profile.to_string_lossy().replace('\\', "/").trim_start_matches('/'));
    let mut cmd = Command::new(soffice);
    cmd.arg(format!("-env:UserInstallation={profile_url}"))
        .args(["--headless", "--norestore", "--nolockcheck", "--convert-to", "pdf", "--outdir"])
        .arg(&work)
        .arg(input)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let child = cmd.spawn().map_err(|e| format!("LibreOffice ne démarre pas ({e})"))?;
    let stem = input.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let produced = work.join(format!("{stem}.pdf"));
    let res = wait(child, job, "LibreOffice", &produced).and_then(|_| std::fs::copy(&produced, out).map(|_| ()).map_err(|e| format!("écriture impossible ({e})")));
    let _ = std::fs::remove_dir_all(&work);
    res
}

/// Microsoft Office par automatisation COM (PowerShell).
fn ms_office(app: &str, abs_in: &str, abs_out: &Path, job: &Job) -> Result<(), String> {
    let mut cmd = Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", SCRIPT])
        .env("AURA_IN", abs_in)
        .env("AURA_OUT", abs_out)
        .env("AURA_APP", app)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let child = cmd.spawn().map_err(|e| format!("PowerShell indisponible ({e})"))?;
    wait(child, job, app, abs_out)
}

/// Attend la fin de la conversion (annulable, 5 minutes au plus).
fn wait(mut child: std::process::Child, job: &Job, app: &str, abs_out: &Path) -> Result<(), String> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if status.success() && abs_out.exists() {
                return Ok(());
            }
            let mut err = String::new();
            if let Some(mut e) = child.stderr.take() {
                let _ = std::io::Read::read_to_string(&mut e, &mut err);
            }
            let first = err.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
            return Err(format!("{app} n’a pas pu convertir le document{}", if first.is_empty() { ".".into() } else { format!(" ({first}).") }));
        }
        if job.cancelled() || start.elapsed() > Duration::from_secs(300) {
            let _ = child.kill();
            return Err(if job.cancelled() { crate::tools::CANCELLED.into() } else { format!("{app} ne répond pas.") });
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

pub fn word_to_pdf(input: &Path, out: &Path, job: &Job) -> Result<(), String> {
    to_pdf(input, out, job)
}
