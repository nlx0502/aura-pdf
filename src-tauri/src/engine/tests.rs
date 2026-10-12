//! Tests d'intégration du moteur avec la vraie bibliothèque PDFium et les PDF
//! de `tests/fixtures` (générés par `scripts/make_fixtures.py`).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub(super) fn fixture(name: &str) -> PathBuf {
    root().join("..").join("tests").join("fixtures").join(name)
}

pub(super) type Events = Arc<Mutex<Vec<(String, serde_json::Value)>>>;

/// PDFium est une bibliothèque globale : un seul moteur partagé par tous les tests
/// (comme dans l'application).
pub(super) fn engine() -> (Engine, Events) {
    static SHARED: std::sync::OnceLock<(Engine, Events)> = std::sync::OnceLock::new();
    SHARED
        .get_or_init(|| {
            let events: Events = Default::default();
            let ev = events.clone();
            let emitter: Emitter = Arc::new(move |name: &str, payload: serde_json::Value| {
                ev.lock().unwrap().push((name.to_string(), payload));
            });
            (Engine::start(root().join("resources").join("pdfium"), emitter), events)
        })
        .clone()
}

pub(super) fn block<T>(f: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(f)
}

pub(super) fn have_fixtures() -> bool {
    let ok = fixture("Rapport Q1 2025.pdf").exists();
    if !ok {
        eprintln!("PDF de test absents : lancer `python scripts/make_fixtures.py`");
    }
    ok
}

#[test]
fn ouverture_et_proprietes() {
    if !have_fixtures() {
        return;
    }
    let (e, _) = engine();
    let info = block(e.open(fixture("Rapport Q1 2025.pdf"), None)).unwrap();
    assert_eq!(info.page_count, 14);
    assert_eq!(info.metadata.author.as_deref(), Some("Division ingénierie"));
    assert_eq!(info.attachments.len(), 1);
    assert!(info.outline.len() >= 7, "table des matières attendue");
    assert_eq!(info.outline[1].page, Some(1));
    assert!(!info.encrypted);
}

#[test]
fn gros_document_ouvert_rapidement() {
    if !have_fixtures() {
        return;
    }
    let (e, _) = engine();
    let t0 = Instant::now();
    let info = block(e.open(fixture("Gros document.pdf"), None)).unwrap();
    assert_eq!(info.page_count, 600);
    assert!(t0.elapsed() < Duration::from_secs(2), "ouverture trop lente : {:?}", t0.elapsed());
}

#[test]
fn mot_de_passe() {
    if !have_fixtures() {
        return;
    }
    let (e, _) = engine();
    let path = fixture("Protégé (aura).pdf");
    assert_eq!(block(e.open(path.clone(), None)).unwrap_err().code, "password_required");
    assert_eq!(block(e.open(path.clone(), Some("faux".into()))).unwrap_err().code, "wrong_password");
    let info = block(e.open(path, Some("aura".into()))).unwrap();
    assert!(info.encrypted);
}

#[test]
fn fichier_endommage() {
    if !have_fixtures() {
        return;
    }
    let (e, _) = engine();
    assert_eq!(block(e.open(fixture("Endommagé.pdf"), None)).unwrap_err().code, "corrupted");
}

#[test]
fn recherche_progressive() {
    if !have_fixtures() {
        return;
    }
    let (e, events) = engine();
    let info = block(e.open(fixture("Rapport Q1 2025.pdf"), None)).unwrap();
    e.search(info.id, "ANNEXES".into(), 7).unwrap();
    let t0 = Instant::now();
    let mine = |n: &str, p: &serde_json::Value, name: &str| n == name && p["token"] == 7 && p["doc"] == info.id;
    loop {
        if events.lock().unwrap().iter().any(|(n, p)| mine(n, p, "search:done")) {
            break;
        }
        assert!(t0.elapsed() < Duration::from_secs(10), "recherche trop longue");
        std::thread::sleep(Duration::from_millis(20));
    }
    let ev = events.lock().unwrap();
    let total = ev.iter().find(|(n, p)| mine(n, p, "search:done")).unwrap().1["total"].as_u64().unwrap();
    // 2 occurrences dans le texte de chaque page, plus le titre « Annexes » des pages 7 et 13.
    assert_eq!(total, 30);
    let first = ev.iter().find(|(n, p)| mine(n, p, "search:progress") && !p["hits"].as_array().unwrap().is_empty()).unwrap();
    let hit = &first.1["hits"][0];
    assert_eq!(hit["matched"], "annexes");
    assert!(hit["rects"][0]["w"].as_f64().unwrap() > 10.0);
}

#[test]
fn rendu_page_et_tuile() {
    if !have_fixtures() {
        return;
    }
    let (e, _) = engine();
    let info = block(e.open(fixture("Paysage.pdf"), None)).unwrap();
    // Page 3 : /Rotate 90 → affichée en paysage.
    assert!(info.pages[2].w > info.pages[2].h);
    let (tx, rx) = std::sync::mpsc::channel();
    for tile in [None, Some([100, 100, 256, 128])] {
        let tx = tx.clone();
        e.render(RenderJob {
            doc: info.id,
            page: 2,
            width_px: 800,
            rotation: 0,
            annotations: true,
            forms: true,
            tile,
            prio: prio::VISIBLE,
            gen: 0,
            responder: Box::new(move |o| {
                let _ = tx.send(match o {
                    RenderOutcome::Ok(b) => Some((b.width, b.height, b.rgba.len())),
                    _ => None,
                });
            }),
        });
    }
    // Même priorité : la demande la plus récente est servie d'abord (ordre indifférent ici).
    let mut got = [rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap(), rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap()];
    got.sort_by_key(|g| std::cmp::Reverse(g.0));
    let [full, tile] = got;
    assert_eq!(full.0, 800);
    assert!(full.1 < 800, "page en paysage");
    assert_eq!(full.2, (full.0 * full.1 * 4) as usize);
    assert_eq!((tile.0, tile.1), (256, 128));
}

/// Une seule impression à la fois dans le moteur : les tests d'impression sont sérialisés.
static PRINT_LOCK: Mutex<()> = Mutex::new(());

fn wait_print_done(events: &Events, job: u64) -> serde_json::Value {
    let t0 = Instant::now();
    loop {
        if let Some((_, p)) = events.lock().unwrap().iter().find(|(n, p)| n == "print:done" && p["job"] == job) {
            return p.clone();
        }
        assert!(t0.elapsed() < Duration::from_secs(30), "impression trop longue");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn impression_vers_pdf_imposee() {
    use crate::print::layout::{Mode, PrintSettings};
    if !have_fixtures() {
        return;
    }
    let _guard = PRINT_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (e, events) = engine();
    let info = block(e.open(fixture("Rapport Q1 2025.pdf"), None)).unwrap();
    let out = std::env::temp_dir().join("aura-test-4-par-feuille.pdf");
    let _ = std::fs::remove_file(&out);
    let settings = PrintSettings { mode: Mode::Multi, per_sheet: 4, summary: true, ..Default::default() };
    block(e.print_start(info.id, 901, settings, 0.0, crate::print::PrintTarget::Pdf { path: out.to_string_lossy().into() })).unwrap();
    let done = wait_print_done(&events, 901);
    assert_eq!(done["ok"], true, "{done}");
    // 14 pages à 4 par feuille → 4 feuilles + 1 page de résumé.
    let result = block(e.open(out.clone(), None)).unwrap();
    assert_eq!(result.page_count, 5);
    // Le contenu des pages sources est bien présent (texte vectoriel, recherchable).
    let words = block(e.page_text(result.id, 0)).unwrap();
    assert!(words.iter().any(|w| w.t == "Stratégie"), "texte vectoriel attendu");
    let summary = block(e.page_text(result.id, 4)).unwrap();
    assert!(summary.iter().any(|w| w.t == "Résumé"));
}

#[test]
fn impression_brochure_et_pages_tournees() {
    use crate::print::layout::{Mode, PrintSettings};
    if !have_fixtures() {
        return;
    }
    let _guard = PRINT_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (e, events) = engine();
    let info = block(e.open(fixture("Paysage.pdf"), None)).unwrap();
    let out = std::env::temp_dir().join("aura-test-brochure.pdf");
    let settings = PrintSettings { mode: Mode::Booklet, ..Default::default() };
    block(e.print_start(info.id, 902, settings, 0.0, crate::print::PrintTarget::Pdf { path: out.to_string_lossy().into() })).unwrap();
    assert_eq!(wait_print_done(&events, 902)["ok"], true);
    let result = block(e.open(out, None)).unwrap();
    // 4 pages → une feuille pliée : 2 faces de 2 pages, en paysage.
    assert_eq!(result.page_count, 2);
    assert!(result.pages[0].w > result.pages[0].h);
    // La page 3 (/Rotate 90) reste lisible : son texte est extrait sur la face qui la porte.
    // Face 2 : page 2 à gauche, page 3 à droite. La page 3 (/Rotate 90) doit être
    // tournée comme à l'écran : texte vertical, lu de haut en bas, dans la moitié droite.
    let words = block(e.page_text(result.id, 1)).unwrap();
    let vertical: Vec<_> = words.iter().filter(|w| w.x > 421.0).collect();
    let text: String = vertical.iter().map(|w| w.t.as_str()).collect();
    assert!(text.starts_with("Page3"), "{text}");
    assert!(vertical.windows(2).all(|p| p[1].y >= p[0].y - 0.5), "lecture de haut en bas");
    assert!(words.iter().any(|w| w.t == "841×595" && w.x < 421.0), "page 2 dans la moitié gauche");
}

/// Chaîne complète d'impression GDI, sans papier : « Microsoft Print to PDF » écrit dans un fichier.
#[cfg(windows)]
#[test]
fn impression_gdi_vers_imprimante_virtuelle() {
    use crate::print::layout::PrintSettings;
    if !have_fixtures() {
        return;
    }
    let printer = "Microsoft Print to PDF";
    if !crate::print::windows::list_printers().iter().any(|p| p.name == printer) {
        eprintln!("« {printer} » absente : test ignoré");
        return;
    }
    let _guard = PRINT_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (e, events) = engine();
    let info = block(e.open(fixture("Paysage.pdf"), None)).unwrap();
    let out = std::env::temp_dir().join("aura-test-gdi.pdf");
    let _ = std::fs::remove_file(&out);
    let settings = PrintSettings { copies: 2, ..Default::default() };
    let target = crate::print::PrintTarget::Printer { name: printer.into(), file: Some(out.to_string_lossy().into()) };
    block(e.print_start(info.id, 904, settings, 0.0, target)).unwrap();
    let done = wait_print_done(&events, 904);
    assert_eq!(done["ok"], true, "{done}");
    // Progression « envoi » : la dernière annonce 8 faces transmises sur 8 (4 pages × 2 copies).
    let last = events
        .lock()
        .unwrap()
        .iter()
        .filter(|(n, p)| n == "print:progress" && p["job"] == 904 && p["phase"] == "send")
        .last()
        .map(|(_, p)| (p["done"].as_u64(), p["total"].as_u64()))
        .unwrap();
    assert_eq!(last, (Some(8), Some(8)));
    // Le spouleur écrit le fichier de façon asynchrone.
    let t0 = Instant::now();
    while std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0) == 0 {
        assert!(t0.elapsed() < Duration::from_secs(30), "fichier non produit par le spouleur");
        std::thread::sleep(Duration::from_millis(200));
    }
    std::thread::sleep(Duration::from_millis(500));
    let result = block(e.open(out, None)).unwrap();
    assert_eq!(result.page_count, 8);
    // Orientation automatique, face par face : portrait, paysage, paysage (/Rotate 90),
    // puis retour au portrait (A5) — ce retour était oublié avant correction.
    let land: Vec<bool> = result.pages.iter().take(4).map(|p| p.w > p.h).collect();
    assert_eq!(land, vec![false, true, true, false]);
}

#[test]
fn annulation_en_cours_de_route() {
    use crate::print::layout::PrintSettings;
    if !have_fixtures() {
        return;
    }
    let _guard = PRINT_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (e, events) = engine();
    let info = block(e.open(fixture("Gros document.pdf"), None)).unwrap();
    let out = std::env::temp_dir().join("aura-test-annule.pdf");
    let _ = std::fs::remove_file(&out);
    block(e.print_start(info.id, 905, PrintSettings::default(), 0.0, crate::print::PrintTarget::Pdf { path: out.to_string_lossy().into() })).unwrap();
    // Attendre que la composition ait commencé, puis annuler.
    let t0 = Instant::now();
    while !events.lock().unwrap().iter().any(|(n, p)| n == "print:progress" && p["job"] == 905) {
        assert!(t0.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(5));
    }
    e.print_cancel(905).unwrap();
    let done = wait_print_done(&events, 905);
    assert_eq!(done["cancelled"], true);
    assert!(!out.exists(), "aucun fichier après annulation");
    // Le moteur reste disponible : une nouvelle impression peut démarrer.
    block(e.print_start(info.id, 906, PrintSettings { range: crate::print::layout::RangeKind::Current, ..Default::default() }, 0.0, crate::print::PrintTarget::Pdf { path: out.to_string_lossy().into() })).unwrap();
    assert_eq!(wait_print_done(&events, 906)["ok"], true);
}

#[test]
fn plage_invalide_refusee() {
    use crate::print::layout::{PrintSettings, RangeKind};
    if !have_fixtures() {
        return;
    }
    let _guard = PRINT_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (e, _) = engine();
    let info = block(e.open(fixture("Paysage.pdf"), None)).unwrap();
    let settings = PrintSettings { range: RangeKind::Custom, custom: "9".into(), ..Default::default() };
    let err = block(e.print_start(info.id, 903, settings, 0.0, crate::print::PrintTarget::Pdf { path: "x.pdf".into() })).unwrap_err();
    assert_eq!(err.code, "range");
}

/// Enregistrement depuis le moteur : écriture, rechargement sous le même identifiant,
/// et rendu PDFium de l'annotation créée.
#[test]
fn annotations_enregistrees_et_rendues() {
    use crate::annot::geom::Rect;
    use crate::annot::model::{Annot, Kind};
    if !have_fixtures() {
        return;
    }
    let path = std::env::temp_dir().join(format!("aura-moteur-annot-{}.pdf", std::process::id()));
    std::fs::copy(fixture("Rapport Q1 2025.pdf"), &path).unwrap();
    let (e, _) = engine();
    let info = block(e.open(path.clone(), None)).unwrap();
    assert!(block(e.annot_load(info.id)).unwrap().is_empty());

    let render = |e: &Engine| {
        let (tx, rx) = std::sync::mpsc::channel();
        e.render(RenderJob {
            doc: info.id, page: 0, width_px: 300, rotation: 0, annotations: true, forms: true, tile: None, prio: prio::VISIBLE, gen: u64::MAX,
            responder: Box::new(move |o| { let _ = tx.send(match o { RenderOutcome::Ok(b) => Some(b.rgba), _ => None }); }),
        });
        rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap()
    };
    let before = render(&e);
    let a = Annot { id: "aura-test".into(), kind: Kind::Rect, color: "#EF6B6B".into(), width: 4.0, rect: Some(Rect { x: 100.0, y: 100.0, w: 300.0, h: 200.0 }), ..Default::default() };
    let (info2, saved) = block(e.annot_save(info.id, vec![a])).unwrap();
    assert_eq!(info2.id, info.id);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].source, Some(0));
    // L'apparence générée est bien dessinée par PDFium : des pixels rouges apparaissent.
    let after = render(&e);
    let reds = |px: &[u8]| px.chunks_exact(4).filter(|p| p[0] > 200 && p[1] < 140 && p[2] < 140).count();
    assert!(reds(&after) > reds(&before) + 100, "rectangle rouge rendu");
    // Masquée (en cours d'édition), elle disparaît du rendu.
    e.annot_hide(info.id, 0, vec![0]).unwrap();
    assert!(reds(&render(&e)) <= reds(&before) + 5);
    e.close(info.id).unwrap();
    let _ = std::fs::remove_file(&path);
}

#[test]
fn couche_texte() {
    if !have_fixtures() {
        return;
    }
    let (e, _) = engine();
    let info = block(e.open(fixture("Rapport Q1 2025.pdf"), None)).unwrap();
    let words = block(e.page_text(info.id, 0)).unwrap();
    assert!(words.iter().any(|w| w.t == "Stratégie"));
    let links = block(e.page_links(info.id, 1)).unwrap();
    assert_eq!(links.first().and_then(|l| l.page), Some(6));
}

/// Fichiers chiffrés par le module Sécurité : PDFium les ouvre, refuse un
/// mauvais mot de passe et reconnaît les droits (mot de passe utilisateur ou
/// propriétaire) ; un document modifié reste chiffré après rechargement.
#[test]
fn securite_documents_chiffres_dans_pdfium() {
    if !have_fixtures() {
        return;
    }
    use crate::security::{apply, Meta, Perms, SecuritySpec};
    let (e, _) = engine();
    for aes256 in [true, false] {
        let path = std::env::temp_dir().join(format!("aura-secu-pdfium-{aes256}-{}.pdf", std::process::id()));
        std::fs::copy(fixture("Rapport Q1 2025.pdf"), &path).unwrap();
        let spec = SecuritySpec {
            protect: true,
            user_password: "lecture".into(),
            owner_password: "chef".into(),
            aes256,
            encrypt_metadata: true,
            permissions: Perms { print: false, copy: false, edit: false, annotate: true, forms: true, extract: false },
            metadata: Meta { title: "Confidentiel".into(), ..Default::default() },
            pdfa: None,
        };
        apply(&path, None, &spec, &crate::tools::Job::silent()).unwrap();
        assert!(block(e.open(path.clone(), None)).is_err(), "mot de passe exigé");
        assert!(block(e.open(path.clone(), Some("faux".into()))).is_err());
        let info = block(e.open(path.clone(), Some("lecture".into()))).unwrap();
        assert!(info.encrypted);
        assert_eq!(info.page_count, 14);
        assert_eq!(info.metadata.title.as_deref(), Some("Confidentiel"));
        assert!(!info.permissions.copy && !info.permissions.print && info.permissions.annotate);
        // Le propriétaire a tous les droits.
        let again = block(e.reload_with(info.id, Some("chef".into()))).unwrap();
        assert_eq!(again.permissions, Perms::ALL);
        // Rendu d'une page déchiffrée.
        assert!(!block(e.page_text(info.id, 0)).unwrap().is_empty());
    }
}

/// Formulaire rempli puis aplati : PDFium affiche les valeurs comme du
/// contenu de page (texte extractible, plus de champs).
#[test]
fn formulaire_aplati_dans_pdfium() {
    let src = fixture("Demande d’ouverture.pdf");
    if !src.exists() {
        return;
    }
    use crate::forms::{flatten, list, save, FormPlan};
    let (e, _) = engine();
    let bytes = std::fs::read(&src).unwrap();
    let mut fields = list(&lopdf::Document::load_mem(&bytes).unwrap());
    let email = fields.iter_mut().find(|f| f.name == "email").unwrap();
    email.value = "camille@exemple.fr".into();
    let mut extra = email.clone();
    extra.is_new = true;
    extra.label = "E-mail du conseiller".into();
    extra.value = String::new();
    extra.widgets[0].rect = crate::annot::geom::Rect { x: 330.0, y: 280.0, w: 220.0, h: 22.0 };
    let plan = FormPlan { upsert: vec![email.clone(), extra], delete: vec![] };
    let filled = save(bytes, None, &plan).unwrap();
    let flat = flatten(&filled, None).unwrap();
    let path = std::env::temp_dir().join(format!("aura-aplati-{}.pdf", std::process::id()));
    std::fs::write(&path, &flat).unwrap();
    let info = block(e.open(path.clone(), None)).unwrap();
    assert!(!info.has_form, "plus de formulaire");
    let words: String = block(e.page_text(info.id, 0)).unwrap().iter().map(|w| w.t.clone()).collect::<Vec<_>>().join(" ");
    assert!(words.contains("Demande"), "contenu d'origine : {words}");
    assert!(words.contains("Camille") && words.contains("camille@exemple.fr"), "valeurs aplaties : {words}");
    let png = std::env::temp_dir().join(format!("aura-aplati-{}.png", std::process::id()));
    block(e.thumbnail(path, png.clone(), 300)).unwrap();
    assert!(std::fs::metadata(&png).unwrap().len() > 2000);
}

/// Conversion réelle du rapport de test : Word, Excel, texte, images, puis
/// retour en PDF (par Word s'il est installé, sinon par le rendu natif).
#[test]
fn conversion_du_rapport() {
    if !have_fixtures() {
        return;
    }
    use crate::convert::{run, ConvertSpec};
    let (e, _) = engine();
    let dir = std::env::temp_dir().join(format!("aura-conv-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("Rapport.pdf");
    std::fs::copy(fixture("Rapport Q1 2025.pdf"), &src).unwrap();
    let spec = |target: &str| ConvertSpec { target: target.into(), dpi: 72, keep_layout: true, auto_ocr: false, per_page: false, langs: vec![] };
    let files = vec![src.to_string_lossy().to_string()];
    let job = crate::tools::Job::silent();

    let out = run(&e, &files, &spec("txt"), None, &job).unwrap();
    let txt = std::fs::read_to_string(&out[0].path).unwrap();
    assert!(txt.contains("Q1 2025") && txt.contains('\u{c}'), "texte et sauts de page");

    let out = run(&e, &files, &spec("docx"), None, &job).unwrap();
    assert!(out[0].path.ends_with(".docx") && out[0].size > 2000);
    let docx = out[0].path.clone();

    let out = run(&e, &files, &spec("xlsx"), None, &job).unwrap();
    assert!(out[0].path.ends_with(".xlsx"));

    let out = run(&e, &files, &spec("png"), None, &job).unwrap();
    assert_eq!(out[0].pages, 14, "une image par page");
    assert!(std::path::Path::new(&out[0].path).is_dir());

    // Retour en PDF du document Word produit.
    let back = run(&e, &[docx], &spec("pdf"), None, &job).unwrap();
    let doc = lopdf::Document::load(&back[0].path).unwrap();
    assert!(doc.get_pages().len() >= 10, "{} pages", doc.get_pages().len());
}

/// OCR d'une page numérisée inclinée : texte reconnu, PDF recherchable,
/// page redressée ; conversion en texte avec OCR automatique.
#[test]
fn ocr_page_numerisee() {
    let src = fixture("Scan contrat.pdf");
    if !src.exists() {
        return;
    }
    let (e, _) = engine();
    let dir = std::env::temp_dir().join(format!("aura-ocr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Scan contrat.pdf");
    std::fs::copy(&src, &path).unwrap();
    let info = block(e.open(path.clone(), None)).unwrap();
    assert!(info.scanned);
    let spec = crate::ocr::job::OcrSpec { langs: vec!["fra".into()], searchable: true, deskew: true, all_pages: false, in_place: false };
    let out = crate::ocr::job::run(&e, info.id, &spec, &crate::tools::Job::silent()).unwrap();
    assert!(out.path.ends_with("Scan contrat (OCR).pdf"));
    let ocr = block(e.open(out.path.clone().into(), None)).unwrap();
    let text: String = block(e.page_text(ocr.id, 0)).unwrap().iter().map(|w| w.t.clone()).collect::<Vec<_>>().join(" ");
    for w in ["Contrat", "prestation", "Vauclair", "règlement", "Bruxelles"] {
        assert!(text.contains(w), "« {w} » absent de : {text}");
    }
    let raw = std::fs::read(&out.path).unwrap();
    assert!(raw.windows(2).any(|w| w == b"cm"), "page redressée");
    // Conversion en texte d'un document numérisé : l'OCR automatique est appliqué.
    let spec = crate::convert::ConvertSpec { target: "txt".into(), dpi: 300, keep_layout: false, auto_ocr: true, per_page: false, langs: vec!["fra".into()] };
    let res = crate::convert::run(&e, &[path.to_string_lossy().to_string()], &spec, Some(&crate::ocr::convert_ocr), &crate::tools::Job::silent()).unwrap();
    let txt = std::fs::read_to_string(&res[0].path).unwrap();
    assert!(txt.contains("Atelier Nord"), "{txt}");
}
