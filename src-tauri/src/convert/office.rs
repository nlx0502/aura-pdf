//! Conversion par Microsoft Office quand il est installé (Word, Excel,
//! PowerPoint), par automatisation COM : Office produit lui-même le PDF, avec
//! une fidélité parfaite. Le processus est annulable.

use std::path::Path;
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

/// Application Office installée (ProgID COM enregistré).
pub fn available(app: &str) -> bool {
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

/// Convertit un document Office en PDF avec l'application correspondante.
pub fn to_pdf(input: &Path, out: &Path, job: &Job) -> Result<(), String> {
    let ext = input.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let app = app_for(&ext).ok_or("format non pris en charge")?;
    if !available(app) {
        return Err(format!("Microsoft {app} n’est pas installé."));
    }
    let abs_in = std::fs::canonicalize(input).map_err(|e| e.to_string())?;
    let abs_in = abs_in.to_string_lossy().trim_start_matches(r"\\?\").to_string();
    let abs_out = if out.is_absolute() { out.to_path_buf() } else { std::env::current_dir().map_err(|e| e.to_string())?.join(out) };
    let mut cmd = Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", SCRIPT])
        .env("AURA_IN", &abs_in)
        .env("AURA_OUT", &abs_out)
        .env("AURA_APP", app)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let mut child = cmd.spawn().map_err(|e| format!("PowerShell indisponible ({e})"))?;
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
