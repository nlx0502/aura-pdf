//! Vérification des signatures d'un PDF et liste des champs à signer.
//!
//! Pour chaque signature : intégrité (empreinte des plages /ByteRange comparée
//! à celle signée), validité cryptographique (clé publique du certificat),
//! confiance (chaîne jusqu'à une autorité racine de Windows), modifications
//! apportées après la signature, horodatage.

use std::collections::HashMap;
use std::path::Path;

use cms::signed_data::SignerIdentifier;
use der::Encode;
use lopdf::{Document, Object, ObjectId};
use serde::Serialize;

use super::cms::{parse, token_time, OID_MESSAGE_DIGEST, OID_SIGNING_TIME, OID_TIMESTAMP_TOKEN};
use super::crypto::{cert_info, chain_status, spki_der, verify, Hash};
use crate::annot::geom::Rect;
use crate::annot::pdf::{deref, get, num, page_geom, text};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SigReport {
    /// Nom du champ.
    pub field: String,
    /// « digital » (certificat) ou « visual » (signature manuscrite Aura).
    pub kind: &'static str,
    pub signer: String,
    pub issuer: String,
    /// valid, untrusted, invalid, visual, unknown.
    pub status: &'static str,
    pub messages: Vec<String>,
    /// Date de signature (ISO 8601).
    pub date: Option<String>,
    pub timestamp: Option<String>,
    pub reason: Option<String>,
    pub location: Option<String>,
    /// Le document a été modifié après cette signature.
    pub modified_after: bool,
    pub page: Option<u32>,
    pub rect: Option<Rect>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmptyField {
    pub field: String,
    pub page: u32,
    pub rect: Rect,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub signatures: Vec<SigReport>,
    pub empty: Vec<EmptyField>,
}

fn pdf_date(s: &str) -> Option<String> {
    let s = s.trim().trim_start_matches("D:");
    let d: String = s.chars().take(14).collect();
    if d.len() < 8 {
        return None;
    }
    let p = |a: usize, b: usize, def: &str| d.get(a..b).unwrap_or(def).to_string();
    let mut iso = format!("{}-{}-{}T{}:{}:{}", p(0, 4, "1970"), p(4, 6, "01"), p(6, 8, "01"), p(8, 10, "00"), p(10, 12, "00"), p(12, 14, "00"));
    let tz = &s[d.len().min(s.len())..];
    match tz.chars().next() {
        Some('Z') | None => iso.push('Z'),
        Some(c @ ('+' | '-')) => {
            let digits: String = tz.chars().filter(|c| c.is_ascii_digit()).collect();
            iso.push_str(&format!("{c}{}:{}", digits.get(0..2).unwrap_or("00"), digits.get(2..4).unwrap_or("00")));
        }
        _ => iso.push('Z'),
    }
    Some(iso)
}

fn rect_of(doc: &Document, d: &lopdf::Dictionary) -> Option<Rect> {
    let a = get(doc, d, b"Rect")?.as_array().ok()?;
    let v: Vec<f32> = a.iter().filter_map(|x| num(deref(doc, x))).collect();
    (v.len() == 4).then(|| Rect::from_corners(v[0], v[1], v[2], v[3]))
}

/// Champs (feuilles) de type signature avec leur widget : (nom, dictionnaire du champ, id du widget).
fn sig_fields(doc: &Document) -> Vec<(String, ObjectId)> {
    let mut out = Vec::new();
    let Some(acro) = doc.catalog().ok().and_then(|c| get(doc, c, b"AcroForm")).and_then(|a| a.as_dict().ok()) else { return out };
    let Some(fields) = get(doc, acro, b"Fields").and_then(|f| f.as_array().ok()) else { return out };
    fn walk(doc: &Document, list: &[Object], prefix: &str, ft: Option<Vec<u8>>, out: &mut Vec<(String, ObjectId)>, depth: u32) {
        for f in list {
            let Ok(id) = f.as_reference() else { continue };
            let Ok(d) = doc.get_dictionary(id) else { continue };
            let t = d.get(b"T").map(|t| text(deref(doc, t))).unwrap_or_default();
            let full = if prefix.is_empty() { t.clone() } else if t.is_empty() { prefix.to_string() } else { format!("{prefix}.{t}") };
            let my_ft = d.get(b"FT").and_then(|x| x.as_name()).ok().map(|n| n.to_vec()).or(ft.clone());
            match get(doc, d, b"Kids").and_then(|k| k.as_array().ok()) {
                Some(k) if depth < 12 && k.iter().any(|x| doc.get_dictionary(x.as_reference().unwrap_or((0, 0))).map(|kd| kd.has(b"T")).unwrap_or(false)) => {
                    walk(doc, k, &full, my_ft, out, depth + 1)
                }
                _ => {
                    if my_ft.as_deref() == Some(b"Sig") {
                        out.push((full, id));
                    }
                }
            }
        }
    }
    walk(doc, fields, "", None, &mut out, 0);
    out
}

/// Page et rectangle d'affichage d'un widget.
fn locate(doc: &Document, widget: ObjectId, pages: &[ObjectId], annots_of: &HashMap<ObjectId, u32>) -> Option<(u32, Rect)> {
    let d = doc.get_dictionary(widget).ok()?;
    // Champ et widget fusionnés, ou premier widget enfant.
    let (wid, wd) = if d.has(b"Rect") {
        (widget, d)
    } else {
        let k = get(doc, d, b"Kids")?.as_array().ok()?.first()?.as_reference().ok()?;
        (k, doc.get_dictionary(k).ok()?)
    };
    let page = annots_of
        .get(&wid)
        .copied()
        .or_else(|| wd.get(b"P").ok().and_then(|p| p.as_reference().ok()).and_then(|p| pages.iter().position(|x| *x == p)).map(|i| i as u32))?;
    let pd = doc.get_dictionary(pages[page as usize]).ok()?;
    let r = rect_of(doc, wd)?;
    Some((page, page_geom(doc, pd).rect_to_display(&r)))
}

pub fn report(path: &Path) -> Result<Report, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Fichier illisible ({e})."))?;
    let doc = Document::load_mem(&bytes).map_err(|e| format!("Document illisible ({e})."))?;
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut annots_of: HashMap<ObjectId, u32> = HashMap::new();
    for (i, p) in pages.iter().enumerate() {
        if let Some(a) = doc.get_dictionary(*p).ok().and_then(|d| get(&doc, d, b"Annots")).and_then(|a| a.as_array().ok()) {
            for o in a {
                if let Ok(id) = o.as_reference() {
                    annots_of.insert(id, i as u32);
                }
            }
        }
    }
    let mut rep = Report::default();
    for (name, id) in sig_fields(&doc) {
        let Ok(d) = doc.get_dictionary(id) else { continue };
        let loc = locate(&doc, id, &pages, &annots_of);
        match get(&doc, d, b"V").and_then(|v| v.as_dict().ok()) {
            None => {
                if let Some((page, rect)) = loc {
                    rep.empty.push(EmptyField { field: name, page, rect });
                }
            }
            Some(v) => {
                let mut r = check(&doc, v, &bytes);
                r.field = name;
                if let Some((page, rect)) = loc {
                    r.page = Some(page);
                    r.rect = Some(rect);
                }
                rep.signatures.push(r);
            }
        }
    }
    // Signatures manuscrites posées par Aura (non certifiées).
    for (id, page) in &annots_of {
        let Ok(d) = doc.get_dictionary(*id) else { continue };
        if d.get(b"Name").and_then(|n| n.as_name()).ok() != Some(b"AuraSignature".as_slice()) {
            continue;
        }
        let pd = doc.get_dictionary(pages[*page as usize]).ok();
        rep.signatures.push(SigReport {
            field: String::new(),
            kind: "visual",
            signer: d.get(b"T").map(|t| text(deref(&doc, t))).unwrap_or_default(),
            issuer: String::new(),
            status: "visual",
            messages: vec!["Signature manuscrite, sans certificat : elle n’atteste pas l’identité du signataire.".into()],
            date: d.get(b"M").ok().map(|m| text(deref(&doc, m))).and_then(|m| pdf_date(&m)),
            timestamp: None,
            reason: None,
            location: None,
            modified_after: false,
            page: Some(*page),
            rect: pd.and_then(|pd| rect_of(&doc, d).map(|r| page_geom(&doc, pd).rect_to_display(&r))),
        });
    }
    // Champ couvert par une signature manuscrite : considéré comme rempli.
    let overlap = |a: &Rect, b: &Rect| {
        let w = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
        let h = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
        if w <= 0.0 || h <= 0.0 {
            0.0
        } else {
            w * h / (a.w * a.h).min(b.w * b.h).max(1.0)
        }
    };
    let visuals: Vec<(u32, Rect)> = rep.signatures.iter().filter(|s| s.kind == "visual").filter_map(|s| Some((s.page?, s.rect?))).collect();
    let mut filled = Vec::new();
    rep.empty.retain(|f| {
        let covered = visuals.iter().any(|(p, r)| *p == f.page && overlap(r, &f.rect) > 0.5);
        if covered {
            filled.push((f.page, f.rect, f.field.clone()));
        }
        !covered
    });
    for s in rep.signatures.iter_mut().filter(|s| s.kind == "visual") {
        if let Some((_, _, name)) = filled.iter().find(|(p, r, _)| Some(*p) == s.page && s.rect.map(|x| overlap(&x, r) > 0.5).unwrap_or(false)) {
            s.field = name.clone();
        }
    }
    rep.signatures.sort_by(|a, b| a.date.cmp(&b.date));
    Ok(rep)
}

/// Vérifie une signature numérique (dictionnaire /V).
fn check(doc: &Document, v: &lopdf::Dictionary, file: &[u8]) -> SigReport {
    let mut r = SigReport {
        field: String::new(),
        kind: "digital",
        signer: v.get(b"Name").map(|n| text(deref(doc, n))).unwrap_or_default(),
        issuer: String::new(),
        status: "unknown",
        messages: Vec::new(),
        date: v.get(b"M").ok().map(|m| text(deref(doc, m))).and_then(|m| pdf_date(&m)),
        timestamp: None,
        reason: v.get(b"Reason").ok().map(|x| text(deref(doc, x))).filter(|s| !s.is_empty()),
        location: v.get(b"Location").ok().map(|x| text(deref(doc, x))).filter(|s| !s.is_empty()),
        modified_after: false,
        page: None,
        rect: None,
    };
    let sub = v.get(b"SubFilter").and_then(|s| s.as_name()).unwrap_or(b"").to_vec();
    let br: Vec<usize> = get(doc, v, b"ByteRange").and_then(|a| a.as_array().ok()).map(|a| a.iter().filter_map(|x| num(deref(doc, x)).map(|f| f as usize)).collect()).unwrap_or_default();
    let contents = match get(doc, v, b"Contents") {
        Some(Object::String(b, _)) => b.clone(),
        _ => Vec::new(),
    };
    if br.len() != 4 || br[0] + br[1] > file.len() || br[2] + br[3] > file.len() || contents.is_empty() {
        r.status = "invalid";
        r.messages.push("Signature incomplète ou endommagée.".into());
        return r;
    }
    if sub == b"adbe.x509.rsa_sha1" {
        r.messages.push("Ancien format de signature (adbe.x509.rsa_sha1) : vérification non prise en charge.".into());
        return r;
    }
    r.modified_after = br[2] + br[3] != file.len();
    let parsed = match parse(&contents) {
        Ok(p) => p,
        Err(e) => {
            r.status = "invalid";
            r.messages.push(format!("Signature illisible : {e}."));
            return r;
        }
    };
    let Some(si) = parsed.signed.signer_infos.0.iter().next() else {
        r.status = "invalid";
        r.messages.push("Aucun signataire dans la signature.".into());
        return r;
    };
    let cert = parsed.certs.iter().find(|c| match &si.sid {
        SignerIdentifier::IssuerAndSerialNumber(i) => c.tbs_certificate.issuer == i.issuer && c.tbs_certificate.serial_number == i.serial_number,
        SignerIdentifier::SubjectKeyIdentifier(_) => true,
    });
    let Some(cert) = cert else {
        r.status = "invalid";
        r.messages.push("Le certificat du signataire est absent de la signature.".into());
        return r;
    };
    let info = cert_info(cert);
    r.signer = info.name.clone();
    r.issuer = info.issuer.clone();

    let hash = Hash::from_oid(&si.digest_alg.oid).unwrap_or(Hash::Sha256);
    let ranges: [&[u8]; 2] = [&file[br[0]..br[0] + br[1]], &file[br[2]..br[2] + br[3]]];
    let is_timestamp = sub == b"ETSI.RFC3161";
    if is_timestamp {
        r.kind = "digital";
        r.messages.push("Horodatage du document.".into());
    }
    let digest = hash.digest(&ranges);
    let sig_bytes = si.signature.as_bytes();
    let ok = match &si.signed_attrs {
        Some(attrs) => {
            let md = attrs.iter().find(|a| a.oid == OID_MESSAGE_DIGEST).and_then(|a| a.values.iter().next()).and_then(|v| v.decode_as::<der::asn1::OctetString>().ok());
            if let Some(t) = attrs.iter().find(|a| a.oid == OID_SIGNING_TIME).and_then(|a| a.values.iter().next()).and_then(|v| v.to_der().ok().and_then(|d| <x509_cert::time::Time as der::Decode>::from_der(&d).ok())) {
                let secs = t.to_unix_duration().as_secs() as i64;
                if let Some(dt) = chrono::DateTime::from_timestamp(secs, 0) {
                    r.date = Some(dt.to_rfc3339());
                }
            }
            match md {
                Some(md) if md.as_bytes() == digest.as_slice() => {
                    let data = attrs.to_der().unwrap_or_default();
                    verify(&spki_der(cert), &si.signature_algorithm.oid, Some(hash), &data, sig_bytes).map_err(|e| e.to_string())
                }
                Some(_) => Err("le document a été modifié : son empreinte ne correspond plus".into()),
                None => Err("empreinte signée absente".into()),
            }
        }
        None => {
            let mut all = ranges[0].to_vec();
            all.extend_from_slice(ranges[1]);
            verify(&spki_der(cert), &si.signature_algorithm.oid, Some(hash), &all, sig_bytes)
        }
    };
    if let Some(ua) = &si.unsigned_attrs {
        if let Some(tok) = ua.iter().find(|a| a.oid == OID_TIMESTAMP_TOKEN).and_then(|a| a.values.iter().next()) {
            r.timestamp = token_time(tok).map(|t| t.to_rfc3339());
        }
    }
    if let Err(e) = ok {
        r.status = "invalid";
        r.messages.push(format!("Signature non valide : {e}."));
        return r;
    }
    let (trusted, root) = chain_status(cert, &parsed.certs);
    if trusted {
        r.status = "valid";
        r.messages.push(format!("Identité certifiée par {root}."));
    } else {
        r.status = "untrusted";
        r.messages.push(if info.self_signed {
            "Certificat personnel (auto-signé) : l’intégrité est garantie, l’identité n’est pas certifiée par une autorité.".to_string()
        } else {
            format!("Autorité « {root} » inconnue de Windows : l’identité n’est pas vérifiée.")
        });
    }
    // Validité du certificat à la date de signature.
    let at = r.timestamp.as_ref().or(r.date.as_ref()).and_then(|d| chrono::DateTime::parse_from_rfc3339(d).ok()).map(|d| d.timestamp()).unwrap_or_else(|| chrono::Utc::now().timestamp());
    let nb = cert.tbs_certificate.validity.not_before.to_unix_duration().as_secs() as i64;
    if at < nb || at > info.not_after {
        r.status = "invalid";
        r.messages.push("Le certificat n’était pas valide à la date de signature.".into());
    }
    if r.modified_after {
        r.messages.push("Le document a été modifié après cette signature (ajouts ultérieurs, par exemple d’autres signatures ou des annotations).".into());
    }
    r
}
