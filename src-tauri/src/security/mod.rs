//! Module Sécurité : mot de passe, chiffrement AES-128 / AES-256, permissions,
//! métadonnées, archivage PDF/A.
//!
//! Le document est relu (déchiffré avec le mot de passe en mémoire du moteur),
//! modifié puis réécrit en entier : chiffré avec un état neuf (nouvelle clé de
//! fichier) si une protection est demandée, en clair sinon.

pub mod pdfa;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes128CryptFilter, Aes256CryptFilter, CryptFilter};
use lopdf::encryption::{EncryptionState, EncryptionVersion, Permissions};
use lopdf::{Document, Object, StringFormat};
use serde::{Deserialize, Serialize};

use crate::annot::pdf::{text, text_object};
use crate::tools::{Job, Output};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Perms {
    pub print: bool,
    pub copy: bool,
    pub edit: bool,
    pub annotate: bool,
    pub forms: bool,
    /// Insérer, faire pivoter, supprimer ou extraire des pages.
    pub extract: bool,
}

impl Perms {
    pub const ALL: Perms = Perms { print: true, copy: true, edit: true, annotate: true, forms: true, extract: true };

    fn from_bits(p: Permissions) -> Perms {
        Perms {
            print: p.contains(Permissions::PRINTABLE),
            copy: p.contains(Permissions::COPYABLE),
            edit: p.contains(Permissions::MODIFIABLE),
            annotate: p.contains(Permissions::ANNOTABLE),
            forms: p.contains(Permissions::FILLABLE) || p.contains(Permissions::ANNOTABLE),
            extract: p.contains(Permissions::ASSEMBLABLE),
        }
    }

    fn to_bits(self) -> Permissions {
        // L'accès pour les technologies d'assistance reste toujours autorisé.
        let mut p = Permissions::COPYABLE_FOR_ACCESSIBILITY;
        if self.print {
            p |= Permissions::PRINTABLE | Permissions::PRINTABLE_IN_HIGH_QUALITY;
        }
        if self.copy {
            p |= Permissions::COPYABLE;
        }
        if self.edit {
            p |= Permissions::MODIFIABLE;
        }
        if self.annotate {
            p |= Permissions::ANNOTABLE;
        }
        if self.forms {
            p |= Permissions::FILLABLE;
        }
        if self.extract {
            p |= Permissions::ASSEMBLABLE;
        }
        p
    }

    pub fn count(self) -> usize {
        [self.print, self.copy, self.edit, self.annotate, self.forms, self.extract].iter().filter(|b| **b).count()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityInfo {
    /// none, rc4-40, rc4-128, aes-128, aes-256.
    pub algorithm: String,
    /// Un mot de passe est demandé à l'ouverture.
    pub open_password: bool,
    pub encrypt_metadata: bool,
    pub permissions: Perms,
    pub metadata: Meta,
    /// Niveau PDF/A déclaré (« 2b »…).
    pub pdfa: Option<String>,
    /// Ouvert avec le mot de passe propriétaire (ou non chiffré) : tous les droits.
    pub owner: bool,
}

impl SecurityInfo {
    /// Droits effectifs de l'utilisateur.
    pub fn effective(&self) -> Perms {
        if self.owner { Perms::ALL } else { self.permissions }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecuritySpec {
    /// Mot de passe à l'ouverture.
    pub protect: bool,
    #[serde(default)]
    pub user_password: String,
    /// Mot de passe propriétaire (permissions) ; tiré au hasard s'il est vide.
    #[serde(default)]
    pub owner_password: String,
    pub aes256: bool,
    pub encrypt_metadata: bool,
    pub permissions: Perms,
    pub metadata: Meta,
    /// « 1b », « 2b » ou « 3b ».
    #[serde(default)]
    pub pdfa: Option<String>,
}

pub fn load(path: &Path, password: Option<&str>) -> Result<Document, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Fichier illisible ({e})."))?;
    match password {
        Some(p) => Document::load_mem_with_options(&bytes, lopdf::LoadOptions::with_password(p)),
        None => Document::load_mem(&bytes),
    }
    .map_err(|e| format!("Document illisible ({e})."))
}

fn info_dict(doc: &Document) -> Option<&lopdf::Dictionary> {
    doc.trailer.get(b"Info").ok().and_then(|o| doc.dereference(o).ok()).and_then(|(_, o)| o.as_dict().ok())
}

pub fn inspect(path: &Path, password: Option<&str>) -> Result<SecurityInfo, String> {
    let doc = load(path, password)?;
    let state = doc.encryption_state.as_ref();
    let algorithm = match state {
        None => "none".to_string(),
        Some(s) => match (s.version(), s.revision()) {
            (5, _) => "aes-256".into(),
            (4, _) => {
                let aes = s.crypt_filters().values().any(|f| f.method() == b"AESV2");
                if aes { "aes-128".into() } else { "rc4-128".into() }
            }
            (1, _) => "rc4-40".into(),
            _ => "rc4-128".into(),
        },
    };
    let owner = match state {
        None => true,
        Some(s) => is_owner(&doc, s, password.unwrap_or("")),
    };
    // Mot de passe d'ouverture : le fichier ne s'ouvre pas avec un mot de passe vide.
    let open_password = state.is_some() && load(path, Some("")).is_err();
    let meta = info_dict(&doc)
        .map(|d| Meta {
            title: d.get(b"Title").map(text).unwrap_or_default(),
            author: d.get(b"Author").map(text).unwrap_or_default(),
            subject: d.get(b"Subject").map(text).unwrap_or_default(),
            keywords: d.get(b"Keywords").map(text).unwrap_or_default(),
        })
        .unwrap_or_default();
    Ok(SecurityInfo {
        algorithm,
        open_password,
        encrypt_metadata: state.map(|s| s.encrypt_metadata()).unwrap_or(true),
        permissions: state.map(|s| Perms::from_bits(s.permissions())).unwrap_or(Perms::ALL),
        metadata: meta,
        pdfa: pdfa::declared(&doc),
        owner,
    })
}

/// Le mot de passe est-il celui du propriétaire ? (Le dictionnaire /Encrypt,
/// retiré au chargement, est reconstitué à partir de l'état de chiffrement.)
fn is_owner(doc: &Document, state: &EncryptionState, password: &str) -> bool {
    let Ok(dict) = state.encode() else { return false };
    let mut probe = Document::with_version(doc.version.clone());
    probe.trailer = doc.trailer.clone();
    let id = probe.add_object(dict);
    probe.trailer.set("Encrypt", id);
    probe.authenticate_owner_password(password).is_ok()
}

/// Applique la sécurité demandée et réécrit `path`.
pub fn apply(path: &Path, password: Option<&str>, spec: &SecuritySpec, job: &Job) -> Result<Output, String> {
    job.progress(0, 0, 4, "");
    let mut doc = load(path, password)?;
    // Le document est désormais en clair en mémoire ; on repart d'un état neuf.
    doc.encryption_state = None;
    doc.trailer.remove(b"Encrypt");
    strip_xref_streams(&mut doc);
    let restrict = spec.permissions != Perms::ALL;
    let encrypt = spec.protect || restrict;
    if spec.pdfa.is_some() && encrypt {
        return Err("Un document PDF/A ne peut pas être chiffré : désactivez la protection ou l’archivage.".into());
    }
    if spec.protect && spec.user_password.is_empty() {
        return Err("Saisissez le mot de passe d’ouverture.".into());
    }
    job.check()?;

    // Métadonnées (dictionnaire Info et XMP).
    job.progress(1, 1, 4, "");
    set_metadata(&mut doc, &spec.metadata);
    if let Some(level) = &spec.pdfa {
        pdfa::convert(&mut doc, level, &spec.metadata)?;
    } else if doc.catalog().map(|c| c.has(b"Metadata")).unwrap_or(false) {
        pdfa::write_xmp(&mut doc, &spec.metadata, None);
    }
    ensure_id(&mut doc);
    job.check()?;

    // Chiffrement.
    job.progress(2, 2, 4, "");
    if encrypt {
        let owner = if spec.owner_password.is_empty() { random_password() } else { spec.owner_password.clone() };
        let user = if spec.protect { spec.user_password.clone() } else { String::new() };
        let perms = spec.permissions.to_bits();
        let state = if spec.aes256 {
            let mut key = [0u8; 32];
            rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut key);
            let mut cf: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = BTreeMap::new();
            cf.insert(b"StdCF".to_vec(), Arc::new(Aes256CryptFilter));
            EncryptionState::try_from(EncryptionVersion::V5 {
                encrypt_metadata: spec.encrypt_metadata,
                crypt_filters: cf,
                file_encryption_key: &key,
                stream_filter: b"StdCF".to_vec(),
                string_filter: b"StdCF".to_vec(),
                owner_password: &owner,
                user_password: &user,
                permissions: perms,
            })
        } else {
            let mut cf: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = BTreeMap::new();
            cf.insert(b"StdCF".to_vec(), Arc::new(Aes128CryptFilter));
            EncryptionState::try_from(EncryptionVersion::V4 {
                document: &doc,
                encrypt_metadata: spec.encrypt_metadata,
                crypt_filters: cf,
                stream_filter: b"StdCF".to_vec(),
                string_filter: b"StdCF".to_vec(),
                owner_password: &owner,
                user_password: &user,
                permissions: perms,
            })
        }
        .map_err(|e| format!("Chiffrement impossible ({e})."))?;
        // Les flux sont écrits compressés puis chiffrés (pas de flux d'objets).
        doc.compress();
        doc.encrypt(&state).map_err(|e| format!("Chiffrement impossible ({e})."))?;
        complete_encrypt_dict(&mut doc);
        if spec.aes256 && doc.version.as_str() < "1.7" {
            doc.version = "1.7".into();
        }
    } else {
        doc.compress();
    }
    job.check()?;
    job.progress(3, 3, 4, "");
    super::tools::write(&mut doc, path, false)
}

fn set_metadata(doc: &mut Document, m: &Meta) {
    let mut info = info_dict(doc).cloned().unwrap_or_default();
    for (k, v) in [("Title", &m.title), ("Author", &m.author), ("Subject", &m.subject), ("Keywords", &m.keywords)] {
        if v.trim().is_empty() {
            info.remove(k.as_bytes());
        } else {
            info.set(k, text_object(v.trim()));
        }
    }
    info.set("Producer", text_object("Aura PDF"));
    info.set("ModDate", Object::string_literal(crate::annot::pdf::now_pdf_date()));
    let id = doc.add_object(info);
    doc.trailer.set("Info", id);
}

/// Identifiant de fichier (/ID), requis par le chiffrement et par PDF/A.
fn ensure_id(doc: &mut Document) {
    if doc.trailer.has(b"ID") {
        return;
    }
    let mut id = [0u8; 16];
    rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut id);
    let s = Object::String(id.to_vec(), StringFormat::Hexadecimal);
    doc.trailer.set("ID", vec![s.clone(), s]);
}

fn random_password() -> String {
    let mut b = [0u8; 24];
    rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Flux d'objets et de références croisées hérités : leur contenu a été extrait
/// au chargement et ils seraient réécrits en double (l'écriture est classique).
pub(crate) fn strip_xref_streams(doc: &mut Document) {
    let dead: Vec<_> = doc
        .objects
        .iter()
        .filter(|(_, o)| o.as_stream().map(|s| s.dict.has_type(b"ObjStm") || s.dict.has_type(b"XRef")).unwrap_or(false))
        .map(|(id, _)| *id)
        .collect();
    for id in dead {
        doc.objects.remove(&id);
    }
}

/// Droits effectifs sur un fichier (lecture rapide : seuls les fichiers qui
/// mentionnent /Encrypt sont analysés).
pub fn rights(path: &Path, password: Option<&str>) -> Result<Perms, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Fichier illisible ({e})."))?;
    if !bytes.windows(8).any(|w| w == b"/Encrypt") {
        return Ok(Perms::ALL);
    }
    inspect(path, password).map(|i| i.effective())
}

/// Complète le dictionnaire /Encrypt écrit par lopdf avec les entrées
/// qu'Acrobat exige : longueur de clé (/Length 128 ou 256 bits) et, pour le
/// filtre StdCF, longueur en octets et événement d'authentification.
pub(crate) fn complete_encrypt_dict(doc: &mut Document) {
    let Some(id) = doc.trailer.get(b"Encrypt").ok().and_then(|o| o.as_reference().ok()) else { return };
    let Ok(d) = doc.get_dictionary_mut(id) else { return };
    let v = d.get(b"V").and_then(|o| o.as_i64()).unwrap_or(0);
    let bytes = match v {
        5 => 32,
        4 => 16,
        _ => return,
    };
    d.set("Length", bytes * 8);
    if let Ok(Object::Dictionary(cf)) = d.get_mut(b"CF") {
        for (_, f) in cf.iter_mut() {
            if let Object::Dictionary(f) = f {
                f.set("Length", bytes);
                f.set("AuthEvent", "DocOpen");
            }
        }
    }
}
