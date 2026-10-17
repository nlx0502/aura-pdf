//! Impression par CUPS (macOS, Linux) : liste des imprimantes (`lpstat`,
//! `lpoptions`) et envoi du PDF imposé (`lp`). CUPS rastérise lui-même le PDF
//! pour l'imprimante ; copies, assemblage, recto verso et niveaux de gris sont
//! transmis comme options de la tâche.

use std::process::Command;

use serde::Serialize;

use super::layout::Paper;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterInfo {
    pub name: String,
    /// État lisible (« Prête », « Hors ligne »…).
    pub status: String,
    /// `ok`, `warn` ou `error` (couleur de la pastille).
    pub level: &'static str,
    pub is_default: bool,
    pub duplex: bool,
    pub color: bool,
}

/// Sortie d'une commande CUPS, en anglais (messages analysables).
fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).env("LC_ALL", "C").env("LANG", "C").output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

/// État d'une imprimante d'après une ligne de `lpstat -p`.
pub fn describe(line: &str) -> (String, &'static str) {
    let l = line.to_ascii_lowercase();
    if l.contains("disabled") {
        ("Hors ligne".into(), "error")
    } else if l.contains("now printing") {
        ("Impression en cours".into(), "ok")
    } else {
        ("Prête".into(), "ok")
    }
}

pub fn list_printers() -> Vec<PrinterInfo> {
    let default = run("lpstat", &["-d"]).and_then(|s| s.split(':').nth(1).map(|d| d.trim().to_string())).filter(|d| !d.is_empty());
    let states = run("lpstat", &["-p"]).unwrap_or_default();
    let mut out: Vec<PrinterInfo> = run("lpstat", &["-e"])
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|name| {
            let line = states.lines().find(|l| l.starts_with(&format!("printer {name} "))).unwrap_or("");
            let (status, level) = describe(line);
            let opts = run("lpoptions", &["-p", name, "-l"]).unwrap_or_default();
            let duplex = opts.lines().any(|l| l.starts_with("Duplex") || l.starts_with("sides"));
            let color = opts.lines().any(|l| (l.starts_with("ColorModel") || l.starts_with("print-color-mode")) && (l.contains("RGB") || l.contains("CMYK") || l.contains("color")));
            PrinterInfo { is_default: default.as_deref() == Some(name), name: name.to_string(), status, level, duplex, color }
        })
        .collect();
    out.sort_by(|a, b| b.is_default.cmp(&a.is_default).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

/// Options de la tâche d'impression.
pub struct JobOptions<'a> {
    pub title: &'a str,
    pub copies: u32,
    pub collate: bool,
    pub duplex: bool,
    pub short_edge: bool,
    pub gray: bool,
    pub paper: Paper,
}

/// Arguments de `lp` (séparés pour être testables).
pub fn lp_args(printer: &str, file: &str, o: &JobOptions) -> Vec<String> {
    let mut a = vec!["-d".to_string(), printer.to_string(), "-t".to_string(), o.title.to_string(), "-n".to_string(), o.copies.max(1).to_string()];
    let mut opt = |v: String| {
        a.push("-o".into());
        a.push(v);
    };
    opt(format!("collate={}", o.collate));
    opt(format!("sides={}", if !o.duplex { "one-sided" } else if o.short_edge { "two-sided-short-edge" } else { "two-sided-long-edge" }));
    opt(format!(
        "media={}",
        match o.paper {
            Paper::A4 => "A4",
            Paper::A3 => "A3",
            Paper::Letter => "Letter",
        }
    ));
    // Le document est déjà imposé à la taille du papier : pas de mise à l'échelle.
    opt("fit-to-page=false".into());
    if o.gray {
        opt("print-color-mode=monochrome".into());
        opt("ColorModel=Gray".into());
    }
    a.push("--".into());
    a.push(file.to_string());
    a
}

/// Envoie le fichier PDF ; renvoie l'identifiant de la tâche CUPS.
pub fn print_file(printer: &str, file: &str, o: &JobOptions) -> AppResult<String> {
    let out = Command::new("lp")
        .args(lp_args(printer, file, o))
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| AppError::new("printer", format!("Service d’impression indisponible ({e}).")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(AppError::new("printer", format!("L’imprimante a refusé le document ({err}).")));
    }
    // « request id is Imprimante-12 (1 file(s)) »
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    Ok(s.split_whitespace().nth(3).unwrap_or("").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_de_lp() {
        let o = JobOptions { title: "Rapport", copies: 2, collate: true, duplex: true, short_edge: false, gray: true, paper: Paper::A4 };
        let a = lp_args("Bureau", "/tmp/x.pdf", &o).join(" ");
        assert_eq!(a, "-d Bureau -t Rapport -n 2 -o collate=true -o sides=two-sided-long-edge -o media=A4 -o fit-to-page=false -o print-color-mode=monochrome -o ColorModel=Gray -- /tmp/x.pdf");
        assert_eq!(describe("printer Bureau disabled since …").1, "error");
        assert_eq!(describe("printer Bureau is idle.  enabled since …").0, "Prête");
    }
}
