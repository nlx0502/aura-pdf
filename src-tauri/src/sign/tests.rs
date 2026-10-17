//! Tests de la signature : certificat personnel, signature numérique,
//! co-signature, détection d'une falsification, signature manuscrite.

use std::path::PathBuf;

use super::appearance::{Visual, VisualSpec};
use super::verify::report;
use super::*;
use crate::tools::Job;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("tests").join("fixtures").join(name)
}

fn tmp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("aura-sign-{}-{name}", std::process::id()))
}

fn drawn() -> VisualSpec {
    VisualSpec {
        visual: Visual::Draw { strokes: vec![vec![[10.0, 80.0], [30.0, 20.0], [50.0, 90.0], [70.0, 30.0], [120.0, 60.0]]] },
        color: "#1F2A6B".into(),
        caption: vec!["Signé par Julien Martin".into()],
    }
}

#[test]
fn signature_numerique_cosignature_et_falsification() {
    let src = fixture("Rapport Q1 2025.pdf");
    if !src.exists() {
        return;
    }
    let cert = tmp("cert.p12");
    crypto::create_self_signed("Julien Martin", "", "Aura", "secret", &cert).unwrap();
    let path = tmp("signe.pdf");
    std::fs::copy(&src, &path).unwrap();
    let original = std::fs::read(&path).unwrap();
    let cr = |reason: &str| CertRequest { path: cert.to_string_lossy().into(), password: "secret".into(), tsa: None, reason: reason.into(), location: "Lyon".into(), pades: true, certify: None };
    let req = SignRequest { page: 13, rect: Rect { x: 320.0, y: 640.0, w: 200.0, h: 80.0 }, visual: drawn(), field: None, cert: Some(cr("Approbation")), author: String::new() };
    sign(&path, &req, &Job::silent()).unwrap();
    // Mise à jour incrémentale : l'original est intact en tête du fichier.
    let signed = std::fs::read(&path).unwrap();
    assert_eq!(&signed[..original.len() - 64], &original[..original.len() - 64]);

    let r = report(&path).unwrap();
    assert_eq!(r.signatures.len(), 1);
    let s = &r.signatures[0];
    assert_eq!((s.signer.as_str(), s.status), ("Julien Martin", "untrusted"), "{:?}", s.messages);
    assert_eq!(s.reason.as_deref(), Some("Approbation"));
    assert!(!s.modified_after);
    assert_eq!(s.page, Some(13));
    assert!((s.rect.unwrap().x - 320.0).abs() < 0.5, "position d'affichage conservée");

    // Co-signature : la première reste valide, mais signale l'ajout ultérieur.
    let mut req2 = req.clone();
    req2.rect.y = 720.0;
    req2.visual = VisualSpec { visual: Visual::Type { text: "J. Martin".into(), font: "caveat".into() }, color: "#1F2A6B".into(), caption: vec![] };
    sign(&path, &req2, &Job::silent()).unwrap();
    let r = report(&path).unwrap();
    assert_eq!(r.signatures.len(), 2);
    assert!(r.signatures.iter().all(|s| s.status == "untrusted"), "{:?}", r.signatures);
    assert_eq!(r.signatures.iter().filter(|s| s.modified_after).count(), 1);

    // Falsification : un octet du contenu signé modifié → signature invalide.
    let mut bytes = std::fs::read(&path).unwrap();
    let pos = bytes.windows(4).position(|w| w == b"Aura").unwrap();
    bytes[pos] = b'X';
    let forged = tmp("falsifie.pdf");
    std::fs::write(&forged, &bytes).unwrap();
    let r = report(&forged).unwrap();
    assert!(r.signatures.iter().all(|s| s.status == "invalid"), "{:?}", r.signatures);

    // Mauvais mot de passe.
    let mut bad = req.clone();
    bad.cert.as_mut().unwrap().password = "faux".into();
    assert!(sign(&path, &bad, &Job::silent()).unwrap_err().contains("Mot de passe"));
    for p in [cert, path, forged] {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn signature_manuscrite_et_champ_existant() {
    let src = fixture("Rapport Q1 2025.pdf");
    if !src.exists() {
        return;
    }
    let path = tmp("manuscrite.pdf");
    std::fs::copy(&src, &path).unwrap();
    let req = SignRequest { page: 0, rect: Rect { x: 60.0, y: 700.0, w: 180.0, h: 70.0 }, visual: drawn(), field: None, cert: None, author: "Julien Martin".into() };
    sign(&path, &req, &Job::silent()).unwrap();
    let r = report(&path).unwrap();
    assert_eq!(r.signatures.len(), 1);
    assert_eq!((r.signatures[0].kind, r.signatures[0].status), ("visual", "visual"));
    assert_eq!(r.signatures[0].signer, "Julien Martin");

    // Champ de signature vide ajouté à la main, puis rempli par une signature numérique.
    let mut doc = lopdf::Document::load(&path).unwrap();
    let pid = *doc.get_pages().values().next().unwrap();
    let widget = doc.add_object(lopdf::dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Sig", "T" => lopdf::Object::string_literal("Client"),
        "Rect" => vec![300.into(), 60.into(), 500.into(), 120.into()], "P" => pid,
    });
    doc.get_dictionary_mut(pid).unwrap().set("Annots", vec![lopdf::Object::Reference(widget)]);
    doc.catalog_mut().unwrap().set("AcroForm", lopdf::dictionary! { "Fields" => vec![lopdf::Object::Reference(widget)] });
    doc.save(&path).unwrap();
    let r = report(&path).unwrap();
    assert_eq!(r.empty.len(), 1);
    assert_eq!(r.empty[0].field, "Client");
    let cert = tmp("cert2.p12");
    crypto::create_self_signed("Société B", "", "", "x", &cert).unwrap();
    let f = r.empty[0].clone();
    let req = SignRequest {
        page: f.page,
        rect: f.rect,
        visual: VisualSpec { visual: Visual::Type { text: "Société B".into(), font: "delafield".into() }, color: "#1F2A6B".into(), caption: vec![] },
        field: Some("Client".into()),
        cert: Some(CertRequest { path: cert.to_string_lossy().into(), password: "x".into(), tsa: None, reason: String::new(), location: String::new(), pades: true, certify: None }),
        author: String::new(),
    };
    sign(&path, &req, &Job::silent()).unwrap();
    let r = report(&path).unwrap();
    assert!(r.empty.is_empty(), "champ rempli");
    // Signature manuscrite posée sur un champ : le champ ne figure plus parmi ceux à remplir.
    let s = r.signatures.iter().find(|s| s.kind == "digital").unwrap();
    assert_eq!((s.field.as_str(), s.signer.as_str(), s.status), ("Client", "Société B", "untrusted"));
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(cert);
}

/// Vérification croisée avec OpenSSL (lancée à la main) :
/// `AURA_P12=… AURA_OUT=… cargo test --lib export_pour_openssl -- --ignored`.
#[test]
#[ignore]
fn export_pour_openssl() {
    let p12 = std::env::var("AURA_P12").unwrap();
    let out = PathBuf::from(std::env::var("AURA_OUT").unwrap());
    let path = out.join("signe.pdf");
    std::fs::copy(fixture("Rapport Q1 2025.pdf"), &path).unwrap();
    let req = SignRequest {
        page: 0,
        rect: Rect { x: 60.0, y: 700.0, w: 180.0, h: 70.0 },
        visual: drawn(),
        field: None,
        cert: Some(CertRequest { path: p12, password: "secret".into(), tsa: std::env::var("AURA_TSA").ok(), reason: String::new(), location: String::new(), pades: true, certify: std::env::var("AURA_CERTIFY").ok().and_then(|p| p.parse().ok()) }),
        author: String::new(),
    };
    sign(&path, &req, &Job::silent()).unwrap();
    let r = report(&path).unwrap();
    println!("STATUT {} {:?} {:?}", r.signatures[0].status, r.signatures[0].messages, r.signatures[0].timestamp);
    // Contenu signé (plages) et conteneur CMS, pour `openssl cms -verify`.
    let bytes = std::fs::read(&path).unwrap();
    let doc = lopdf::Document::load_mem(&bytes).unwrap();
    let v = doc.objects.values().filter_map(|o| o.as_dict().ok()).find(|d| d.get(b"Type").and_then(|t| t.as_name()).ok() == Some(b"Sig".as_slice())).unwrap();
    let br: Vec<usize> = v.get(b"ByteRange").unwrap().as_array().unwrap().iter().map(|x| x.as_i64().unwrap() as usize).collect();
    let mut content = bytes[br[0]..br[0] + br[1]].to_vec();
    content.extend_from_slice(&bytes[br[2]..br[2] + br[3]]);
    std::fs::write(out.join("contenu.bin"), content).unwrap();
    let Ok(lopdf::Object::String(c, _)) = v.get(b"Contents") else { panic!() };
    let len = 4 + (((c[2] as usize) << 8) | c[3] as usize);
    std::fs::write(out.join("signature.der"), &c[..len]).unwrap();
}

#[test]
fn pades_et_certification() {
    let src = fixture("Rapport Q1 2025.pdf");
    if !src.exists() {
        return;
    }
    let cert = tmp("cert-mdp.p12");
    crypto::create_self_signed("Claire Dubois", "", "Aura", "secret", &cert).unwrap();
    let path = tmp("certifie.pdf");
    std::fs::copy(&src, &path).unwrap();
    let cr = CertRequest { path: cert.to_string_lossy().into(), password: "secret".into(), tsa: None, reason: String::new(), location: String::new(), pades: true, certify: Some(2) };
    let req = SignRequest { page: 0, rect: Rect { x: 320.0, y: 640.0, w: 200.0, h: 80.0 }, visual: drawn(), field: None, cert: Some(cr), author: String::new() };
    sign(&path, &req, &Job::silent()).unwrap();

    let bytes = std::fs::read(&path).unwrap();
    let doc = lopdf::Document::load_mem(&bytes).unwrap();
    let perms = doc.catalog().unwrap().get(b"Perms").unwrap().as_dict().unwrap();
    let sig = doc.get_dictionary(perms.get(b"DocMDP").unwrap().as_reference().unwrap()).unwrap();
    assert_eq!(sig.get(b"SubFilter").unwrap().as_name().unwrap(), b"ETSI.CAdES.detached");

    let r = report(&path).unwrap();
    let s = &r.signatures[0];
    assert_eq!(s.status, "untrusted", "{:?}", s.messages);
    assert!(s.pades);
    assert_eq!(s.certified, Some(2));
    assert!(s.date.is_some(), "date tirée de /M en l'absence de signing-time");

    // Une seconde signature ne peut pas certifier.
    assert!(sign(&path, &req, &Job::silent()).unwrap_err().contains("première signature"));
    // Mais une signature d'approbation est possible.
    let mut approve = req.clone();
    approve.cert.as_mut().unwrap().certify = None;
    approve.rect.y = 720.0;
    sign(&path, &approve, &Job::silent()).unwrap();
    let r = report(&path).unwrap();
    assert_eq!(r.signatures.len(), 2);
    assert!(r.signatures.iter().all(|s| s.status == "untrusted"), "{:?}", r.signatures);
    for p in [cert, path] {
        let _ = std::fs::remove_file(p);
    }
}
