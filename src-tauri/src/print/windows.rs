//! Impression Windows : liste des imprimantes (état, capacités) et envoi des
//! feuilles par GDI. Les feuilles arrivent déjà composées (bitmap BGRA) ; ce
//! module ne fait que configurer le pilote (papier, orientation, recto verso,
//! couleur) et transférer les pixels.

use serde::Serialize;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Printing::*;
use windows::Win32::Storage::Xps::*;

use super::layout::Paper;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterInfo {
    pub name: String,
    /// État lisible (« Prête », « Hors ligne », « Prête · toner faible »…).
    pub status: String,
    /// `ok`, `warn` ou `error` (couleur de la pastille).
    pub level: &'static str,
    pub is_default: bool,
    pub duplex: bool,
    pub color: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterMargins {
    /// Marge non imprimable la plus grande, en points.
    pub margin: f32,
    pub dpi: u32,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn from_pwstr(p: PWSTR) -> String {
    if p.is_null() {
        String::new()
    } else {
        p.to_string().unwrap_or_default()
    }
}

fn default_printer() -> Option<String> {
    unsafe {
        let mut len = 0u32;
        let _ = GetDefaultPrinterW(None, &mut len);
        if len == 0 {
            return None;
        }
        let mut buf = vec![0u16; len as usize];
        if GetDefaultPrinterW(Some(PWSTR(buf.as_mut_ptr())), &mut len).as_bool() {
            Some(String::from_utf16_lossy(&buf[..len.saturating_sub(1) as usize]))
        } else {
            None
        }
    }
}

/// Traduit l'état Windows d'une imprimante.
pub fn describe_status(status: u32, attributes: u32) -> (String, &'static str) {
    const PAUSED: u32 = 0x1;
    const ERROR: u32 = 0x2;
    const PAPER_JAM: u32 = 0x8;
    const PAPER_OUT: u32 = 0x10;
    const OFFLINE: u32 = 0x80;
    const BUSY: u32 = 0x200;
    const PRINTING: u32 = 0x400;
    const NOT_AVAILABLE: u32 = 0x1000;
    const TONER_LOW: u32 = 0x20000;
    const NO_TONER: u32 = 0x40000;
    const USER_INTERVENTION: u32 = 0x100000;
    const DOOR_OPEN: u32 = 0x400000;
    const ATTR_WORK_OFFLINE: u32 = 0x400;

    if status & (OFFLINE | NOT_AVAILABLE) != 0 || attributes & ATTR_WORK_OFFLINE != 0 {
        return ("Hors ligne".into(), "error");
    }
    if status & PAPER_JAM != 0 {
        return ("Bourrage papier".into(), "error");
    }
    if status & PAPER_OUT != 0 {
        return ("Plus de papier".into(), "error");
    }
    if status & NO_TONER != 0 {
        return ("Toner épuisé".into(), "error");
    }
    if status & DOOR_OPEN != 0 {
        return ("Capot ouvert".into(), "error");
    }
    if status & (ERROR | USER_INTERVENTION) != 0 {
        return ("Intervention requise".into(), "error");
    }
    if status & PAUSED != 0 {
        return ("En pause".into(), "warn");
    }
    if status & TONER_LOW != 0 {
        return ("Prête · toner faible".into(), "warn");
    }
    if status & PRINTING != 0 {
        return ("Impression en cours".into(), "ok");
    }
    if status & BUSY != 0 {
        return ("Occupée".into(), "warn");
    }
    ("Prête".into(), "ok")
}

pub fn list_printers() -> Vec<PrinterInfo> {
    let def = default_printer();
    let mut out = Vec::new();
    unsafe {
        let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
        let (mut needed, mut count) = (0u32, 0u32);
        let _ = EnumPrintersW(flags, PCWSTR::null(), 2, None, &mut needed, &mut count);
        if needed == 0 {
            return out;
        }
        // Tampon aligné pour les structures PRINTER_INFO_2W.
        let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
        let bytes = std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, needed as usize);
        if EnumPrintersW(flags, PCWSTR::null(), 2, Some(bytes), &mut needed, &mut count).is_err() {
            return out;
        }
        let infos = std::slice::from_raw_parts(buf.as_ptr() as *const PRINTER_INFO_2W, count as usize);
        for info in infos {
            let name = from_pwstr(info.pPrinterName);
            if name.is_empty() {
                continue;
            }
            let port = from_pwstr(info.pPortName);
            let (status, level) = describe_status(info.Status, info.Attributes);
            let wname = wide(&name);
            let wport = wide(&port);
            let duplex = DeviceCapabilitiesW(PCWSTR(wname.as_ptr()), PCWSTR(wport.as_ptr()), DC_DUPLEX, None, None) == 1;
            let color = DeviceCapabilitiesW(PCWSTR(wname.as_ptr()), PCWSTR(wport.as_ptr()), DC_COLORDEVICE, None, None) == 1;
            out.push(PrinterInfo { is_default: def.as_deref() == Some(name.as_str()), name, status, level, duplex, color });
        }
    }
    // Imprimante par défaut en premier, puis ordre alphabétique.
    out.sort_by(|a, b| b.is_default.cmp(&a.is_default).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

/// Réglages de pilote demandés pour une impression.
#[derive(Debug, Clone, Copy)]
pub struct DriverSettings {
    pub paper: Paper,
    pub landscape: bool,
    pub duplex: bool,
    pub short_edge: bool,
    pub gray: bool,
}

/// DEVMODE du pilote, complété avec nos réglages (fusion par le pilote).
fn devmode(printer: &str, s: &DriverSettings) -> AppResult<Vec<u8>> {
    unsafe {
        let wname = wide(printer);
        let mut h = PRINTER_HANDLE::default();
        OpenPrinterW(PCWSTR(wname.as_ptr()), &mut h, None).map_err(|e| AppError::new("printer", format!("Imprimante indisponible ({e}).")))?;
        let size = DocumentPropertiesW(None, h, PCWSTR(wname.as_ptr()), None, None, 0);
        if size <= 0 {
            let _ = ClosePrinter(h);
            return Err(AppError::new("printer", "Le pilote de l’imprimante ne répond pas."));
        }
        let mut buf = vec![0u8; size as usize];
        let dm = buf.as_mut_ptr() as *mut DEVMODEW;
        DocumentPropertiesW(None, h, PCWSTR(wname.as_ptr()), Some(dm), None, DM_OUT_BUFFER.0);
        let d = &mut *dm;
        d.Anonymous1.Anonymous1.dmPaperSize = match s.paper {
            Paper::A4 => DMPAPER_A4,
            Paper::A3 => DMPAPER_A3,
            Paper::Letter => DMPAPER_LETTER,
        } as i16;
        d.Anonymous1.Anonymous1.dmOrientation = if s.landscape { DMORIENT_LANDSCAPE } else { DMORIENT_PORTRAIT } as i16;
        d.Anonymous1.Anonymous1.dmCopies = 1;
        d.dmDuplex = if !s.duplex {
            DMDUP_SIMPLEX
        } else if s.short_edge != s.landscape {
            // Bord court en portrait = retournement horizontal ; l'inverse en paysage.
            DMDUP_HORIZONTAL
        } else {
            DMDUP_VERTICAL
        };
        d.dmColor = if s.gray { DMCOLOR_MONOCHROME } else { DMCOLOR_COLOR };
        d.dmFields |= DM_PAPERSIZE | DM_ORIENTATION | DM_COPIES | DM_DUPLEX | DM_COLOR;
        // Laisser le pilote valider et compléter.
        let mut merged = vec![0u8; size as usize];
        DocumentPropertiesW(
            None,
            h,
            PCWSTR(wname.as_ptr()),
            Some(merged.as_mut_ptr() as *mut DEVMODEW),
            Some(dm as *const DEVMODEW),
            DM_IN_BUFFER.0 | DM_OUT_BUFFER.0,
        );
        let _ = ClosePrinter(h);
        Ok(merged)
    }
}

/// Marges non imprimables et résolution, pour le papier choisi.
pub fn margins(printer: &str, paper: Paper, landscape: bool) -> AppResult<PrinterMargins> {
    let dm = devmode(printer, &DriverSettings { paper, landscape, duplex: false, short_edge: false, gray: false })?;
    unsafe {
        let wname = wide(printer);
        let dc = CreateICW(PCWSTR::null(), PCWSTR(wname.as_ptr()), PCWSTR::null(), Some(dm.as_ptr() as *const DEVMODEW));
        if dc.is_invalid() {
            return Err(AppError::new("printer", "Imprimante indisponible."));
        }
        let dpi_x = GetDeviceCaps(Some(dc), LOGPIXELSX).max(72) as f32;
        let dpi_y = GetDeviceCaps(Some(dc), LOGPIXELSY).max(72) as f32;
        let off_x = GetDeviceCaps(Some(dc), PHYSICALOFFSETX) as f32 / dpi_x * 72.0;
        let off_y = GetDeviceCaps(Some(dc), PHYSICALOFFSETY) as f32 / dpi_y * 72.0;
        let phys_w = GetDeviceCaps(Some(dc), PHYSICALWIDTH) as f32;
        let horz = GetDeviceCaps(Some(dc), HORZRES) as f32;
        let right = (phys_w - horz) / dpi_x * 72.0 - off_x;
        let _ = DeleteDC(dc);
        Ok(PrinterMargins { margin: off_x.max(off_y).max(right).clamp(0.0, 36.0), dpi: dpi_x as u32 })
    }
}

/// Document d'impression GDI en cours.
pub struct GdiJob {
    dc: HDC,
    printer: String,
    base: DriverSettings,
    current_landscape: bool,
    open: bool,
    page_open: bool,
    /// Résolution de rendu des feuilles (plafonnée pour la mémoire).
    pub dpi: u32,
}

impl GdiJob {
    /// `output` : fichier de sortie (imprimantes virtuelles, tests) au lieu du port de l'imprimante.
    pub fn start(printer: &str, title: &str, s: DriverSettings, output: Option<&str>) -> AppResult<Self> {
        let dm = devmode(printer, &s)?;
        unsafe {
            let wname = wide(printer);
            let dc = CreateDCW(PCWSTR::null(), PCWSTR(wname.as_ptr()), PCWSTR::null(), Some(dm.as_ptr() as *const DEVMODEW));
            if dc.is_invalid() {
                return Err(AppError::new("printer", "Impossible de se connecter à l’imprimante."));
            }
            let wtitle = wide(title);
            let wout = output.map(wide);
            let info = DOCINFOW {
                cbSize: std::mem::size_of::<DOCINFOW>() as i32,
                lpszDocName: PCWSTR(wtitle.as_ptr()),
                lpszOutput: wout.as_ref().map(|w| PCWSTR(w.as_ptr())).unwrap_or(PCWSTR::null()),
                ..Default::default()
            };
            if StartDocW(dc, &info) <= 0 {
                let _ = DeleteDC(dc);
                return Err(AppError::new("printer", "L’imprimante a refusé le document."));
            }
            let dpi = (GetDeviceCaps(Some(dc), LOGPIXELSX).max(72) as u32).min(300);
            Ok(Self { dc, printer: printer.to_string(), base: s, current_landscape: s.landscape, open: true, page_open: false, dpi })
        }
    }

    /// Imprime une face : bitmap BGRA (haut en bas) couvrant toute la feuille.
    pub fn page(&mut self, landscape: bool, bgra: &[u8], w: u32, h: u32) -> AppResult<()> {
        unsafe {
            // Orientation propre à cette face (mode automatique) : on compare à
            // l'orientation *actuelle* du contexte, pas à celle du départ.
            if landscape != self.current_landscape {
                if let Ok(dm) = devmode(&self.printer, &DriverSettings { landscape, ..self.base }) {
                    ResetDCW(self.dc, dm.as_ptr() as *const DEVMODEW);
                    self.current_landscape = landscape;
                }
            }
            if StartPage(self.dc) <= 0 {
                return Err(AppError::new("printer", "Erreur de l’imprimante au début d’une page."));
            }
            self.page_open = true;
            let phys_w = GetDeviceCaps(Some(self.dc), PHYSICALWIDTH);
            let phys_h = GetDeviceCaps(Some(self.dc), PHYSICALHEIGHT);
            let off_x = GetDeviceCaps(Some(self.dc), PHYSICALOFFSETX);
            let off_y = GetDeviceCaps(Some(self.dc), PHYSICALOFFSETY);
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w as i32,
                    biHeight: -(h as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            // La feuille couvre la page physique ; l'origine GDI est la zone imprimable.
            StretchDIBits(self.dc, -off_x, -off_y, phys_w, phys_h, 0, 0, w as i32, h as i32, Some(bgra.as_ptr() as *const _), &bmi, DIB_RGB_COLORS, SRCCOPY);
            if EndPage(self.dc) <= 0 {
                self.page_open = false;
                return Err(AppError::new("printer", "Erreur de l’imprimante à la fin d’une page."));
            }
            self.page_open = false;
            Ok(())
        }
    }

    pub fn finish(mut self) -> AppResult<()> {
        unsafe {
            self.open = false;
            let ok = EndDoc(self.dc) > 0;
            let _ = DeleteDC(self.dc);
            if ok {
                Ok(())
            } else {
                Err(AppError::new("printer", "L’imprimante n’a pas terminé le document."))
            }
        }
    }
}

/// Commandes envoyées au thread d'impression GDI.
pub enum GdiCmd {
    Page { landscape: bool, bgra: Vec<u8>, w: u32, h: u32 },
    Finish,
}

/// Tâche GDI exécutée sur son propre thread : certains pilotes (imprimantes
/// virtuelles) ouvrent une fenêtre pendant `StartDoc`, ce qui ne doit jamais
/// bloquer le thread moteur ni l'interface.
pub struct GdiThread {
    pub tx: crossbeam_channel::Sender<GdiCmd>,
    /// Résolution choisie (reçue une fois le document ouvert), ou erreur.
    pub ready: crossbeam_channel::Receiver<AppResult<u32>>,
    /// Résultat final (après `Finish`).
    pub done: crossbeam_channel::Receiver<AppResult<()>>,
    /// Nombre de faces effectivement transmises au pilote.
    pub sent: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

pub fn spawn(printer: String, title: String, s: DriverSettings, output: Option<String>) -> GdiThread {
    use std::sync::atomic::Ordering;
    let (tx, rx) = crossbeam_channel::bounded::<GdiCmd>(2);
    let (ready_tx, ready) = crossbeam_channel::bounded(1);
    let (done_tx, done) = crossbeam_channel::bounded(1);
    let sent = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = sent.clone();
    std::thread::Builder::new()
        .name("aura-pdf-print".into())
        .spawn(move || {
            let mut job = match GdiJob::start(&printer, &title, s, output.as_deref()) {
                Ok(j) => {
                    let _ = ready_tx.send(Ok(j.dpi));
                    j
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            // Fin du canal sans `Finish` = annulation : `job` est abandonné (AbortDoc).
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    GdiCmd::Page { landscape, bgra, w, h } => {
                        if let Err(e) = job.page(landscape, &bgra, w, h) {
                            let _ = done_tx.send(Err(e));
                            return;
                        }
                        counter.fetch_add(1, Ordering::Relaxed);
                    }
                    GdiCmd::Finish => {
                        let _ = done_tx.send(job.finish());
                        return;
                    }
                }
            }
        })
        .expect("thread d’impression");
    GdiThread { tx, ready, done, sent }
}

impl Drop for GdiJob {
    fn drop(&mut self) {
        // Annulation ou erreur : abandonner proprement le document.
        if self.open {
            unsafe {
                let _ = AbortDoc(self.dc);
                let _ = DeleteDC(self.dc);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etats_des_imprimantes() {
        assert_eq!(describe_status(0, 0).0, "Prête");
        assert_eq!(describe_status(0x20000, 0).1, "warn");
        assert_eq!(describe_status(0x80, 0).0, "Hors ligne");
        assert_eq!(describe_status(0, 0x400).0, "Hors ligne");
        assert_eq!(describe_status(0x10, 0).0, "Plus de papier");
    }

    #[test]
    fn liste_sans_plantage() {
        // Dépend de la machine : on vérifie seulement que l'appel aboutit.
        let _ = list_printers();
    }
}
