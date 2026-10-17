//! Module Signer : signature manuscrite (dessinée, tapée, importée) et
//! signature numérique PDF avec un certificat (.pfx / .p12), vérification des
//! signatures existantes.
//!
//! La signature est ajoutée par mise à jour incrémentale : le fichier d'origine
//! n'est pas modifié, les signatures déjà présentes restent valides. Pour une
//! signature numérique, un emplacement est réservé dans /Contents, l'empreinte
//! SHA-256 de tout le fichier hors de cet emplacement (/ByteRange) est signée,
//! puis le conteneur CMS est écrit à sa place.

pub mod appearance;
pub mod cms;
pub mod crypto;
pub mod store;
pub mod verify;

use std::path::{Path, PathBuf};

use lopdf::{dictionary, Document, IncrementalDocument, Object, ObjectId, StringFormat};
use serde::Deserialize;

use crate::annot::geom::Rect;
use crate::annot::pdf::{deref, get, now_pdf_date, page_geom, text_object};
use crate::tools::{Job, Output};
use appearance::VisualSpec;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertRequest {
    pub path: String,
    pub password: String,
    /// Adresse du serveur d'horodatage (RFC 3161), si demandé.
    #[serde(default)]
    pub tsa: Option<String>,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub location: String,
    /// Profil PAdES (ETSI.CAdES.detached) ; sinon PKCS #7 détaché (adbe).
    #[serde(default = "yes")]
    pub pades: bool,
    /// Certification (DocMDP) : 1 aucune modification, 2 remplissage et
    /// signatures, 3 avec en plus les annotations.
    #[serde(default)]
    pub certify: Option<u8>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignRequest {
    /// Page (index 0) et rectangle en points d'affichage.
    pub page: u32,
    pub rect: Rect,
    pub visual: VisualSpec,
    /// Champ de signature existant à remplir.
    #[serde(default)]
    pub field: Option<String>,
    #[serde(default)]
    pub cert: Option<CertRequest>,
    /// Auteur (signature manuscrite sans certificat).
    #[serde(default)]
    pub author: String,
}

/// Valeurs provisoires remplacées après écriture du fichier.
const BR_PLACEHOLDER: [i64; 3] = [1_000_000_001, 1_000_000_002, 1_000_000_003];

/// Taille réservée au conteneur CMS (octets).
fn reserve(signer: &crypto::Signer, tsa: bool) -> usize {
    let certs: usize = signer.chain.iter().map(|c| der::Encode::encoded_len(c).map(|l| u32::from(l) as usize).unwrap_or(2000)).sum();
    4096 + certs + if tsa { 12_000 } else { 0 }
}

/// Champ de formulaire de signature par son nom complet.
fn find_field(doc: &Document, name: &str) -> Option<ObjectId> {
    let acro = doc.catalog().ok().and_then(|c| get(doc, c, b"AcroForm")).and_then(|a| a.as_dict().ok())?;
    let fields = get(doc, acro, b"Fields").and_then(|f| f.as_array().ok())?;
    fn walk(doc: &Document, list: &[Object], prefix: &str, name: &str, depth: u32) -> Option<ObjectId> {
        for f in list {
            let Ok(id) = f.as_reference() else { continue };
            let Ok(d) = doc.get_dictionary(id) else { continue };
            let t = d.get(b"T").map(|t| crate::annot::pdf::text(deref(doc, t))).unwrap_or_default();
            let full = if prefix.is_empty() { t.clone() } else if t.is_empty() { prefix.to_string() } else { format!("{prefix}.{t}") };
            if full == name && (d.has(b"FT") || d.has(b"Rect")) {
                return Some(id);
            }
            if depth < 12 {
                if let Some(k) = get(doc, d, b"Kids").and_then(|k| k.as_array().ok()) {
                    if let Some(r) = walk(doc, k, &full, name, depth + 1) {
                        return Some(r);
                    }
                }
            }
        }
        None
    }
    walk(doc, fields, "", name, 0)
}

pub fn sign(path: &Path, req: &SignRequest, job: &Job) -> Result<Output, String> {
    let digital = req.cert.is_some();
    job.progress(0, 0, 1, "");
    let bytes = std::fs::read(path).map_err(|e| format!("Fichier illisible ({e})."))?;
    let prev = Document::load_mem(&bytes).map_err(|e| format!("Document illisible ({e})."))?;
    if prev.is_encrypted() || prev.trailer.has(b"Encrypt") {
        return Err("Les documents protégés par mot de passe ne peuvent pas encore être signés.".into());
    }
    let signer = match &req.cert {
        Some(c) => Some(crypto::load(&c.path, &c.password)?),
        None => None,
    };
    if let Some(s) = &signer {
        let info = crypto::cert_info(&s.chain[0]);
        if info.expired {
            return Err(format!("Le certificat de {} a expiré.", info.name));
        }
    }
    job.check()?;

    let pages: Vec<ObjectId> = prev.get_pages().values().copied().collect();
    let page_id = *pages.get(req.page as usize).ok_or("Page introuvable.")?;
    let geom = page_geom(&prev, prev.get_dictionary(page_id).map_err(|e| e.to_string())?);
    let field_id = req.field.as_ref().and_then(|f| find_field(&prev, f));
    let root_id = prev.trailer.get(b"Root").and_then(|r| r.as_reference()).map_err(|_| "catalogue introuvable")?;
    let date = now_pdf_date();
    let signer_name = signer.as_ref().map(|s| crypto::cert_info(&s.chain[0]).name).unwrap_or_else(|| req.author.clone());

    let mut inc = IncrementalDocument::create_from(bytes, prev);
    let (ap_id, user) = appearance::build(&mut inc.new_document, &req.visual, &geom, &req.rect)?;
    let rect_obj = vec![Object::Real(user.x), Object::Real(user.y), Object::Real(user.x + user.w), Object::Real(user.y + user.h)];

    if let (Some(s), Some(c)) = (&signer, &req.cert) {
        let size = reserve(s, c.tsa.is_some());
        let mut sig = dictionary! {
            "Type" => "Sig",
            "Filter" => "Adobe.PPKLite",
            "SubFilter" => if c.pades { "ETSI.CAdES.detached" } else { "adbe.pkcs7.detached" },
            "ByteRange" => vec![0.into(), BR_PLACEHOLDER[0].into(), BR_PLACEHOLDER[1].into(), BR_PLACEHOLDER[2].into()],
            "Contents" => Object::String(vec![0u8; size], StringFormat::Hexadecimal),
            "M" => Object::string_literal(date.clone()),
            "Name" => text_object(&signer_name),
            "Prop_Build" => dictionary! { "App" => dictionary! { "Name" => "AuraPDF" } },
        };
        if !c.reason.trim().is_empty() {
            sig.set("Reason", text_object(c.reason.trim()));
        }
        if !c.location.trim().is_empty() {
            sig.set("Location", text_object(c.location.trim()));
        }
        if let Some(p) = c.certify {
            // Certification : seule la première signature d'un document peut certifier.
            let prev = inc.get_prev_documents();
            let signed = prev.objects.values().any(|o| o.as_dict().map(|d| d.has_type(b"Sig") && d.has(b"ByteRange")).unwrap_or(false));
            if signed {
                return Err("Ce document est déjà signé : seule la première signature peut le certifier.".into());
            }
            sig.set(
                "Reference",
                vec![Object::Dictionary(dictionary! {
                    "Type" => "SigRef",
                    "TransformMethod" => "DocMDP",
                    "TransformParams" => dictionary! { "Type" => "TransformParams", "P" => p.clamp(1, 3) as i64, "V" => "1.2" },
                })],
            );
        }
        let sig_id = inc.new_document.add_object(sig);
        if c.certify.is_some() {
            inc.opt_clone_object_to_new_document(root_id).map_err(|e| e.to_string())?;
            inc.new_document.get_dictionary_mut(root_id).map_err(|e| e.to_string())?.set("Perms", dictionary! { "DocMDP" => sig_id });
        }
        match field_id {
            Some(fid) => {
                // Champ existant : la valeur et l'apparence sont posées sur son widget.
                inc.opt_clone_object_to_new_document(fid).map_err(|e| e.to_string())?;
                let d = inc.new_document.get_dictionary_mut(fid).map_err(|e| e.to_string())?;
                d.set("V", sig_id);
                d.set("AP", dictionary! { "N" => ap_id });
                d.set("Rect", rect_obj.clone());
            }
            None => {
                let prev = inc.get_prev_documents();
                let count = prev
                    .catalog()
                    .ok()
                    .and_then(|c| get(prev, c, b"AcroForm"))
                    .and_then(|a| a.as_dict().ok())
                    .and_then(|a| get(prev, a, b"Fields"))
                    .and_then(|f| f.as_array().ok())
                    .map(|f| f.len())
                    .unwrap_or(0);
                let widget = inc.new_document.add_object(dictionary! {
                    "Type" => "Annot", "Subtype" => "Widget", "FT" => "Sig",
                    "T" => text_object(&format!("Signature{}", count + 1)),
                    "Rect" => rect_obj, "P" => page_id, "F" => 132,
                    "AP" => dictionary! { "N" => ap_id }, "V" => sig_id,
                });
                add_to_page(&mut inc, page_id, widget)?;
                // Formulaire : nouveau champ et indicateur « contient des signatures ».
                let prev = inc.get_prev_documents();
                let acro = prev.catalog().ok().and_then(|c| c.get(b"AcroForm").ok()).cloned();
                match acro {
                    Some(Object::Reference(aid)) => {
                        inc.opt_clone_object_to_new_document(aid).map_err(|e| e.to_string())?;
                        let a = inc.new_document.get_dictionary_mut(aid).map_err(|e| e.to_string())?;
                        push_field(a, widget);
                    }
                    other => {
                        let mut a = other.and_then(|o| o.as_dict().ok().cloned()).unwrap_or_default();
                        push_field(&mut a, widget);
                        inc.opt_clone_object_to_new_document(root_id).map_err(|e| e.to_string())?;
                        inc.new_document.get_dictionary_mut(root_id).map_err(|e| e.to_string())?.set("AcroForm", a);
                    }
                }
            }
        }
    } else {
        // Signature manuscrite non certifiée : annotation verrouillée, imprimable.
        let nm = format!("aura-sig-{}", chrono::Utc::now().timestamp_millis());
        let stamp = inc.new_document.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Stamp", "Name" => "AuraSignature",
            "Rect" => rect_obj, "P" => page_id, "F" => 4 | 128 | 512,
            "AP" => dictionary! { "N" => ap_id }, "NM" => text_object(&nm),
            "T" => text_object(&signer_name), "Contents" => text_object("Signature manuscrite"),
            "M" => Object::string_literal(date.clone()), "CreationDate" => Object::string_literal(date),
        });
        add_to_page(&mut inc, page_id, stamp)?;
    }
    job.check()?;

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| format!("écriture du PDF : {e}"))?;
    if let (Some(s), Some(c)) = (&signer, &req.cert) {
        job.progress(1, 0, 1, "");
        let (cs, ce) = contents_span(&out).ok_or("emplacement de la signature introuvable")?;
        let br = [0usize, cs, ce, out.len() - ce];
        patch_byte_range(&mut out, br)?;
        let digest = crypto::Hash::Sha256.digest(&[&out[..cs], &out[ce..]]);
        job.check()?;
        if c.tsa.is_some() {
            job.progress(2, 0, 1, "");
        }
        let container = cms::build(s, &digest, c.tsa.as_deref(), c.pades)?;
        job.progress(3, 0, 1, "");
        let hex: Vec<u8> = container.iter().flat_map(|b| format!("{b:02X}").into_bytes()).collect();
        if hex.len() > ce - cs - 2 {
            return Err("Signature trop volumineuse pour l’emplacement réservé.".into());
        }
        out[cs + 1..cs + 1 + hex.len()].copy_from_slice(&hex);
    } else {
        job.progress(1, 0, 1, "");
    }
    crate::edit::write_atomic(path, &out).map_err(|e| e.message)?;
    let pages = Document::load_mem(&out).map(|d| d.get_pages().len() as u32).unwrap_or(0);
    Ok(Output { path: path.to_string_lossy().into(), size: out.len() as u64, pages })
}

fn push_field(acro: &mut lopdf::Dictionary, widget: ObjectId) {
    let mut fields = acro.get(b"Fields").ok().and_then(|f| f.as_array().ok()).cloned().unwrap_or_default();
    fields.push(Object::Reference(widget));
    acro.set("Fields", fields);
    acro.set("SigFlags", 3);
}

fn add_to_page(inc: &mut IncrementalDocument, page_id: ObjectId, annot: ObjectId) -> Result<(), String> {
    let prev = inc.get_prev_documents();
    let mut list = prev.get_dictionary(page_id).ok().and_then(|d| get(prev, d, b"Annots")).and_then(|a| a.as_array().ok()).cloned().unwrap_or_default();
    list.push(Object::Reference(annot));
    inc.opt_clone_object_to_new_document(page_id).map_err(|e| e.to_string())?;
    inc.new_document.get_dictionary_mut(page_id).map_err(|e| e.to_string())?.set("Annots", list);
    Ok(())
}

/// Position de la chaîne hexadécimale réservée (de `<` à `>` inclus).
fn contents_span(out: &[u8]) -> Option<(usize, usize)> {
    let zeros = [b'0'; 128];
    let mid = out.windows(zeros.len()).rposition(|w| w == zeros)?;
    let start = out[..mid].iter().rposition(|&b| b == b'<')?;
    let end = mid + out[mid..].iter().position(|&b| b == b'>')? + 1;
    Some((start, end))
}

/// Remplace les valeurs provisoires de /ByteRange (même longueur, complétée d'espaces).
fn patch_byte_range(out: &mut [u8], br: [usize; 4]) -> Result<(), String> {
    let needle = BR_PLACEHOLDER[0].to_string().into_bytes();
    let pos = out.windows(needle.len()).rposition(|w| w == needle.as_slice()).ok_or("/ByteRange introuvable")?;
    let open = out[..pos].iter().rposition(|&b| b == b'[').ok_or("/ByteRange illisible")?;
    let close = pos + out[pos..].iter().position(|&b| b == b']').ok_or("/ByteRange illisible")?;
    let mut text = format!("[{} {} {} {}", br[0], br[1], br[2], br[3]).into_bytes();
    let room = close - open;
    if text.len() > room {
        return Err("/ByteRange trop long".into());
    }
    text.resize(room, b' ');
    out[open..close].copy_from_slice(&text);
    Ok(())
}

/// Chemin par défaut d'un certificat personnel créé par Aura.
#[allow(dead_code)]
pub fn default_cert_path(name: &str) -> PathBuf {
    let dir = dirs::document_dir().unwrap_or_else(std::env::temp_dir);
    crate::tools::free_path(&dir, &format!("Certificat {name}")).with_extension("p12")
}

#[cfg(test)]
mod tests;
