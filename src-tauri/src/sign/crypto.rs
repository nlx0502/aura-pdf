//! Cryptographie de la signature : certificats PKCS #12, signature RSA ou
//! ECDSA, vérification, chaîne de confiance, certificat personnel.

use std::path::Path;

use const_oid::ObjectIdentifier;
use der::{Decode, Encode};
use serde::Serialize;
use sha2::Digest;
use signature::{SignatureEncoding, Signer as _, Verifier as _};
use spki::AlgorithmIdentifierOwned;
use x509_cert::Certificate;

pub const OID_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1");
pub const OID_SHA1_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.5");
pub const OID_SHA256_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11");
pub const OID_SHA384_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.12");
pub const OID_SHA512_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.13");
pub const OID_RSA_PSS: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.10");
pub const OID_EC_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");
pub const OID_EC_SHA384: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.3");
pub const OID_EC_SHA512: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.4");
pub const OID_SHA1: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.14.3.2.26");
pub const OID_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");
pub const OID_SHA384: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.2");
pub const OID_SHA512: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.3");

/// Algorithme d'empreinte d'après son identifiant (ou celui d'une signature).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hash {
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl Hash {
    pub fn from_oid(oid: &ObjectIdentifier) -> Option<Hash> {
        Some(match *oid {
            x if x == OID_SHA1 || x == OID_SHA1_RSA => Hash::Sha1,
            x if x == OID_SHA256 || x == OID_SHA256_RSA || x == OID_EC_SHA256 => Hash::Sha256,
            x if x == OID_SHA384 || x == OID_SHA384_RSA || x == OID_EC_SHA384 => Hash::Sha384,
            x if x == OID_SHA512 || x == OID_SHA512_RSA || x == OID_EC_SHA512 => Hash::Sha512,
            _ => return None,
        })
    }

    pub fn digest(self, parts: &[&[u8]]) -> Vec<u8> {
        fn run<D: Digest>(parts: &[&[u8]]) -> Vec<u8> {
            let mut h = D::new();
            for p in parts {
                h.update(p);
            }
            h.finalize().to_vec()
        }
        match self {
            Hash::Sha1 => run::<sha1::Sha1>(parts),
            Hash::Sha256 => run::<sha2::Sha256>(parts),
            Hash::Sha384 => run::<sha2::Sha384>(parts),
            Hash::Sha512 => run::<sha2::Sha512>(parts),
        }
    }
}

pub fn alg(oid: ObjectIdentifier, null_params: bool) -> AlgorithmIdentifierOwned {
    AlgorithmIdentifierOwned { oid, parameters: null_params.then(|| der::Any::from(der::asn1::Null)) }
}

/// Clé privée de signature.
pub enum SignKey {
    Rsa(rsa::RsaPrivateKey),
    P256(p256::ecdsa::SigningKey),
    P384(p384::ecdsa::SigningKey),
    /// Clé conservée par Windows (magasin personnel, carte à puce).
    Store(super::store::StoreKey),
}

impl SignKey {
    pub fn from_pkcs8(der: &[u8]) -> Result<SignKey, String> {
        use pkcs8_reexport::DecodePrivateKey;
        if let Ok(k) = rsa::RsaPrivateKey::from_pkcs8_der(der) {
            return Ok(SignKey::Rsa(k));
        }
        if let Ok(k) = p256::ecdsa::SigningKey::from_pkcs8_der(der) {
            return Ok(SignKey::P256(k));
        }
        if let Ok(k) = p384::ecdsa::SigningKey::from_pkcs8_der(der) {
            return Ok(SignKey::P384(k));
        }
        Err("type de clé non pris en charge (RSA, ECDSA P-256 ou P-384 attendus)".into())
    }

    /// Signe `msg` ; renvoie la signature et l'algorithme à déclarer.
    pub fn sign(&self, msg: &[u8]) -> Result<(Vec<u8>, AlgorithmIdentifierOwned), String> {
        use sha2::Digest;
        Ok(match self {
            SignKey::Rsa(k) => {
                let sk = rsa::pkcs1v15::SigningKey::<sha2::Sha256>::new(k.clone());
                (sk.sign(msg).to_vec(), alg(OID_SHA256_RSA, true))
            }
            SignKey::P256(k) => {
                let s: p256::ecdsa::DerSignature = k.sign(msg);
                (s.to_vec(), alg(OID_EC_SHA256, false))
            }
            SignKey::P384(k) => {
                let s: p384::ecdsa::DerSignature = k.sign(msg);
                (s.to_vec(), alg(OID_EC_SHA384, false))
            }
            SignKey::Store(k) => match k.kind {
                super::store::KeyKind::Rsa => (k.sign_digest(&sha2::Sha256::digest(msg))?, alg(OID_SHA256_RSA, true)),
                super::store::KeyKind::P256 => (k.sign_digest(&sha2::Sha256::digest(msg))?, alg(OID_EC_SHA256, false)),
                super::store::KeyKind::P384 => (k.sign_digest(&sha2::Sha384::digest(msg))?, alg(OID_EC_SHA384, false)),
            },
        })
    }
}

/// Réexport des traits PKCS #8 (même version que rsa et p256).
mod pkcs8_reexport {
    pub use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey};
}

/// Vérifie `sig` sur `msg` avec la clé publique `spki` (DER) et l'algorithme `sig_alg`.
/// `digest_hint` : empreinte à utiliser quand l'algorithme ne la précise pas (rsaEncryption).
pub fn verify(spki_der: &[u8], sig_alg: &ObjectIdentifier, digest_hint: Option<Hash>, msg: &[u8], sig: &[u8]) -> Result<(), String> {
    use pkcs8_reexport::DecodePublicKey;
    let hash = Hash::from_oid(sig_alg).or(digest_hint).unwrap_or(Hash::Sha256);
    if *sig_alg == OID_RSA_PSS {
        let pk = rsa::RsaPublicKey::from_public_key_der(spki_der).map_err(|e| e.to_string())?;
        let s = rsa::pss::Signature::try_from(sig).map_err(|e| e.to_string())?;
        return match hash {
            Hash::Sha384 => rsa::pss::VerifyingKey::<sha2::Sha384>::new(pk).verify(msg, &s),
            Hash::Sha512 => rsa::pss::VerifyingKey::<sha2::Sha512>::new(pk).verify(msg, &s),
            _ => rsa::pss::VerifyingKey::<sha2::Sha256>::new(pk).verify(msg, &s),
        }
        .map_err(|_| "signature incorrecte".to_string());
    }
    if let Ok(pk) = rsa::RsaPublicKey::from_public_key_der(spki_der) {
        let s = rsa::pkcs1v15::Signature::try_from(sig).map_err(|e| e.to_string())?;
        return match hash {
            Hash::Sha1 => rsa::pkcs1v15::VerifyingKey::<sha1::Sha1>::new(pk).verify(msg, &s),
            Hash::Sha256 => rsa::pkcs1v15::VerifyingKey::<sha2::Sha256>::new(pk).verify(msg, &s),
            Hash::Sha384 => rsa::pkcs1v15::VerifyingKey::<sha2::Sha384>::new(pk).verify(msg, &s),
            Hash::Sha512 => rsa::pkcs1v15::VerifyingKey::<sha2::Sha512>::new(pk).verify(msg, &s),
        }
        .map_err(|_| "signature incorrecte".to_string());
    }
    use ecdsa::signature::hazmat::PrehashVerifier;
    let pre = hash.digest(&[msg]);
    if let Ok(vk) = p256::ecdsa::VerifyingKey::from_public_key_der(spki_der) {
        let s = p256::ecdsa::Signature::from_der(sig).map_err(|e| e.to_string())?;
        return vk.verify_prehash(&pre, &s).map_err(|_| "signature incorrecte".to_string());
    }
    if let Ok(vk) = p384::ecdsa::VerifyingKey::from_public_key_der(spki_der) {
        let s = p384::ecdsa::Signature::from_der(sig).map_err(|e| e.to_string())?;
        return vk.verify_prehash(&pre, &s).map_err(|_| "signature incorrecte".to_string());
    }
    Err("algorithme de clé non pris en charge".into())
}

/// Nom commun (CN) d'un nom distinctif, sinon le nom complet.
pub fn common_name(name: &x509_cert::name::Name) -> String {
    let s = name.to_string();
    for part in s.split(',') {
        if let Some(v) = part.trim().strip_prefix("CN=") {
            return v.replace("\\,", ",").trim().to_string();
        }
    }
    s
}

fn org(name: &x509_cert::name::Name) -> Option<String> {
    let s = name.to_string();
    s.split(',').find_map(|p| p.trim().strip_prefix("O=").map(|v| v.trim().to_string()))
}

pub fn spki_der(c: &Certificate) -> Vec<u8> {
    c.tbs_certificate.subject_public_key_info.to_der().unwrap_or_default()
}

/// Le certificat `child` a-t-il été émis (et signé) par `issuer` ?
pub fn issued_by(child: &Certificate, issuer: &Certificate) -> bool {
    if child.tbs_certificate.issuer != issuer.tbs_certificate.subject {
        return false;
    }
    let Ok(tbs) = child.tbs_certificate.to_der() else { return false };
    let Some(sig) = child.signature.as_bytes() else { return false };
    verify(&spki_der(issuer), &child.signature_algorithm.oid, None, &tbs, sig).is_ok()
}

/// Certificat chargé, prêt à signer.
pub struct Signer {
    pub key: SignKey,
    /// Certificat du signataire en premier, puis les autorités éventuelles.
    pub chain: Vec<Certificate>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertInfo {
    pub name: String,
    pub issuer: String,
    pub organization: Option<String>,
    /// Fin de validité (secondes depuis 1970).
    pub not_after: i64,
    pub self_signed: bool,
    pub expired: bool,
}

pub fn cert_info(c: &Certificate) -> CertInfo {
    let not_after = c.tbs_certificate.validity.not_after.to_unix_duration().as_secs() as i64;
    let now = chrono::Utc::now().timestamp();
    CertInfo {
        name: common_name(&c.tbs_certificate.subject),
        issuer: common_name(&c.tbs_certificate.issuer),
        organization: org(&c.tbs_certificate.subject),
        not_after,
        self_signed: c.tbs_certificate.subject == c.tbs_certificate.issuer,
        expired: not_after < now,
    }
}

/// Ouvre un certificat : fichier PKCS #12, ou « store:… » pour le magasin Windows
/// (le mot de passe est alors ignoré : Windows demande le PIN s'il le faut).
pub fn load(path: &str, password: &str) -> Result<Signer, String> {
    if path.starts_with(super::store::PREFIX) {
        let (key, chain) = super::store::load(path)?;
        return Ok(Signer { key: SignKey::Store(key), chain });
    }
    load_p12(Path::new(path), password)
}

/// Ouvre un certificat PKCS #12 (.pfx / .p12).
pub fn load_p12(path: &Path, password: &str) -> Result<Signer, String> {
    let data = std::fs::read(path).map_err(|e| format!("Certificat illisible ({e})."))?;
    let ks = p12_keystore::KeyStore::from_pkcs12(&data, password).map_err(|e| {
        let m = e.to_string().to_lowercase();
        if m.contains("mac") || m.contains("password") || m.contains("decrypt") {
            "Mot de passe du certificat incorrect.".to_string()
        } else {
            format!("Certificat illisible ({e}).")
        }
    })?;
    let (_, chain) = ks.private_key_chain().ok_or("Ce fichier ne contient pas de clé privée.")?;
    let key = SignKey::from_pkcs8(chain.key())?;
    let mut certs: Vec<Certificate> = chain.chain().iter().filter_map(|c| Certificate::from_der(c.as_der()).ok()).collect();
    if certs.is_empty() {
        return Err("Ce fichier ne contient pas de certificat.".into());
    }
    // Le certificat du signataire est celui dont la clé publique correspond à la clé privée.
    let own = public_spki(&key);
    if let Some(i) = certs.iter().position(|c| Some(spki_der(c)) == own) {
        certs.swap(0, i);
    }
    Ok(Signer { key, chain: certs })
}

fn public_spki(key: &SignKey) -> Option<Vec<u8>> {
    use rsa::pkcs8::EncodePublicKey;
    match key {
        SignKey::Rsa(k) => k.to_public_key().to_public_key_der().ok().map(|d| d.as_bytes().to_vec()),
        SignKey::P256(k) => k.verifying_key().to_public_key_der().ok().map(|d| d.as_bytes().to_vec()),
        SignKey::P384(k) => k.verifying_key().to_public_key_der().ok().map(|d| d.as_bytes().to_vec()),
        SignKey::Store(_) => None,
    }
}

/// Crée un certificat personnel (auto-signé, ECDSA P-256, 3 ans) et l'écrit en PKCS #12.
pub fn create_self_signed(name: &str, email: &str, organization: &str, password: &str, out: &Path) -> Result<CertInfo, String> {
    use std::str::FromStr;
    use x509_cert::builder::{Builder, CertificateBuilder, Profile};
    let clean = |s: &str| s.replace([',', '+', '"', '\\', '<', '>', ';', '='], " ").trim().to_string();
    let mut dn = format!("CN={}", clean(name));
    if !organization.trim().is_empty() {
        dn.push_str(&format!(",O={}", clean(organization)));
    }
    if !email.trim().is_empty() {
        dn.push_str(&format!(",emailAddress={}", clean(email)));
    }
    let subject = x509_cert::name::Name::from_str(&dn).map_err(|e| format!("Nom invalide ({e})."))?;
    let key = p256::ecdsa::SigningKey::random(&mut rand_core::OsRng);
    let spki = spki::SubjectPublicKeyInfoOwned::from_key(*key.verifying_key()).map_err(|e| e.to_string())?;
    let mut serial = [0u8; 16];
    rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut serial);
    serial[0] &= 0x7F;
    let serial = x509_cert::serial_number::SerialNumber::new(&serial).map_err(|e| e.to_string())?;
    let validity = x509_cert::time::Validity::from_now(std::time::Duration::from_secs(3 * 365 * 24 * 3600)).map_err(|e| e.to_string())?;
    let profile = Profile::Leaf { issuer: subject.clone(), enable_key_agreement: false, enable_key_encipherment: false };
    let builder = CertificateBuilder::new(profile, serial, validity, subject, spki, &key).map_err(|e| e.to_string())?;
    let cert = builder.build::<p256::ecdsa::DerSignature>().map_err(|e| e.to_string())?;
    let cert_der = cert.to_der().map_err(|e| e.to_string())?;
    use pkcs8_reexport::EncodePrivateKey;
    let key_der = key.to_pkcs8_der().map_err(|e| e.to_string())?;
    let mut ks = p12_keystore::KeyStore::new();
    let chain = p12_keystore::PrivateKeyChain::new(key_der.as_bytes(), *b"aura", [p12_keystore::Certificate::from_der(&cert_der).map_err(|e| e.to_string())?]);
    ks.add_entry(&clean(name), p12_keystore::KeyStoreEntry::PrivateKeyChain(chain));
    let bytes = ks.writer(password).write().map_err(|e| e.to_string())?;
    crate::edit::write_atomic(out, &bytes).map_err(|e| e.message)?;
    Ok(cert_info(&cert))
}

/// Autorités racines de confiance du système.
pub fn system_roots() -> &'static [Certificate] {
    static ROOTS: std::sync::OnceLock<Vec<Certificate>> = std::sync::OnceLock::new();
    ROOTS.get_or_init(|| {
        let res = rustls_native_certs::load_native_certs();
        res.certs.iter().filter_map(|c| Certificate::from_der(c.as_ref()).ok()).collect()
    })
}

/// Chaîne de confiance : du certificat du signataire vers une racine du système.
/// Renvoie (racine de confiance atteinte, nom de l'autorité finale).
pub fn chain_status(leaf: &Certificate, pool: &[Certificate]) -> (bool, String) {
    let roots = system_roots();
    let mut cur = leaf.clone();
    for _ in 0..8 {
        // Racine du système qui a émis ce certificat ?
        if let Some(root) = roots.iter().find(|r| issued_by(&cur, r)) {
            return (true, common_name(&root.tbs_certificate.subject));
        }
        if roots.iter().any(|r| r.to_der().ok() == cur.to_der().ok()) {
            return (true, common_name(&cur.tbs_certificate.subject));
        }
        match pool.iter().find(|c| c.tbs_certificate.subject != cur.tbs_certificate.subject && issued_by(&cur, c)) {
            Some(next) => cur = next.clone(),
            None => break,
        }
    }
    (false, common_name(&cur.tbs_certificate.issuer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certificat_personnel_aller_retour() {
        let out = std::env::temp_dir().join(format!("aura-cert-{}.p12", std::process::id()));
        let info = create_self_signed("Julien Martin", "julien@exemple.fr", "Aura", "secret", &out).unwrap();
        assert_eq!(info.name, "Julien Martin");
        assert!(info.self_signed && !info.expired);
        assert!(load_p12(&out, "faux").is_err());
        let s = load_p12(&out, "secret").unwrap();
        let (sig, a) = s.key.sign(b"bonjour").unwrap();
        assert_eq!(a.oid, OID_EC_SHA256);
        verify(&spki_der(&s.chain[0]), &a.oid, None, b"bonjour", &sig).unwrap();
        assert!(verify(&spki_der(&s.chain[0]), &a.oid, None, b"bonsoir", &sig).is_err());
        // Auto-signé : il s'est émis lui-même, mais aucune racine du système ne le reconnaît.
        assert!(issued_by(&s.chain[0], &s.chain[0]));
        assert!(!chain_status(&s.chain[0], &s.chain).0);
        let _ = std::fs::remove_file(out);
    }
}
