use super::*;
use crate::tools::Job;

fn sample(tag: &str) -> std::path::PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/Rapport Q1 2025.pdf");
    let dir = std::env::temp_dir().join(format!("aura-secu-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("doc.pdf");
    std::fs::copy(src, &out).unwrap();
    out
}

fn spec() -> SecuritySpec {
    SecuritySpec {
        protect: true,
        user_password: "ouvre-moi".into(),
        owner_password: "patron".into(),
        aes256: true,
        encrypt_metadata: true,
        permissions: Perms { print: true, copy: false, edit: false, annotate: true, forms: true, extract: false },
        metadata: Meta { title: "Rapport protégé".into(), author: "Équipe".into(), subject: String::new(), keywords: "test".into() },
        pdfa: None,
    }
}

#[test]
fn chiffrement_aes256_et_permissions() {
        let p = sample("t1");
    apply(&p, None, &spec(), &Job::silent()).unwrap();
    assert!(load(&p, None).map(|d| d.encryption_state.is_none() && d.trailer.has(b"Encrypt")).unwrap_or(true), "illisible sans mot de passe");
    assert!(load(&p, Some("faux")).is_err());
    let info = inspect(&p, Some("ouvre-moi")).unwrap();
    assert_eq!(info.algorithm, "aes-256");
    assert!(info.open_password);
    assert!(!info.owner);
    assert!(!info.permissions.copy && info.permissions.print && !info.permissions.extract);
    assert!(inspect(&p, Some("patron")).unwrap().owner, "mot de passe propriétaire reconnu");
    assert_eq!(info.metadata.title, "Rapport protégé");
    let d = load(&p, Some("ouvre-moi")).unwrap();
    assert!(d.get_pages().len() > 1);
    // Le mot de passe propriétaire ouvre aussi le document.
    assert!(load(&p, Some("patron")).is_ok());
    // Retrait de la protection.
    let mut clear = spec();
    clear.protect = false;
    clear.permissions = Perms::ALL;
    apply(&p, Some("ouvre-moi"), &clear, &Job::silent()).unwrap();
    let info = inspect(&p, None).unwrap();
    assert_eq!(info.algorithm, "none");
    assert!(!info.open_password);
    assert_eq!(info.metadata.author, "Équipe");
}

#[test]
fn chiffrement_aes128_permissions_seules() {
        let p = sample("t2");
    let mut s = spec();
    s.protect = false;
    s.aes256 = false;
    apply(&p, None, &s, &Job::silent()).unwrap();
    let info = inspect(&p, None).unwrap();
    assert_eq!(info.algorithm, "aes-128");
    assert!(!info.open_password);
    assert!(!info.permissions.copy);
    assert!(!info.owner && !info.effective().copy);
    assert!(load(&p, None).unwrap().get_pages().len() > 1);
}

#[test]
fn archivage_pdfa() {
        let p = sample("t3");
    let before = pdfa::check(&load(&p, None).unwrap(), "2b", false).unwrap();
    assert!(before.iter().any(|c| c.id == "colors" && !c.ok));
    assert!(before.iter().any(|c| c.id == "fonts" && !c.ok && c.fixable), "Helvetica non intégrée, remplaçable");
    let mut s = spec();
    s.pdfa = Some("2b".into());
    assert!(apply(&p, None, &s, &Job::silent()).is_err(), "PDF/A et chiffrement incompatibles");
    s.protect = false;
    s.permissions = Perms::ALL;
    apply(&p, None, &s, &Job::silent()).unwrap();
    let doc = load(&p, None).unwrap();
    assert_eq!(pdfa::declared(&doc).as_deref(), Some("2b"));
    let after = pdfa::check(&doc, "2b", false).unwrap();
    for c in &after {
        assert!(c.ok, "{c:?}");
    }
    // Les polices intégrées gardent le texte extractible.
    assert!(doc.extract_text(&[1]).unwrap_or_default().contains("Q1"));
    assert_eq!(inspect(&p, None).unwrap().pdfa.as_deref(), Some("2b"));
}

#[test]
fn edition_et_annotation_d_un_document_chiffre() {
    use crate::annot::model::{Annot, Kind};
    use crate::edit::{PageSrc, Plan, PlanPage};
    for aes256 in [true, false] {
        let p = sample(if aes256 { "edit256" } else { "edit128" });
        let mut s = spec();
        s.aes256 = aes256;
        s.permissions = Perms::ALL;
        apply(&p, None, &s, &Job::silent()).unwrap();
        let pages = load(&p, Some("ouvre-moi")).unwrap().get_pages().len() as u32;
        // Suppression de la première page : le résultat reste chiffré, mêmes mots de passe.
        let keep = load(&p, Some("ouvre-moi")).unwrap().encryption_state;
        let plan = Plan {
            pages: (1..pages).map(|i| PlanPage { src: PageSrc::Orig { index: i }, rot: 0, crop: false, redact: vec![], links: vec![] }).collect(),
            header: None,
            numbering: None,
            watermark: None,
            crop: None,
            clean_metadata: false,
        };
        crate::edit::apply(&p, Some("ouvre-moi"), &plan, &p, keep.as_ref()).unwrap();
        let info = inspect(&p, Some("ouvre-moi")).unwrap();
        assert_eq!(info.algorithm, if aes256 { "aes-256" } else { "aes-128" });
        assert!(info.open_password);
        assert_eq!(load(&p, Some("ouvre-moi")).unwrap().get_pages().len() as u32, pages - 1);
        assert!(load(&p, Some("faux")).is_err());
        // Annotation : mise à jour incrémentale chiffrée.
        let a = Annot { id: "n1".into(), page: 0, kind: Kind::Note, color: "#FFD400".into(), contents: "Secret".into(), rect: Some(crate::annot::geom::Rect { x: 50.0, y: 50.0, w: 20.0, h: 20.0 }), ..Default::default() };
        crate::annot::save(&p, Some("ouvre-moi"), &[a]).unwrap();
        let back = crate::annot::load(&p, Some("ouvre-moi")).unwrap();
        assert!(back.iter().any(|a| a.contents == "Secret"));
        let raw = std::fs::read(&p).unwrap();
        assert!(!raw.windows(6).any(|w| w == b"Secret"), "le commentaire est chiffré dans le fichier");
    }
}

/// Fichiers à ouvrir dans Acrobat (vérification manuelle) :
/// `cargo test --lib exporter_pour_acrobat -- --ignored`, dans %TEMP%\aura-acrobat.
#[test]
#[ignore]
fn exporter_pour_acrobat() {
    let dir = std::env::temp_dir().join("aura-acrobat");
    let _ = std::fs::create_dir_all(&dir);
    for (name, aes256, protect) in [("aes256.pdf", true, true), ("aes128.pdf", false, true), ("permissions.pdf", true, false)] {
        let p = dir.join(name);
        std::fs::copy(sample(name), &p).unwrap();
        let mut s = spec();
        s.aes256 = aes256;
        s.protect = protect;
        apply(&p, None, &s, &Job::silent()).unwrap();
    }
    let p = dir.join("pdfa2b.pdf");
    std::fs::copy(sample("pdfa"), &p).unwrap();
    let mut s = spec();
    s.protect = false;
    s.permissions = Perms::ALL;
    s.pdfa = Some("2b".into());
    apply(&p, None, &s, &Job::silent()).unwrap();
}
