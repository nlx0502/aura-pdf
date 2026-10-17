//! Certificats du système, y compris ceux d'une carte à puce ou d'un jeton :
//! magasin « Personnel » de Windows (CNG) ou trousseau de macOS (Security). La
//! clé privée reste dans son fournisseur et c'est lui qui signe ; le système
//! demande le code PIN ou le mot de passe lui-même si nécessaire.

use der::Decode;
use spki::ObjectIdentifier;
use x509_cert::Certificate;

use super::crypto::{cert_info, CertInfo};

/// Préfixe des chemins de certificat désignant le magasin Windows.
pub const PREFIX: &str = "store:";

const OID_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1");
const OID_EC: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.2.1");
const OID_P256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.3.1.7");
const OID_P384: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.132.0.34");

/// Certificat du magasin muni d'une clé privée.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreCert {
    /// Chemin à transmettre (« store: » + empreinte SHA-1).
    pub id: String,
    #[serde(flatten)]
    pub info: CertInfo,
    /// Fournisseur de la clé (« Microsoft Smart Card Key Storage Provider »…).
    pub provider: String,
}

/// Type de clé, d'après le certificat.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KeyKind {
    Rsa,
    P256,
    P384,
}

pub fn key_kind(cert: &Certificate) -> Result<KeyKind, String> {
    let alg = &cert.tbs_certificate.subject_public_key_info.algorithm;
    if alg.oid == OID_RSA {
        return Ok(KeyKind::Rsa);
    }
    if alg.oid == OID_EC {
        let curve = alg.parameters.as_ref().and_then(|p| p.decode_as::<ObjectIdentifier>().ok());
        return match curve {
            Some(c) if c == OID_P256 => Ok(KeyKind::P256),
            Some(c) if c == OID_P384 => Ok(KeyKind::P384),
            _ => Err("courbe elliptique non prise en charge (P-256 ou P-384 attendues)".into()),
        };
    }
    Err("type de clé non pris en charge (RSA, ECDSA P-256 ou P-384 attendus)".into())
}

/// Clé privée d'un certificat du magasin.
pub struct StoreKey {
    pub thumbprint: Vec<u8>,
    pub kind: KeyKind,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    (s.len() % 2 == 0).then_some(())?;
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// Certificats personnels utilisables pour signer.
pub fn list() -> Result<Vec<StoreCert>, String> {
    imp::list()
}

/// Ouvre un certificat du magasin : chaîne (signataire en premier) et clé.
pub fn load(id: &str) -> Result<(StoreKey, Vec<Certificate>), String> {
    let thumb = unhex(id.trim_start_matches(PREFIX)).ok_or("Référence de certificat invalide.")?;
    let chain = imp::chain(&thumb)?;
    let kind = key_kind(&chain[0])?;
    Ok((StoreKey { thumbprint: thumb, kind }, chain))
}

impl StoreKey {
    /// Signe l'empreinte `digest` (SHA-256 ou SHA-384 selon la clé) ; renvoie
    /// la signature au format attendu par CMS (PKCS #1 v1.5 ou ECDSA DER).
    pub fn sign_digest(&self, digest: &[u8]) -> Result<Vec<u8>, String> {
        self.encode(imp::sign(&self.thumbprint, digest, self.kind == KeyKind::Rsa)?)
    }

    /// ECDSA : CNG renvoie r‖s, CMS attend une structure DER (le trousseau de
    /// macOS la renvoie déjà ainsi).
    fn encode(&self, raw: Vec<u8>) -> Result<Vec<u8>, String> {
        if self.kind != KeyKind::Rsa && raw.first() == Some(&0x30) && raw.len() != if self.kind == KeyKind::P256 { 64 } else { 96 } {
            return Ok(raw);
        }
        Ok(match self.kind {
            KeyKind::Rsa => raw,
            KeyKind::P256 => p256::ecdsa::Signature::from_slice(&raw).map_err(|e| e.to_string())?.to_der().as_bytes().to_vec(),
            KeyKind::P384 => p384::ecdsa::Signature::from_slice(&raw).map_err(|e| e.to_string())?.to_der().as_bytes().to_vec(),
        })
    }
}

/// Usage de clé compatible avec la signature (absence d'extension : tout usage).
#[allow(dead_code)]
fn signing_usage(cert: &Certificate) -> bool {
    use x509_cert::ext::pkix::KeyUsage;
    match cert.tbs_certificate.get::<KeyUsage>() {
        Ok(Some((_, ku))) => ku.digital_signature() || ku.non_repudiation(),
        _ => true,
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;

    use windows::core::w;
    use windows::core::BOOL;
    use windows::Win32::Security::Cryptography::*;

    use super::*;

    const ENCODING: CERT_QUERY_ENCODING_TYPE = CERT_QUERY_ENCODING_TYPE(X509_ASN_ENCODING.0 | PKCS_7_ASN_ENCODING.0);

    struct Store(HCERTSTORE);
    impl Drop for Store {
        fn drop(&mut self) {
            unsafe {
                let _ = CertCloseStore(Some(self.0), 0);
            }
        }
    }

    fn open() -> Result<Store, String> {
        unsafe { CertOpenSystemStoreW(None, w!("MY")) }.map(Store).map_err(|e| format!("Magasin de certificats inaccessible ({e})."))
    }

    fn der_of(ctx: *const CERT_CONTEXT) -> Vec<u8> {
        unsafe { std::slice::from_raw_parts((*ctx).pbCertEncoded, (*ctx).cbCertEncoded as usize).to_vec() }
    }

    fn property(ctx: *const CERT_CONTEXT, id: u32) -> Option<Vec<u8>> {
        let mut n = 0u32;
        unsafe { CertGetCertificateContextProperty(ctx, id, None, &mut n) }.ok()?;
        let mut buf = vec![0u8; n as usize];
        unsafe { CertGetCertificateContextProperty(ctx, id, Some(buf.as_mut_ptr() as *mut c_void), &mut n) }.ok()?;
        buf.truncate(n as usize);
        Some(buf)
    }

    /// Nom du fournisseur de la clé privée (absent : pas de clé privée).
    fn provider(ctx: *const CERT_CONTEXT) -> Option<String> {
        let buf = property(ctx, CERT_KEY_PROV_INFO_PROP_ID)?;
        let info = unsafe { &*(buf.as_ptr() as *const CRYPT_KEY_PROV_INFO) };
        Some(unsafe { info.pwszProvName.to_string() }.unwrap_or_default())
    }

    fn find(store: &Store, thumb: &[u8]) -> Result<*mut CERT_CONTEXT, String> {
        let blob = CRYPT_INTEGER_BLOB { cbData: thumb.len() as u32, pbData: thumb.as_ptr() as *mut u8 };
        let ctx = unsafe { CertFindCertificateInStore(store.0, ENCODING, 0, CERT_FIND_HASH, Some(&blob as *const _ as *const c_void), None) };
        if ctx.is_null() {
            Err("Ce certificat n’est plus dans le magasin de certificats de Windows.".into())
        } else {
            Ok(ctx)
        }
    }

    pub fn list() -> Result<Vec<StoreCert>, String> {
        let store = open()?;
        let mut out = Vec::new();
        let mut ctx: *mut CERT_CONTEXT = std::ptr::null_mut();
        loop {
            ctx = unsafe { CertEnumCertificatesInStore(store.0, Some(ctx)) };
            if ctx.is_null() {
                break;
            }
            let Some(provider) = provider(ctx) else { continue };
            let Ok(cert) = Certificate::from_der(&der_of(ctx)) else { continue };
            if key_kind(&cert).is_err() || !signing_usage(&cert) {
                continue;
            }
            let Some(thumb) = property(ctx, CERT_HASH_PROP_ID) else { continue };
            out.push(StoreCert { id: format!("{PREFIX}{}", hex(&thumb)), info: cert_info(&cert), provider });
        }
        out.sort_by(|a, b| (a.info.expired, &a.info.name).cmp(&(b.info.expired, &b.info.name)));
        Ok(out)
    }

    pub fn chain(thumb: &[u8]) -> Result<Vec<Certificate>, String> {
        let store = open()?;
        let ctx = find(&store, thumb)?;
        let leaf = Certificate::from_der(&der_of(ctx)).map_err(|e| e.to_string())?;
        let mut certs = vec![leaf];
        // Autorités intermédiaires connues de Windows (la racine est omise).
        let para = CERT_CHAIN_PARA { cbSize: std::mem::size_of::<CERT_CHAIN_PARA>() as u32, ..Default::default() };
        let mut chain: *mut CERT_CHAIN_CONTEXT = std::ptr::null_mut();
        if unsafe { CertGetCertificateChain(None, ctx, None, None, &para, 0, None, &mut chain) }.is_ok() && !chain.is_null() {
            unsafe {
                let c = &*chain;
                if c.cChain > 0 {
                    let simple = &**c.rgpChain;
                    let n = simple.cElement as usize;
                    for i in 1..n.saturating_sub(1) {
                        let el = &**simple.rgpElement.add(i);
                        if let Ok(x) = Certificate::from_der(&der_of(el.pCertContext)) {
                            certs.push(x);
                        }
                    }
                }
                CertFreeCertificateChain(chain);
            }
        }
        unsafe {
            let _ = CertFreeCertificateContext(Some(ctx));
        }
        Ok(certs)
    }

    pub fn sign(thumb: &[u8], digest: &[u8], rsa: bool) -> Result<Vec<u8>, String> {
        let store = open()?;
        let ctx = find(&store, thumb)?;
        sign_ctx(ctx, digest, rsa)
    }

    /// Signe avec la clé du certificat `ctx` (le contexte est libéré).
    fn sign_ctx(ctx: *mut CERT_CONTEXT, digest: &[u8], rsa: bool) -> Result<Vec<u8>, String> {
        let mut key = HCRYPTPROV_OR_NCRYPT_KEY_HANDLE::default();
        let mut spec = CERT_KEY_SPEC::default();
        let mut free = BOOL::default();
        let acquired = unsafe { CryptAcquireCertificatePrivateKey(ctx, CRYPT_ACQUIRE_ONLY_NCRYPT_KEY_FLAG | CRYPT_ACQUIRE_COMPARE_KEY_FLAG, None, &mut key, Some(&mut spec), Some(&mut free)) };
        unsafe {
            let _ = CertFreeCertificateContext(Some(ctx));
        }
        acquired.map_err(|e| format!("Clé privée inaccessible ({}).", e.message()))?;
        let handle = NCRYPT_KEY_HANDLE(key.0);
        let result = sign_key(handle, digest, rsa);
        if free.as_bool() {
            unsafe {
                let _ = NCryptFreeObject(NCRYPT_HANDLE(handle.0));
            }
        }
        result.map_err(|e| if e.to_lowercase().contains("annul") || e.to_lowercase().contains("cancel") { "Signature annulée.".to_string() } else { format!("La carte ou le magasin a refusé la signature ({e}).") })
    }

    /// Signature d'une empreinte par une clé CNG (PKCS #1 v1.5 pour RSA).
    fn sign_key(handle: NCRYPT_KEY_HANDLE, digest: &[u8], rsa: bool) -> Result<Vec<u8>, String> {
        let pad = BCRYPT_PKCS1_PADDING_INFO { pszAlgId: if digest.len() == 48 { BCRYPT_SHA384_ALGORITHM } else { BCRYPT_SHA256_ALGORITHM } };
        let (info, flags) = if rsa { (Some(&pad as *const _ as *const c_void), NCRYPT_FLAGS(BCRYPT_PAD_PKCS1.0)) } else { (None, NCRYPT_FLAGS(0)) };
        let mut n = 0u32;
        unsafe { NCryptSignHash(handle, info, digest, None, &mut n, flags) }.map_err(|e| e.message())?;
        let mut sig = vec![0u8; n as usize];
        unsafe { NCryptSignHash(handle, info, digest, Some(&mut sig), &mut n, flags) }.map_err(|e| e.message())?;
        sig.truncate(n as usize);
        Ok(sig)
    }

    /// Tests : PKCS #12 importé en mémoire, clé non persistée (le magasin de
    /// l'utilisateur n'est pas touché). Renvoie le certificat et la signature.
    #[cfg(test)]
    pub fn sign_with_pfx(pfx: &[u8], password: &str, digest: &[u8]) -> Result<(Certificate, Vec<u8>), String> {
        let blob = CRYPT_INTEGER_BLOB { cbData: pfx.len() as u32, pbData: pfx.as_ptr() as *mut u8 };
        let pw: Vec<u16> = password.encode_utf16().chain([0]).collect();
        let store = unsafe { PFXImportCertStore(&blob, windows::core::PCWSTR(pw.as_ptr()), PKCS12_NO_PERSIST_KEY | PKCS12_ALWAYS_CNG_KSP) }.map(Store).map_err(|e| e.to_string())?;
        let mut ctx: *mut CERT_CONTEXT = std::ptr::null_mut();
        loop {
            ctx = unsafe { CertEnumCertificatesInStore(store.0, Some(ctx)) };
            if ctx.is_null() {
                return Err("aucun certificat avec clé".into());
            }
            if property(ctx, CERT_NCRYPT_KEY_HANDLE_PROP_ID).is_some() {
                break;
            }
        }
        // Clé non persistée : son descripteur est porté par le certificat.
        let h = property(ctx, CERT_NCRYPT_KEY_HANDLE_PROP_ID).unwrap();
        let handle = NCRYPT_KEY_HANDLE(usize::from_ne_bytes(h[..std::mem::size_of::<usize>()].try_into().unwrap()));
        let cert = Certificate::from_der(&der_of(ctx)).map_err(|e| e.to_string())?;
        let kind = key_kind(&cert)?;
        let raw = sign_key(handle, digest, kind == KeyKind::Rsa)?;
        unsafe {
            let _ = CertFreeCertificateContext(Some(ctx));
        }
        let key = StoreKey { thumbprint: Vec::new(), kind };
        Ok((cert, key.encode(raw)?))
    }
}

/// Trousseau de macOS : identités (certificat + clé privée), dont celles des
/// cartes à puce exposées par CryptoTokenKit.
#[cfg(target_os = "macos")]
mod imp {
    use security_framework::identity::SecIdentity;
    use security_framework::item::{ItemClass, ItemSearchOptions, Limit, Reference, SearchResult};
    use security_framework::key::Algorithm;
    use sha2::Digest;

    use super::*;

    /// Identifiant stable d'un certificat : empreinte SHA-256 de son DER.
    fn id_of(der: &[u8]) -> Vec<u8> {
        sha2::Sha256::digest(der).to_vec()
    }

    fn search(class: ItemClass) -> Result<Vec<Reference>, String> {
        match ItemSearchOptions::new().class(class).load_refs(true).limit(Limit::All).search() {
            Ok(r) => Ok(r.into_iter().filter_map(|x| if let SearchResult::Ref(r) = x { Some(r) } else { None }).collect()),
            // errSecItemNotFound : trousseau sans identité.
            Err(e) if e.code() == -25300 => Ok(Vec::new()),
            Err(e) => Err(format!("Trousseau inaccessible ({e}).")),
        }
    }

    fn identities() -> Result<Vec<(SecIdentity, Vec<u8>)>, String> {
        Ok(search(ItemClass::identity())?
            .into_iter()
            .filter_map(|r| match r {
                Reference::Identity(i) => {
                    let der = i.certificate().ok()?.to_der();
                    Some((i, der))
                }
                _ => None,
            })
            .collect())
    }

    fn find(thumb: &[u8]) -> Result<(SecIdentity, Vec<u8>), String> {
        identities()?.into_iter().find(|(_, der)| id_of(der) == thumb).ok_or_else(|| "Ce certificat n’est plus dans le trousseau.".to_string())
    }

    pub fn list() -> Result<Vec<StoreCert>, String> {
        let mut out = Vec::new();
        for (_, der) in identities()? {
            let Ok(cert) = Certificate::from_der(&der) else { continue };
            if key_kind(&cert).is_err() || !signing_usage(&cert) {
                continue;
            }
            out.push(StoreCert { id: format!("{PREFIX}{}", hex(&id_of(&der))), info: cert_info(&cert), provider: "Trousseau macOS".into() });
        }
        out.sort_by(|a, b| (a.info.expired, &a.info.name).cmp(&(b.info.expired, &b.info.name)));
        out.dedup_by(|a, b| a.id == b.id);
        Ok(out)
    }

    pub fn chain(thumb: &[u8]) -> Result<Vec<Certificate>, String> {
        let (_, der) = find(thumb)?;
        let leaf = Certificate::from_der(&der).map_err(|e| e.to_string())?;
        // Autorités intermédiaires présentes dans le trousseau (racine omise).
        let pool: Vec<Certificate> = search(ItemClass::certificate())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|r| if let Reference::Certificate(c) = r { Certificate::from_der(&c.to_der()).ok() } else { None })
            .collect();
        let mut certs = vec![leaf];
        for _ in 0..6 {
            let cur = certs.last().unwrap();
            if cur.tbs_certificate.subject == cur.tbs_certificate.issuer {
                break;
            }
            match pool.iter().find(|c| c.tbs_certificate.subject == cur.tbs_certificate.issuer && c.tbs_certificate.subject != c.tbs_certificate.issuer) {
                Some(c) => certs.push(c.clone()),
                None => break,
            }
        }
        Ok(certs)
    }

    pub fn sign(thumb: &[u8], digest: &[u8], rsa: bool) -> Result<Vec<u8>, String> {
        let (identity, _) = find(thumb)?;
        let key = identity.private_key().map_err(|e| format!("Clé privée inaccessible ({e})."))?;
        let alg = match (rsa, digest.len()) {
            (true, 48) => Algorithm::RSASignatureDigestPKCS1v15SHA384,
            (true, _) => Algorithm::RSASignatureDigestPKCS1v15SHA256,
            (false, 48) => Algorithm::ECDSASignatureDigestX962SHA384,
            (false, _) => Algorithm::ECDSASignatureDigestX962SHA256,
        };
        key.create_signature(alg, digest).map_err(|e| {
            let m = e.to_string();
            if m.contains("-128") || m.to_lowercase().contains("cancel") {
                "Signature annulée.".to_string()
            } else {
                format!("Le trousseau a refusé la signature ({m}).")
            }
        })
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use super::*;
    pub fn list() -> Result<Vec<StoreCert>, String> {
        Ok(Vec::new())
    }
    pub fn chain(_: &[u8]) -> Result<Vec<Certificate>, String> {
        Err("Magasin de certificats indisponible sur ce système.".into())
    }
    pub fn sign(_: &[u8], _: &[u8], _: bool) -> Result<Vec<u8>, String> {
        Err("Magasin de certificats indisponible sur ce système.".into())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::sign::crypto::{create_self_signed, spki_der, verify, Hash};

    /// Signature par CNG (comme pour un certificat du magasin ou une carte),
    /// vérifiée par le code de vérification d'Aura.
    #[test]
    fn signature_par_cng() {
        let out = std::env::temp_dir().join(format!("aura-cng-{}.p12", std::process::id()));
        create_self_signed("Carte Test", "", "Aura", "secret", &out).unwrap();
        let pfx = std::fs::read(&out).unwrap();
        let _ = std::fs::remove_file(&out);
        let msg = b"contenu signe";
        let digest = Hash::Sha256.digest(&[msg]);
        let (cert, sig) = imp::sign_with_pfx(&pfx, "secret", &digest).unwrap();
        assert_eq!(key_kind(&cert).unwrap(), KeyKind::P256);
        verify(&spki_der(&cert), &ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2"), Some(Hash::Sha256), msg, &sig).unwrap();
        // RSA, si un PKCS #12 RSA est fourni (AURA_P12_RSA, mot de passe « secret »).
        if let Ok(p) = std::env::var("AURA_P12_RSA") {
            let (cert, sig) = imp::sign_with_pfx(&std::fs::read(p).unwrap(), "secret", &digest).unwrap();
            assert_eq!(key_kind(&cert).unwrap(), KeyKind::Rsa);
            verify(&spki_der(&cert), &ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11"), Some(Hash::Sha256), msg, &sig).unwrap();
        }
    }

    #[test]
    fn liste_du_magasin() {
        // Lecture seule : ne doit pas échouer, même sans certificat personnel.
        let l = list().unwrap();
        for c in &l {
            assert!(c.id.starts_with(PREFIX));
        }
    }
}
