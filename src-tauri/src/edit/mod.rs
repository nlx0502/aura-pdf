//! Module Éditer : organisation des pages, en-têtes, numérotation, filigrane,
//! rognage, liens et caviardage, appliqués avec lopdf (licence MIT).
//!
//! L'interface décrit le document voulu par un [`Plan`] (ordre des pages,
//! rotations, pages insérées, réglages) ; [`apply`] produit le nouveau fichier
//! en une passe. Le contenu d'origine n'est jamais regénéré : les ajouts
//! (en-têtes, filigrane…) sont des flux supplémentaires, et seul le caviardage
//! réécrit le contenu des pages concernées, glyphe par glyphe.

pub mod fonts;
pub mod layers;
pub mod links;
pub mod pages;
pub mod redact;
pub mod stamp;
#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::path::Path;

use lopdf::{Document, ObjectId};
use serde::Deserialize;

use crate::annot::geom::Rect;
use crate::error::{AppError, AppResult};

/// Origine d'une page du document final.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PageSrc {
    /// Page du document ouvert (index d'origine).
    Orig { index: u32 },
    /// Page vierge (taille en points).
    Blank { w: f32, h: f32 },
    /// Page d'un autre fichier PDF.
    File { path: String, index: u32 },
}

/// Modification d'un lien (annotation /Link) d'une page d'origine.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum LinkEdit {
    /// Nouveau lien (rectangle en points d'affichage de la page d'origine).
    Add { rect: Rect, uri: String },
    /// Nouvelle adresse d'un lien existant (index dans /Annots).
    Update { index: u32, uri: String },
    Delete { index: u32 },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanPage {
    pub src: PageSrc,
    /// Quarts de tour horaires ajoutés à la rotation de la page.
    #[serde(default)]
    pub rot: i32,
    /// Rogner cette page avec les marges du plan.
    #[serde(default)]
    pub crop: bool,
    /// Zones à caviarder, en points d'affichage de la page source (rotation
    /// propre incluse, rotation ajoutée exclue).
    #[serde(default)]
    pub redact: Vec<Rect>,
    #[serde(default)]
    pub links: Vec<LinkEdit>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderFooter {
    #[serde(default)]
    pub header: String,
    #[serde(default)]
    pub footer: String,
    #[serde(default = "default_small")]
    pub size: f32,
    #[serde(default = "default_gray")]
    pub color: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Numbering {
    /// Modèle : `{n}` numéro, `{total}` nombre de pages.
    pub format: String,
    /// tl, tc, tr, bl, bc, br.
    pub position: String,
    #[serde(default = "default_small")]
    pub size: f32,
    /// Numéro de la première page numérotée.
    #[serde(default = "default_one")]
    pub start: u32,
    /// Première page (page de garde) sans numéro.
    #[serde(default)]
    pub skip_first: bool,
    #[serde(default = "default_gray")]
    pub color: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Watermark {
    pub text: String,
    /// 0 à 1.
    pub opacity: f32,
    pub color: String,
    /// Angle en degrés (sens antihoraire à l'écran).
    #[serde(default = "default_angle")]
    pub angle: f32,
}

/// Marges de rognage en points (sens d'affichage final).
#[derive(Debug, Clone, Copy, Deserialize, Default)]
pub struct Margins {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub pages: Vec<PlanPage>,
    #[serde(default)]
    pub header: Option<HeaderFooter>,
    #[serde(default)]
    pub numbering: Option<Numbering>,
    #[serde(default)]
    pub watermark: Option<Watermark>,
    #[serde(default)]
    pub crop: Option<Margins>,
    /// Supprimer les métadonnées (toujours fait en cas de caviardage).
    #[serde(default)]
    pub clean_metadata: bool,
}

fn default_small() -> f32 {
    9.0
}
fn default_one() -> u32 {
    1
}
fn default_gray() -> String {
    "#5A5B60".into()
}
fn default_angle() -> f32 {
    35.0
}

impl Plan {
    /// Le plan reproduit-il le document tel quel (aucune modification) ?
    pub fn is_identity(&self, page_count: u32) -> bool {
        self.pages.len() as u32 == page_count
            && self.pages.iter().enumerate().all(|(i, p)| {
                matches!(p.src, PageSrc::Orig { index } if index == i as u32)
                    && p.rot.rem_euclid(4) == 0
                    && !p.crop
                    && p.redact.is_empty()
                    && p.links.is_empty()
            })
            && self.header.as_ref().map(|h| h.header.trim().is_empty() && h.footer.trim().is_empty()).unwrap_or(true)
            && self.numbering.is_none()
            && self.watermark.as_ref().map(|w| w.text.trim().is_empty()).unwrap_or(true)
            && !self.clean_metadata
    }

    pub fn has_redactions(&self) -> bool {
        self.pages.iter().any(|p| !p.redact.is_empty())
    }
}

pub(crate) fn load(path: &Path, password: Option<&str>) -> AppResult<Document> {
    let bytes = std::fs::read(path)?;
    match password {
        Some(p) => Document::load_mem_with_password(&bytes, p),
        None => Document::load_mem(&bytes),
    }
    .map_err(|e| AppError::new("edit", format!("Impossible de lire le document ({e}).")))
}

/// Applique le plan au fichier `src` et écrit le résultat dans `out`
/// (fichier temporaire puis remplacement : `out` n'est jamais à moitié écrit).
///
/// Document chiffré : `keep` est l'état de chiffrement du fichier d'origine ;
/// le résultat est chiffré à nouveau avec la même clé (mêmes mots de passe et
/// mêmes permissions).
pub fn apply(src: &Path, password: Option<&str>, plan: &Plan, out: &Path, keep: Option<&lopdf::encryption::EncryptionState>) -> AppResult<()> {
    let mut doc = load(src, password)?;
    if doc.trailer.has(b"Encrypt") {
        // Mot de passe absent ou refusé : le contenu n'a pas été déchiffré.
        return Err(AppError::new("edit", "Le document protégé n’a pas pu être déchiffré."));
    }
    let id = doc.trailer.get(b"ID").ok().cloned();
    let own = doc.encryption_state.take();
    crate::security::strip_xref_streams(&mut doc);
    let mut bytes = build(&mut doc, plan).map_err(|e| AppError::new("edit", e))?;
    if let Some(state) = keep.or(own.as_ref()) {
        bytes = reencrypt(&bytes, state, id).map_err(|e| AppError::new("edit", e))?;
    }
    write_atomic(out, &bytes)
}

/// Chiffre un document en clair avec un état existant (l'identifiant de
/// fichier d'origine est conservé : la clé AES-128 en dépend).
pub(crate) fn reencrypt(bytes: &[u8], state: &lopdf::encryption::EncryptionState, id: Option<lopdf::Object>) -> Result<Vec<u8>, String> {
    let mut doc = Document::load_mem(bytes).map_err(|e| format!("relecture impossible ({e})"))?;
    if let Some(id) = id {
        doc.trailer.set("ID", id);
    }
    doc.encrypt(state).map_err(|e| format!("chiffrement impossible ({e})"))?;
    crate::security::complete_encrypt_dict(&mut doc);
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| format!("écriture impossible ({e})"))?;
    Ok(out)
}

pub(crate) fn write_atomic(out: &Path, bytes: &[u8]) -> AppResult<()> {
    let tmp = out.with_extension("aura-tmp");
    std::fs::write(&tmp, bytes).map_err(|e| AppError::new("save", format!("Impossible d’écrire le fichier ({e}).")))?;
    std::fs::rename(&tmp, out).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        AppError::new("save", format!("Le fichier est utilisé par un autre programme ({e})."))
    })
}

/// Construit le nouveau document en mémoire.
pub fn build(doc: &mut Document, plan: &Plan) -> Result<Vec<u8>, String> {
    if plan.pages.is_empty() {
        return Err("Un document doit garder au moins une page.".into());
    }
    let orig: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for p in &plan.pages {
        if let PageSrc::Orig { index } = p.src {
            if index as usize >= orig.len() {
                return Err(format!("Page {} introuvable.", index + 1));
            }
        }
    }

    // 1. Caviardage et liens, sur les pages d'origine (avant toute réorganisation).
    let mut redacted: HashSet<ObjectId> = HashSet::new();
    let mut replaced: HashSet<ObjectId> = HashSet::new();
    for p in &plan.pages {
        let PageSrc::Orig { index } = p.src else { continue };
        let id = orig[index as usize];
        if !p.redact.is_empty() && redacted.insert(id) {
            replaced.extend(redact::redact_page(doc, id, &p.redact).map_err(|e| format!("Page {} : {e}", index + 1))?);
        }
        if !p.links.is_empty() {
            links::apply(doc, id, &p.links)?;
        }
    }

    // 2. Nouvelle liste de pages.
    let ids = pages::assemble(doc, &orig, plan)?;
    if !replaced.is_empty() {
        pages::drop_unused_xobjects(doc, &ids, &replaced);
    }

    // 3. Rognage puis ajouts (les ajouts se placent dans la page rognée).
    if let Some(m) = plan.crop {
        for (p, &id) in plan.pages.iter().zip(&ids) {
            if p.crop {
                pages::crop(doc, id, m);
            }
        }
    }
    stamp::apply(doc, &ids, plan)?;

    // 4. Nettoyage.
    if !redacted.is_empty() || plan.clean_metadata {
        pages::strip_metadata(doc, &redacted);
    }
    doc.prune_objects();
    doc.compress();
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| format!("écriture impossible ({e})"))?;
    Ok(out)
}
