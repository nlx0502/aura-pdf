//! Conteneur de signature CMS (PKCS #7 détaché) et horodatage RFC 3161.

use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::content_info::ContentInfo;
use cms::signed_data::{EncapsulatedContentInfo, SignedAttributes, SignedData, SignerIdentifier, SignerInfo, SignerInfos, UnsignedAttributes};
use const_oid::ObjectIdentifier;
use der::asn1::{GeneralizedTime, Int, OctetString, SetOfVec};
use der::{Any, Decode, Encode, Sequence};
use x509_cert::attr::{Attribute, AttributeValue};
use x509_cert::Certificate;

use super::crypto::{alg, Signer, OID_SHA256};

pub const OID_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.1");
pub const OID_SIGNED_DATA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");
pub const OID_CONTENT_TYPE: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.3");
pub const OID_MESSAGE_DIGEST: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
pub const OID_SIGNING_TIME: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.5");
pub const OID_TIMESTAMP_TOKEN: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.14");

fn attr(oid: ObjectIdentifier, value: Any) -> Result<Attribute, String> {
    let mut values: SetOfVec<AttributeValue> = SetOfVec::new();
    values.insert(value).map_err(|e| e.to_string())?;
    Ok(Attribute { oid, values })
}

fn any<T: Encode>(v: &T) -> Result<Any, String> {
    let der = v.to_der().map_err(|e| e.to_string())?;
    Any::from_der(&der).map_err(|e| e.to_string())
}

/// Conteneur CMS détaché pour l'empreinte SHA-256 `digest` du document.
pub fn build(signer: &Signer, digest: &[u8], tsa: Option<&str>) -> Result<Vec<u8>, String> {
    let cert = &signer.chain[0];
    let now = der::DateTime::from_system_time(std::time::SystemTime::now()).map_err(|e| e.to_string())?;
    let attrs = vec![
        attr(OID_CONTENT_TYPE, any(&OID_DATA)?)?,
        attr(OID_SIGNING_TIME, any(&x509_cert::time::Time::UtcTime(der::asn1::UtcTime::from_date_time(now).map_err(|e| e.to_string())?))?)?,
        attr(OID_MESSAGE_DIGEST, any(&OctetString::new(digest).map_err(|e| e.to_string())?)?)?,
    ];
    let signed_attrs: SignedAttributes = SetOfVec::try_from(attrs).map_err(|e| e.to_string())?;
    let to_sign = signed_attrs.to_der().map_err(|e| e.to_string())?;
    let (sig, sig_alg) = signer.key.sign(&to_sign);

    let mut unsigned_attrs: Option<UnsignedAttributes> = None;
    if let Some(url) = tsa {
        let token = timestamp(url, &sig)?;
        unsigned_attrs = Some(SetOfVec::try_from(vec![attr(OID_TIMESTAMP_TOKEN, token)?]).map_err(|e| e.to_string())?);
    }

    let info = SignerInfo {
        version: cms::content_info::CmsVersion::V1,
        sid: SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
            issuer: cert.tbs_certificate.issuer.clone(),
            serial_number: cert.tbs_certificate.serial_number.clone(),
        }),
        digest_alg: alg(OID_SHA256, false),
        signed_attrs: Some(signed_attrs),
        signature_algorithm: sig_alg,
        signature: OctetString::new(sig).map_err(|e| e.to_string())?,
        unsigned_attrs,
    };
    let mut certs = Vec::new();
    for c in &signer.chain {
        certs.push(CertificateChoices::Certificate(c.clone()));
    }
    let sd = SignedData {
        version: cms::content_info::CmsVersion::V1,
        digest_algorithms: SetOfVec::try_from(vec![alg(OID_SHA256, false)]).map_err(|e| e.to_string())?,
        encap_content_info: EncapsulatedContentInfo { econtent_type: OID_DATA, econtent: None },
        certificates: Some(cms::signed_data::CertificateSet(SetOfVec::try_from(certs).map_err(|e| e.to_string())?)),
        crls: None,
        signer_infos: SignerInfos(SetOfVec::try_from(vec![info]).map_err(|e| e.to_string())?),
    };
    let ci = ContentInfo { content_type: OID_SIGNED_DATA, content: any(&sd)? };
    ci.to_der().map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- horodatage (RFC 3161)

#[derive(Sequence)]
struct MessageImprint {
    hash_algorithm: spki::AlgorithmIdentifierOwned,
    hashed_message: OctetString,
}

#[derive(Sequence)]
struct TimeStampReq {
    version: u8,
    message_imprint: MessageImprint,
    nonce: Int,
    cert_req: bool,
}

#[derive(Sequence)]
struct PkiStatusInfo {
    status: u8,
    #[asn1(optional = "true")]
    status_string: Option<Any>,
    #[asn1(optional = "true")]
    fail_info: Option<Any>,
}

#[derive(Sequence)]
struct TimeStampResp {
    status: PkiStatusInfo,
    #[asn1(optional = "true")]
    token: Option<ContentInfo>,
}

/// Jeton d'horodatage pour la valeur de signature `sig` (attribut non signé).
fn timestamp(url: &str, sig: &[u8]) -> Result<Any, String> {
    let digest = super::crypto::Hash::Sha256.digest(&[sig]);
    let mut nonce = [0u8; 8];
    rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut nonce);
    nonce[0] &= 0x7F;
    let req = TimeStampReq {
        version: 1,
        message_imprint: MessageImprint { hash_algorithm: alg(OID_SHA256, true), hashed_message: OctetString::new(digest).map_err(|e| e.to_string())? },
        nonce: Int::new(&nonce).map_err(|e| e.to_string())?,
        cert_req: true,
    };
    let body = req.to_der().map_err(|e| e.to_string())?;
    let resp = ureq::post(url)
        .timeout(std::time::Duration::from_secs(20))
        .set("Content-Type", "application/timestamp-query")
        .send_bytes(&body)
        .map_err(|e| format!("Serveur d’horodatage injoignable ({e})."))?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader().take(1_000_000), &mut bytes).map_err(|e| e.to_string())?;
    let r = TimeStampResp::from_der(&bytes).map_err(|e| format!("Réponse d’horodatage illisible ({e})."))?;
    if r.status.status > 1 {
        return Err(format!("Horodatage refusé par le serveur (statut {}).", r.status.status));
    }
    let token = r.token.ok_or("Réponse d’horodatage sans jeton.")?;
    any(&token)
}

use std::io::Read as _;

// ---------------------------------------------------------------- lecture (vérification)

/// Contenu d'une signature CMS, pour la vérification.
pub struct Parsed {
    pub signed: SignedData,
    pub certs: Vec<Certificate>,
}

pub fn parse(der_bytes: &[u8]) -> Result<Parsed, String> {
    // Le conteneur est complété par des zéros dans /Contents : on ne lit que la structure.
    let len = der_len(der_bytes).ok_or("conteneur de signature illisible")?;
    let ci = ContentInfo::from_der(&der_bytes[..len.min(der_bytes.len())]).map_err(|e| format!("conteneur de signature illisible ({e})"))?;
    if ci.content_type != OID_SIGNED_DATA {
        return Err("format de signature inattendu".into());
    }
    let signed: SignedData = ci.content.decode_as().map_err(|e| format!("signature illisible ({e})"))?;
    let certs = signed
        .certificates
        .as_ref()
        .map(|s| s.0.iter().filter_map(|c| if let CertificateChoices::Certificate(c) = c { Some(c.clone()) } else { None }).collect())
        .unwrap_or_default();
    Ok(Parsed { signed, certs })
}

/// Longueur totale d'un élément DER (en-tête compris).
fn der_len(b: &[u8]) -> Option<usize> {
    if b.len() < 2 {
        return None;
    }
    let l = b[1] as usize;
    if l < 0x80 {
        return Some(2 + l);
    }
    let n = l & 0x7F;
    if n == 0 || n > 4 || b.len() < 2 + n {
        return None;
    }
    let mut v = 0usize;
    for i in 0..n {
        v = (v << 8) | b[2 + i] as usize;
    }
    Some(2 + n + v)
}

/// Date d'un jeton d'horodatage (genTime du TSTInfo).
pub fn token_time(token: &Any) -> Option<chrono::DateTime<chrono::Utc>> {
    let ci: ContentInfo = token.decode_as().ok()?;
    let sd: SignedData = ci.content.decode_as().ok()?;
    let content = sd.encap_content_info.econtent?;
    let tst: OctetString = content.decode_as().ok()?;
    let mut r = der::SliceReader::new(tst.as_bytes()).ok()?;
    use der::Reader;
    let t: GeneralizedTime = r
        .sequence(|r| {
            let _version: Int = r.decode()?;
            let _policy: ObjectIdentifier = r.decode()?;
            let _imprint: Any = r.decode()?;
            let _serial: Int = r.decode()?;
            let t: GeneralizedTime = r.decode()?;
            while !r.is_finished() {
                let _: Any = r.decode()?;
            }
            Ok(t)
        })
        .ok()?;
    let secs = t.to_unix_duration().as_secs() as i64;
    chrono::DateTime::from_timestamp(secs, 0)
}
