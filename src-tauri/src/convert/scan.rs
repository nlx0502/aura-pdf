//! Sources de création : scanner (WIA, boîte de dialogue de Windows ; non
//! disponible sur Mac) et presse-papiers (image ou texte).

#[cfg(windows)]
use std::process::{Command, Stdio};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
try {
  $dlg = New-Object -ComObject WIA.CommonDialog
  # Scanner, qualité maximale, format PNG.
  $img = $dlg.ShowAcquireImage(1, 0, 131072, '{B96B3CAF-0728-11D3-9D7B-0000F81EF32E}', $false, $true, $false)
  if ($img -eq $null) { exit 2 }
  $img.SaveFile($env:AURA_OUT)
} catch {
  if ($_.Exception.HResult -eq -2145320939) { exit 3 }
  if ($_.Exception.Message -match '0x80210015') { exit 3 }
  if ($_.Exception.Message -match '0x80210064') { exit 2 }
  if ($_.Exception.HResult -eq -2145320955 -or $_.Exception.Message -match '0x80210005') { exit 4 }
  [Console]::Error.WriteLine($_.Exception.Message)
  exit 1
}
"#;

/// Numérisation : boîte de dialogue WIA de Windows uniquement.
#[cfg(not(windows))]
pub fn acquire() -> Result<Option<String>, String> {
    Err("La numérisation directe n’est disponible que sous Windows. Sur Mac, numérisez avec Transfert d’images ou Aperçu (Fichier › Importer depuis le scanner), puis créez le PDF à partir des images.".into())
}

/// Numérise une page ; `None` si l'utilisateur annule.
#[cfg(windows)]
pub fn acquire() -> Result<Option<String>, String> {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let out = std::env::temp_dir().join(format!("aura-scan-{nanos}.png"));
    let mut cmd = Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", SCRIPT]).env("AURA_OUT", &out).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let res = cmd.output().map_err(|e| format!("PowerShell indisponible ({e})"))?;
    match res.status.code() {
        Some(0) if out.exists() => Ok(Some(out.to_string_lossy().into())),
        Some(2) => Ok(None),
        Some(3) => Err("Aucun scanner n’est détecté. Vérifiez qu’il est allumé et connecté.".into()),
        Some(4) => Err("Le scanner est hors ligne. Allumez-le ou vérifiez sa connexion, puis réessayez.".into()),
        _ => {
            let msg = String::from_utf8_lossy(&res.stderr).lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
            Err(format!("La numérisation a échoué{}", if msg.is_empty() { ".".into() } else { format!(" ({msg}).") }))
        }
    }
}

pub enum Clip {
    Image(String),
    Text(String),
}

/// Contenu du presse-papiers : image (enregistrée en PNG temporaire) ou texte.
pub fn clipboard() -> Result<Clip, String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("presse-papiers inaccessible ({e})"))?;
    if let Ok(img) = cb.get_image() {
        let rgba = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned()).ok_or("image invalide")?;
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let out = std::env::temp_dir().join(format!("aura-presse-papiers-{nanos}.png"));
        rgba.save(&out).map_err(|e| e.to_string())?;
        return Ok(Clip::Image(out.to_string_lossy().into()));
    }
    match cb.get_text() {
        Ok(t) if !t.trim().is_empty() => Ok(Clip::Text(t)),
        _ => Err("Le presse-papiers ne contient ni image ni texte. Copiez d’abord une capture, une image ou du texte.".into()),
    }
}
