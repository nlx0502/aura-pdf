//! Types échangés entre le moteur PDF et l'interface (sérialisés en camelCase).

use serde::Serialize;

/// Taille d'une page affichée, en points PDF (1/72 de pouce), rotation de la
/// page incluse.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct PageSize {
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineItem {
    pub title: String,
    pub depth: u32,
    pub page: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInfo {
    pub index: u32,
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocInfo {
    pub id: u32,
    pub path: String,
    pub name: String,
    pub file_size: u64,
    pub page_count: u32,
    pub pages: Vec<PageSize>,
    pub version: String,
    pub metadata: Metadata,
    pub outline: Vec<OutlineItem>,
    pub attachments: Vec<AttachmentInfo>,
    pub has_form: bool,
    pub signature_count: u32,
    pub encrypted: bool,
    pub scanned: bool,
    pub tagged: bool,
}

/// Rectangle en points d'affichage (origine en haut à gauche de la page).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub page: u32,
    pub rects: Vec<Rect>,
    pub before: String,
    pub matched: String,
    pub after: String,
}

/// Un mot de la couche texte (sélection, copie).
#[derive(Debug, Clone, Serialize)]
pub struct TextWord {
    pub t: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Vrai si un espace doit suivre ce mot lors de la copie.
    pub sp: bool,
    /// Vrai si ce mot termine une ligne.
    pub nl: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageLink {
    pub rect: Rect,
    pub page: Option<u32>,
    pub uri: Option<String>,
}

/// Bitmap RGBA renvoyé par le protocole de rendu.
pub struct RenderedBitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub enum RenderOutcome {
    Ok(RenderedBitmap),
    /// La demande a été dépassée par un défilement ou un zoom plus récent.
    Stale,
    Err(String),
}
