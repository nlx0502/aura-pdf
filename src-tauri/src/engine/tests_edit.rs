//! Tests d'intégration du module Éditer (caviardage, contenu, calques) avec PDFium.

use std::path::PathBuf;

use super::tests::{block, engine, fixture, have_fixtures, wait_print_done, PRINT_LOCK};
use super::*;

fn all_text(e: &Engine, doc: u32, pages: u32) -> String {
    (0..pages)
        .map(|p| block(e.page_text(doc, p)).unwrap().into_iter().map(|w| w.t).collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
}

fn raw_streams(path: &std::path::Path) -> String {
    let d = lopdf::Document::load(path).unwrap();
    d.objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .map(|s| String::from_utf8_lossy(&s.decompressed_content().unwrap_or_else(|_| s.content.clone())).to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn tmp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("aura-{}-{name}", std::process::id()))
}

fn plan_with(pages: u32, hits: &[RedactHit]) -> crate::edit::Plan {
    let pages = (0..pages)
        .map(|i| crate::edit::PlanPage {
            src: crate::edit::PageSrc::Orig { index: i },
            rot: 0,
            crop: false,
            redact: hits
                .iter()
                .filter(|h| h.page == i)
                .flat_map(|h| h.rects.iter().map(|r| crate::annot::geom::Rect { x: r.x - 1.0, y: r.y - 1.0, w: r.w + 2.0, h: r.h + 2.0 }))
                .collect(),
            links: vec![],
        })
        .collect();
    crate::edit::Plan { pages, header: None, numbering: None, watermark: None, crop: None, clean_metadata: false }
}

fn x_of(ws: &[TextWord], word: &str) -> f32 {
    ws.iter().find(|w| w.t == word).map(|w| w.x).unwrap_or_else(|| panic!("« {word} » absent"))
}

#[test]
fn caviardage_bout_en_bout() {
    if !fixture("Données sensibles.pdf").exists() {
        return;
    }
    let (e, _) = engine();
    let info = block(e.open(fixture("Données sensibles.pdf"), None)).unwrap();
    let hits = block(e.redact_find(info.id, vec!["iban".into(), "email".into(), "tel".into()], vec![])).unwrap();
    let kinds: Vec<(u32, &str, &str)> = hits.iter().map(|h| (h.page, h.kind.as_str(), h.text.as_str())).collect();
    assert_eq!(hits.len(), 5, "{kinds:?}");
    let out = tmp("caviarde.pdf");
    let r = block(e.edit_apply(info.id, plan_with(info.page_count, &hits), Some(out.clone()))).unwrap();
    assert!(r.is_none(), "enregistrement dans un autre fichier");

    let o = block(e.open(out.clone(), None)).unwrap();
    let text = all_text(&e, o.id, o.page_count);
    for secret in ["FR76", "0112", "jeanne.martin", "06 12", "paul.durand", "42 68"] {
        assert!(!text.contains(secret), "« {secret} » encore lisible :\n{text}");
    }
    for kept in ["virement", "Contact", "Référent", "Ligne directe", "lundi", "Synthèse"] {
        assert!(text.contains(kept), "« {kept} » a disparu :\n{text}");
    }
    assert!(block(e.redact_find(o.id, vec!["iban".into(), "email".into(), "tel".into()], vec![])).unwrap().is_empty());
    // Rien dans les flux du fichier (texte, formulaires), ni dans les métadonnées.
    let raw = raw_streams(&out);
    assert!(!raw.contains("jeanne") && !raw.contains("durand"), "données présentes dans le fichier");
    assert!(o.metadata.author.is_none(), "métadonnées conservées");
    // La fin de la ligne crénelée garde sa position.
    let before = block(e.page_text(info.id, 0)).unwrap();
    let after = block(e.page_text(o.id, 0)).unwrap();
    assert!((x_of(&before, "lundi") - x_of(&after, "lundi")).abs() < 0.5);
    e.close(o.id).unwrap();
    e.close(info.id).unwrap();
    let _ = std::fs::remove_file(out);
}

/// Police composite (Type0, Identity-H, largeurs /W) : le cas des polices
/// TrueType intégrées par les traitements de texte.
#[test]
fn caviardage_police_composite() {
    use lopdf::{dictionary, Object, Stream};
    let mut doc = lopdf::Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let cmap = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CMapName /Aura def 1 begincodespacerange <0000> <FFFF> endcodespacerange 1 beginbfrange <0000> <FFFF> <0000> endbfrange endcmap CMapName currentdict /CMap defineresource pop end end";
    let tu = doc.add_object(Stream::new(dictionary! {}, cmap.to_vec()));
    let desc = doc.add_object(dictionary! {
        "Type" => "FontDescriptor", "FontName" => "ArialMT", "Flags" => 32,
        "FontBBox" => vec![(-665).into(), (-325).into(), 2000.into(), 1006.into()],
        "ItalicAngle" => 0, "Ascent" => 905, "Descent" => (-212), "CapHeight" => 716, "StemV" => 80,
    });
    let cid = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => "ArialMT",
        "CIDSystemInfo" => dictionary! { "Registry" => Object::string_literal("Adobe"), "Ordering" => Object::string_literal("Identity"), "Supplement" => 0 },
        "FontDescriptor" => desc, "DW" => 600,
        "W" => vec![32.into(), Object::Array(vec![278.into()]), 97.into(), 122.into(), 520.into()],
    });
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "ArialMT", "Encoding" => "Identity-H",
        "DescendantFonts" => vec![Object::Reference(cid)], "ToUnicode" => tu,
    });
    let text = "public secret visible";
    let hex: String = text.chars().map(|c| format!("{:04X}", c as u32)).collect();
    let content = doc.add_object(Stream::new(dictionary! {}, format!("BT /F0 14 Tf 72 700 Td <{hex}> Tj ET").into_bytes()));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
        "Contents" => content, "Resources" => dictionary! { "Font" => dictionary! { "F0" => font } },
    });
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![Object::Reference(page)], "Count" => 1 }));
    let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", cat);
    let src = tmp("type0.pdf");
    doc.save(&src).unwrap();

    let (e, _) = engine();
    let info = block(e.open(src.clone(), None)).unwrap();
    let hits = block(e.redact_find(info.id, vec![], vec!["secret".into()])).unwrap();
    assert_eq!(hits.len(), 1);
    let out = tmp("type0-caviarde.pdf");
    block(e.edit_apply(info.id, plan_with(1, &hits), Some(out.clone()))).unwrap();
    let o = block(e.open(out.clone(), None)).unwrap();
    let t = all_text(&e, o.id, 1);
    assert!(!t.contains("secret") && t.contains("public") && t.contains("visible"), "{t}");
    let before = block(e.page_text(info.id, 0)).unwrap();
    let after = block(e.page_text(o.id, 0)).unwrap();
    assert!((x_of(&before, "visible") - x_of(&after, "visible")).abs() < 0.5);
    e.close(o.id).unwrap();
    e.close(info.id).unwrap();
    let _ = std::fs::remove_file(src);
    let _ = std::fs::remove_file(out);
}

#[test]
fn edition_du_contenu_et_enregistrement() {
    if !have_fixtures() {
        return;
    }
    let (e, _) = engine();
    let path = tmp("contenu.pdf");
    std::fs::copy(fixture("Rapport Q1 2025.pdf"), &path).unwrap();
    let info = block(e.open(path.clone(), None)).unwrap();
    let items = block(e.content_list(info.id, 0)).unwrap();
    let title = items.iter().find(|i| i.kind == "text" && i.text.contains("Stratégie")).expect("titre repéré").clone();
    assert!(title.bold && title.size > 20.0, "{title:?}");
    let para = items.iter().find(|i| i.kind == "text" && i.text.starts_with("L’exercice")).expect("paragraphe repéré");
    assert!(para.ids.len() >= 3, "paragraphe de plusieurs lignes : {para:?}");
    assert!(para.text.contains("adoption des architectures"), "lignes jointes : {}", para.text);

    let spec = content::TextSpec {
        text: "Q1 2025 — Bilan".into(),
        rect: title.rect,
        font: title.font.into(),
        bold: title.bold,
        italic: title.italic,
        size: title.size,
        color: title.color.clone(),
        align: title.align,
        line_height: title.line_height,
        baseline: Some(title.baseline),
    };
    let op = content::ContentOp::ReplaceText { page: 0, ids: title.ids.clone(), spec };
    block(e.content_apply(info.id, vec![op.clone()], false)).unwrap();
    let after = block(e.content_list(info.id, 0)).unwrap();
    let t2 = after.iter().find(|i| i.text.contains("Bilan")).expect("nouveau titre");
    assert!((t2.baseline - title.baseline).abs() < 0.5, "même ligne de base : {} / {}", t2.baseline, title.baseline);
    // Annuler : on repart du fichier.
    block(e.content_apply(info.id, vec![], true)).unwrap();
    assert!(block(e.content_list(info.id, 0)).unwrap().iter().any(|i| i.text.contains("Stratégie")));
    // Rétablir puis enregistrer dans le fichier lui-même.
    block(e.content_apply(info.id, vec![op], true)).unwrap();
    let fresh = block(e.edit_apply(info.id, plan_with(info.page_count, &[]), None)).unwrap().expect("document rouvert");
    assert_ne!(fresh.id, info.id);
    let t = all_text(&e, fresh.id, 1);
    assert!(t.contains("Bilan") && !t.contains("innovation"), "{t}");
    assert!(t.contains("architectures"), "le reste de la page est intact");
    e.close(fresh.id).unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn calques_affichage() {
    if !fixture("Plan avec calques.pdf").exists() {
        return;
    }
    let (e, _) = engine();
    let info = block(e.open(fixture("Plan avec calques.pdf"), None)).unwrap();
    let layers = block(e.layers(info.id)).unwrap();
    assert_eq!(layers.iter().map(|l| (l.name.as_str(), l.visible)).collect::<Vec<_>>(), vec![("Cotes", true), ("Mobilier", false)]);
    // (L'extraction de texte ignore les calques : on vérifie le rendu.)
    // Meuble vert du calque « Mobilier » en (120..240, 120..180) ; trait rouge des cotes en y = 60.
    let mobilier = (180, 445);
    let cote = (300, 535);
    let green = |p: [u8; 4]| p[1] > 180 && p[0] < 120;
    let red = |p: [u8; 4]| p[0] > 180 && p[1] < 120;
    assert!(!green(pixel(&e, info.id, mobilier)), "calque Mobilier masqué par défaut");
    assert!(red(pixel(&e, info.id, cote)), "cotes visibles par défaut");
    block(e.layers_set(info.id, vec![layers[0].id.clone()])).unwrap();
    assert!(green(pixel(&e, info.id, mobilier)), "Mobilier affiché");
    assert!(!red(pixel(&e, info.id, cote)), "Cotes masquées");
    e.close(info.id).unwrap();
}

/// Couleur d'un pixel de la page 1 rendue à 1 pixel par point.
fn pixel(e: &Engine, doc: u32, (x, y): (u32, u32)) -> [u8; 4] {
    let (tx, rx) = std::sync::mpsc::channel();
    e.render(RenderJob {
        doc,
        page: 0,
        width_px: 842,
        rotation: 0,
        annotations: true,
        forms: true,
        tile: None,
        prio: prio::VISIBLE,
        gen: u64::MAX / 2,
        responder: Box::new(move |o| {
            let _ = tx.send(match o {
                RenderOutcome::Ok(b) => Some(b),
                _ => None,
            });
        }),
    });
    let b = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap().expect("rendu");
    let i = ((y * b.width + x) * 4) as usize;
    [b.rgba[i], b.rgba[i + 1], b.rgba[i + 2], b.rgba[i + 3]]
}

#[test]
fn contenu_en_police_du_systeme() {
    if !have_fixtures() || !crate::sysfonts::list().iter().any(|f| f.family == "Arial") {
        return;
    }
    let (e, _) = engine();
    let path = tmp("contenu-arial.pdf");
    std::fs::copy(fixture("Rapport Q1 2025.pdf"), &path).unwrap();
    let info = block(e.open(path.clone(), None)).unwrap();
    let items = block(e.content_list(info.id, 0)).unwrap();
    let title = items.iter().find(|i| i.text.contains("Stratégie")).unwrap().clone();
    let spec = content::TextSpec {
        text: "Synthèse Œuvre".into(),
        rect: title.rect,
        font: "Arial".into(),
        bold: true,
        italic: false,
        size: title.size,
        color: title.color.clone(),
        align: 0,
        line_height: 1.2,
        baseline: Some(title.baseline),
    };
    block(e.content_apply(info.id, vec![content::ContentOp::ReplaceText { page: 0, ids: title.ids.clone(), spec }], false)).unwrap();
    let after = block(e.content_list(info.id, 0)).unwrap();
    let t = after.iter().find(|i| i.text.contains("Synthèse")).expect("texte réécrit");
    assert_eq!(t.font, "Arial", "police reconnue parmi les polices installées");
    assert!(t.bold);
    let fresh = block(e.edit_apply(info.id, plan_with(info.page_count, &[]), None)).unwrap().unwrap();
    assert!(all_text(&e, fresh.id, 1).contains("Synthèse Œuvre"));
    e.close(fresh.id).unwrap();
    let _ = std::fs::remove_file(path);
}

#[test]
fn compression_rendu_fidele() {
    use lopdf::{dictionary, Object, Stream};
    // Image 1600 × 1200 (rouge à gauche, bleu à droite) affichée sur 400 × 300 pt.
    let (w, h) = (1600u32, 1200u32);
    let mut px = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            // Bruit sur deux composantes : image « photographique », recodée en JPEG.
            // Composantes bruitées indépendamment : beaucoup de couleurs, comme une photo.
            let h = |k: u32| ((x.wrapping_mul(k) ^ y.wrapping_mul(k.rotate_left(7))).wrapping_mul(2_654_435_761) >> 24) as u8;
            let (a, b, c) = (h(7919) / 5, h(104_729) / 3, h(31) / 5);
            px.extend_from_slice(&if x < w / 2 { [200 + a, b, c] } else { [c, b, 200 + a] });
        }
    }
    let mut doc = lopdf::Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut img = Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8 }, px);
    img.compress().unwrap();
    let img_id = doc.add_object(img);
    let content = doc.add_object(Stream::new(dictionary! {}, b"q 400 0 0 300 100 400 cm /Im1 Do Q".to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
        "Contents" => content, "Resources" => dictionary! { "XObject" => dictionary! { "Im1" => img_id } },
    });
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![Object::Reference(page)], "Count" => 1 }));
    let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", cat);
    let src = tmp("lourd.pdf");
    doc.save(&src).unwrap();
    let out = tmp("lourd-compresse.pdf");
    let opts = crate::tools::compress::CompressOptions { dpi: 96, quality: 60, gray: false, subset: true, unembed: true, meta: true, thumbs: true, attach: true, compact: true };
    let r = crate::tools::compress::compress(&src, &opts, &out, &crate::tools::Job::silent()).unwrap();
    let before = std::fs::metadata(&src).unwrap().len();
    assert!(r.size * 2 < before, "{} → {}", before, r.size);
    let a = crate::tools::compress::analyze(&out).unwrap();
    assert!(a.image_list[0].jpeg, "image recodée en JPEG");

    let (e, _) = engine();
    let info = block(e.open(out.clone(), None)).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    e.render(RenderJob {
        doc: info.id, page: 0, width_px: 600, rotation: 0, annotations: true, forms: true, tile: None, prio: prio::VISIBLE, gen: u64::MAX / 2,
        responder: Box::new(move |o| {
            let _ = tx.send(match o {
                RenderOutcome::Ok(b) => Some(b),
                _ => None,
            });
        }),
    });
    let b = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap().expect("rendu");
    let at = |x: u32, y: u32| {
        let i = ((y * b.width + x) * 4) as usize;
        [b.rgba[i], b.rgba[i + 1], b.rgba[i + 2]]
    };
    // Moitié gauche rouge, moitié droite bleue (y d'affichage = 800 - 550 = 250).
    let l = at(200, 250);
    let rr = at(400, 250);
    assert!(l[0] > 170 && l[2] < 90, "gauche : {l:?}");
    assert!(rr[2] > 170 && rr[0] < 90, "droite : {rr:?}");
    e.close(info.id).unwrap();
    let _ = std::fs::remove_file(src);
    let _ = std::fs::remove_file(out);
}

/// L'impression respecte les calques affichés (copie avec visibilité appliquée).
#[test]
fn impression_selon_les_calques() {
    if !fixture("Plan avec calques.pdf").exists() {
        return;
    }
    let _guard = PRINT_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (e, events) = engine();
    let info = block(e.open(fixture("Plan avec calques.pdf"), None)).unwrap();
    let layers = block(e.layers(info.id)).unwrap();
    // Cotes masquées, Mobilier affiché.
    block(e.layers_set(info.id, vec![layers[0].id.clone()])).unwrap();
    let out = std::env::temp_dir().join(format!("aura-calques-impr-{}.pdf", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let settings = crate::print::layout::PrintSettings::default();
    block(e.print_start(info.id, 903, settings, 0.0, crate::print::PrintTarget::Pdf { path: out.to_string_lossy().into() })).unwrap();
    let done = wait_print_done(&events, 903);
    assert_eq!(done["ok"], true, "{done}");
    let printed = block(e.open(out, None)).unwrap();
    let green = |p: [u8; 4]| p[1] > 180 && p[0] < 120;
    let red = |p: [u8; 4]| p[0] > 180 && p[1] < 120;
    assert!(green(pixel(&e, printed.id, (180, 445))), "Mobilier imprimé");
    assert!(!red(pixel(&e, printed.id, (300, 535))), "Cotes non imprimées");
}
