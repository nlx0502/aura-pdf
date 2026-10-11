//! Tests du module Éditer sur des documents construits en mémoire et sur les
//! PDF de `tests/fixtures`.

use lopdf::content::Content;
use lopdf::{dictionary, Document, Object, Stream};

use super::*;

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("tests").join("fixtures").join(name)
}

fn orig(i: u32) -> PlanPage {
    PlanPage { src: PageSrc::Orig { index: i }, rot: 0, crop: false, redact: vec![], links: vec![] }
}

fn plan(pages: Vec<PlanPage>) -> Plan {
    Plan { pages, header: None, numbering: None, watermark: None, crop: None, clean_metadata: false }
}

/// Document de `n` pages : chaque page affiche « Page k » en Helvetica.
fn sample(n: usize) -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" });
    let mut kids = Vec::new();
    for k in 1..=n {
        let c = doc.add_object(Stream::new(dictionary! {}, format!("BT /F1 12 Tf 72 700 Td (Page {k}) Tj ET").into_bytes()));
        let p = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => c,
        });
        kids.push(Object::Reference(p));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => kids, "Count" => n as i64,
            // Attributs hérités : la réorganisation doit les recopier dans chaque page.
            "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        }),
    );
    let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", cat);
    doc
}

fn page_text(doc: &Document, index: usize) -> String {
    let id = *doc.get_pages().values().nth(index).unwrap();
    let content = doc.get_page_content(id);
    let ops = Content::decode(&content).map(|c| c.operations).unwrap_or_default();
    let mut s = String::new();
    for op in ops {
        if op.operator == "Tj" || op.operator == "TJ" {
            let items = match &op.operands[0] {
                Object::Array(a) => a.clone(),
                o => vec![o.clone()],
            };
            for it in items {
                if let Object::String(b, _) = it {
                    s.push_str(&String::from_utf8_lossy(&b));
                }
            }
        }
    }
    s
}

fn rebuild(mut doc: Document, p: &Plan) -> Document {
    let bytes = build(&mut doc, p).unwrap();
    Document::load_mem(&bytes).unwrap()
}

#[test]
fn reordonner_supprimer_pivoter() {
    let doc = sample(4);
    let mut p = plan(vec![orig(3), orig(0), orig(2)]);
    p.pages[0].rot = 1;
    let out = rebuild(doc, &p);
    assert_eq!(out.get_pages().len(), 3);
    assert_eq!(page_text(&out, 0), "Page 4");
    assert_eq!(page_text(&out, 1), "Page 1");
    assert_eq!(page_text(&out, 2), "Page 3");
    let first = out.get_dictionary(*out.get_pages().values().next().unwrap()).unwrap();
    assert_eq!(first.get(b"Rotate").unwrap().as_i64().unwrap(), 90);
    // Attributs hérités recopiés : chaque page a sa MediaBox et ses ressources.
    assert!(first.has(b"MediaBox") && first.has(b"Resources"));
    // La page 2 supprimée n'est plus dans le fichier.
    let all = format!("{:?}", out.objects.values().filter_map(|o| o.as_stream().ok()).map(|s| String::from_utf8_lossy(&s.decompressed_content().unwrap_or(s.content.clone())).to_string()).collect::<Vec<_>>());
    assert!(!all.contains("Page 2"), "contenu de la page supprimée encore présent");
}

#[test]
fn pages_vierges_et_import() {
    let mut other = sample(2);
    let path = std::env::temp_dir().join(format!("aura-import-{}.pdf", std::process::id()));
    other.save(&path).unwrap();
    let doc = sample(1);
    let p = plan(vec![
        orig(0),
        PlanPage { src: PageSrc::Blank { w: 300.0, h: 400.0 }, ..orig(0) },
        PlanPage { src: PageSrc::File { path: path.to_string_lossy().into(), index: 1 }, ..orig(0) },
    ]);
    let out = rebuild(doc, &p);
    assert_eq!(out.get_pages().len(), 3);
    assert_eq!(page_text(&out, 2), "Page 2", "page importée");
    let blank = out.get_dictionary(*out.get_pages().values().nth(1).unwrap()).unwrap();
    let mb = blank.get(b"MediaBox").unwrap().as_array().unwrap();
    assert_eq!(mb[2].as_float().unwrap(), 300.0);
    let _ = std::fs::remove_file(path);
}

#[test]
fn liens_vers_page_supprimee_neutralises() {
    let mut doc = sample(3);
    let pages: Vec<_> = doc.get_pages().values().copied().collect();
    let link = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Link", "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
        "Dest" => vec![Object::Reference(pages[2]), "Fit".into()],
    });
    doc.get_dictionary_mut(pages[0]).unwrap().set("Annots", vec![Object::Reference(link)]);
    let out = rebuild(doc, &plan(vec![orig(0), orig(1)]));
    assert_eq!(out.get_pages().len(), 2);
    let l = links::list(&out, 0);
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].page, None, "destination supprimée");
}

#[test]
fn rognage_et_ajouts() {
    let doc = sample(2);
    let mut p = plan(vec![orig(0), orig(1)]);
    p.crop = Some(Margins { top: 50.0, bottom: 20.0, left: 30.0, right: 10.0 });
    p.pages[0].crop = true;
    p.numbering = Some(Numbering { format: "{n} / {total}".into(), position: "br".into(), size: 9.0, start: 1, color: "#000000".into() });
    p.header = Some(HeaderFooter { header: "Dossier d’audit".into(), footer: String::new(), size: 9.0, color: "#333333".into() });
    p.watermark = Some(Watermark { text: "CONFIDENTIEL".into(), opacity: 0.2, color: "#6E6AF6".into(), angle: 35.0 });
    let out = rebuild(doc, &p);
    let ids: Vec<_> = out.get_pages().values().copied().collect();
    let p0 = out.get_dictionary(ids[0]).unwrap();
    let cb: Vec<f32> = p0.get(b"CropBox").unwrap().as_array().unwrap().iter().map(|o| o.as_float().unwrap()).collect();
    assert_eq!(cb, vec![30.0, 20.0, 590.0, 750.0]);
    assert!(!out.get_dictionary(ids[1]).unwrap().has(b"CropBox"));
    let t1 = page_text(&out, 1);
    assert!(t1.contains("Page 2") && t1.contains("2 / 2") && t1.contains("CONFIDENTIEL") && t1.contains("Dossier d") && t1.contains("audit"), "{t1}");
    // Le contenu d'origine est intact et encadré par q … Q.
    let content = String::from_utf8_lossy(&out.get_page_content(ids[1])).to_string();
    assert!(content.starts_with("q\n") && content.contains("/Artifact"), "{content}");
}

#[test]
fn liens_ajout_modification_suppression() {
    let doc = sample(1);
    let mut p = plan(vec![orig(0)]);
    p.pages[0].links = vec![
        LinkEdit::Add { rect: Rect { x: 72.0, y: 90.0, w: 100.0, h: 14.0 }, uri: "audit.exemple.fr".into() },
    ];
    let out = rebuild(doc, &p);
    let l = links::list(&out, 0);
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].uri.as_deref(), Some("https://audit.exemple.fr"));
    assert!((l[0].rect.y - 90.0).abs() < 0.01, "coordonnées d'affichage conservées");
    let mut p2 = plan(vec![orig(0)]);
    p2.pages[0].links = vec![LinkEdit::Update { index: l[0].index, uri: "https://autre.fr".into() }];
    let out2 = rebuild(out, &p2);
    assert_eq!(links::list(&out2, 0)[0].uri.as_deref(), Some("https://autre.fr"));
    let mut p3 = plan(vec![orig(0)]);
    p3.pages[0].links = vec![LinkEdit::Delete { index: 0 }];
    assert!(links::list(&rebuild(out2, &p3), 0).is_empty());
}

#[test]
fn caviardage_glyphe_par_glyphe() {
    let doc = sample(1);
    // « Page 1 » à x = 72, police 12 : « Page » occupe 72 → ~102. On caviarde « Page ».
    // Points d'affichage : y = 800 - 700 - 12 → ligne entre y ≈ 88 et 103.
    let mut p = plan(vec![orig(0)]);
    p.pages[0].redact = vec![Rect { x: 70.0, y: 86.0, w: 33.0, h: 18.0 }];
    let out = rebuild(doc, &p);
    let t = page_text(&out, 0);
    assert!(!t.contains("Page"), "texte caviardé encore présent : {t}");
    assert!(t.contains('1'), "le reste de la ligne doit subsister : {t}");
    let content = String::from_utf8_lossy(&out.get_page_content(*out.get_pages().values().next().unwrap())).to_string();
    assert!(content.contains(" re f"), "bande noire absente");
    // Le « 1 » garde sa position : déplacement compensé dans TJ.
    assert!(content.contains("TJ"), "{content}");
    // Métadonnées supprimées.
    assert!(!out.trailer.has(b"Info"));
}

#[test]
fn caviardage_image() {
    let mut doc = sample(1);
    let pid = *doc.get_pages().values().next().unwrap();
    // Image 4×4 grise, plein cadre sur 100×100 pt en (100, 100).
    let img = doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 4, "Height" => 4, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 },
        vec![200u8; 16],
    ));
    let content = doc.add_object(Stream::new(dictionary! {}, b"q 100 0 0 100 100 100 cm /Im1 Do Q".to_vec()));
    let res = dictionary! { "XObject" => dictionary! { "Im1" => img } };
    let d = doc.get_dictionary_mut(pid).unwrap();
    d.set("Contents", content);
    d.set("Resources", res);
    let mut p = plan(vec![orig(0)]);
    // Coin bas-gauche de l'image (espace utilisateur 100..150) → affichage y = 800-150..800-100.
    p.pages[0].redact = vec![Rect { x: 100.0, y: 650.0, w: 50.0, h: 50.0 }];
    let out = rebuild(doc, &p);
    let images: Vec<Vec<u8>> = out
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter(|s| s.dict.get(b"Subtype").and_then(|n| n.as_name()).ok() == Some(b"Image".as_slice()))
        .map(|s| s.decompressed_content().unwrap_or(s.content.clone()))
        .collect();
    assert_eq!(images.len(), 1, "l'image d'origine ne doit plus être dans le fichier");
    let px = &images[0];
    // Rangées du bas (lignes 2-3), colonnes de gauche : effacées ; coin haut-droit intact.
    assert_eq!(px[3 * 4], 0);
    assert_eq!(px[3], 200);
}

#[test]
fn calques() {
    let mut doc = sample(1);
    let a = doc.add_object(dictionary! { "Type" => "OCG", "Name" => Object::string_literal("Cotes") });
    let b = doc.add_object(dictionary! { "Type" => "OCG", "Name" => Object::string_literal("Annotations techniques") });
    let props = dictionary! {
        "OCGs" => vec![Object::Reference(a), Object::Reference(b)],
        "D" => dictionary! { "Order" => vec![Object::Reference(b), Object::Reference(a)], "OFF" => vec![Object::Reference(a)] },
    };
    doc.catalog_mut().unwrap().set("OCProperties", props);
    let list = layers::list(&doc);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].name, "Annotations techniques");
    assert!(list[0].visible && !list[1].visible);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    let out = layers::with_visibility(bytes, None, &[list[0].id.clone()]).unwrap();
    let back = layers::list(&Document::load_mem(&out).unwrap());
    assert!(!back[0].visible && back[1].visible);
}

#[test]
fn rapport_complet() {
    let src = fixture("Rapport Q1 2025.pdf");
    if !src.exists() {
        return;
    }
    let doc = load(&src, None).unwrap();
    let n = doc.get_pages().len() as u32;
    // Dernière page en tête, page 3 supprimée, page 2 caviardée en partie.
    let mut pages: Vec<PlanPage> = (0..n).filter(|&i| i != 2).map(orig).collect();
    pages.rotate_right(1);
    let p2 = pages.iter_mut().find(|p| matches!(p.src, PageSrc::Orig { index: 1 })).unwrap();
    p2.redact = vec![Rect { x: 50.0, y: 40.0, w: 200.0, h: 20.0 }];
    let mut p = plan(pages);
    p.numbering = Some(Numbering { format: "Page {n} sur {total}".into(), position: "bc".into(), size: 9.0, start: 1, color: "#000".into() });
    let out = rebuild(doc, &p);
    assert_eq!(out.get_pages().len() as u32, n - 1);
    // Signets et pièce jointe conservés.
    assert!(out.catalog().unwrap().has(b"Outlines"));
    assert!(out.catalog().unwrap().has(b"Names"));
    assert!(page_text(&out, 0).contains(&format!("Page 1 sur {}", n - 1)));
}
