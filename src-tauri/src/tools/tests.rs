//! Tests des outils sur les PDF de `tests/fixtures` et des documents construits.

use std::path::PathBuf;

use lopdf::{dictionary, Document, Object, Stream};

use super::compress::{analyze, compress, CompressOptions};
use super::inspect::{inspect, outline};
use super::merge::{merge, MergeOptions};
use super::split::{by_size, split, Part};
use super::Job;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("tests").join("fixtures").join(name)
}

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("aura-tools-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn fusion_avec_signets_sommaire_et_a4() {
    let (a, b) = (fixture("Rapport Q1 2025.pdf"), fixture("Paysage.pdf"));
    if !a.exists() {
        return;
    }
    let dir = tmpdir("fusion");
    let out = dir.join("Dossier complet.pdf");
    let r = merge(&[a, b], &MergeOptions { bookmarks: true, toc: true, a4: true }, &out, &Job::silent()).unwrap();
    // 14 + 4 pages + 1 page de sommaire.
    assert_eq!(r.pages, 19);
    let doc = Document::load(&out).unwrap();
    let marks = outline(&doc);
    assert_eq!(marks.iter().map(|m| m.title.as_str()).collect::<Vec<_>>(), vec!["Sommaire", "Rapport Q1 2025", "Paysage"]);
    assert!(marks[1].children.len() >= 7, "signets du fichier repris sous son signet");
    assert!(marks.iter().all(|m| m.page.is_some()));
    // Pages au format A4, paysage conservé.
    let pages: Vec<_> = doc.get_pages().values().copied().collect();
    let mb = |i: usize| doc.get_dictionary(pages[i]).unwrap().get(b"MediaBox").unwrap().as_array().unwrap().iter().map(|o| o.as_float().unwrap()).collect::<Vec<_>>();
    assert!((mb(1)[3] - 841.89).abs() < 0.1);
    assert!((mb(16)[2] - 841.89).abs() < 0.1, "page paysage en A4 paysage");
    // Le sommaire contient des liens vers chaque fichier.
    let toc = doc.get_dictionary(pages[0]).unwrap();
    assert_eq!(toc.get(b"Annots").unwrap().as_array().unwrap().len(), 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn division_par_plages_et_par_taille() {
    let src = fixture("Rapport Q1 2025.pdf");
    if !src.exists() {
        return;
    }
    let dir = tmpdir("division");
    let parts = vec![Part { start: 0, end: 3, name: "Rapport — partie 1".into() }, Part { start: 4, end: 13, name: "Rapport — partie 2".into() }];
    let outs = split(&src, &parts, &dir, &Job::silent()).unwrap();
    assert_eq!(outs.iter().map(|o| o.pages).collect::<Vec<_>>(), vec![4, 10]);
    let second = inspect(std::path::Path::new(&outs[1].path)).unwrap();
    assert_eq!(second.pages, 10);
    // Seuls les signets des pages conservées subsistent, avec leur bonne page.
    assert!(second.marks.iter().all(|m| m.page < 10));
    assert!(second.marks.iter().any(|m| m.title == "Conformité"));
    // Fichier existant : pas d'écrasement.
    let again = split(&src, &parts[..1], &dir, &Job::silent()).unwrap();
    assert!(again[0].path.ends_with("partie 1 (2).pdf"), "{}", again[0].path);

    let sens = fixture("Données sensibles.pdf");
    if sens.exists() {
        let all = by_size(&sens, 1_000_000).unwrap();
        assert_eq!(all, vec![[0, 2]]);
        // L'image (≈ 21 Ko) fait passer la page 2 au-delà d'une limite de 15 Ko.
        let small = by_size(&sens, 15_000).unwrap();
        assert!(small.len() >= 2, "{small:?}");
        assert_eq!(small.first().unwrap()[0], 0);
        assert_eq!(small.last().unwrap()[1], 2);
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// Document d'une page avec une grande image (2000 × 1500 px) affichée sur 200 × 150 pt.
fn heavy_doc(path: &std::path::Path) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let (w, h) = (2000u32, 1500u32);
    let mut px = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            // Dégradé bruité : compresse mal sans perte, comme une photo.
            let n = ((x * 7919 + y * 104_729) % 37) as u8;
            px.extend_from_slice(&[(x / 8) as u8 ^ n, (y / 6) as u8, 120u8.wrapping_add(n)]);
        }
    }
    let mut img = Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8 }, px);
    img.compress().unwrap();
    let img_id = doc.add_object(img);
    let content = doc.add_object(Stream::new(dictionary! {}, b"q 200 0 0 150 100 500 cm /Im1 Do Q".to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        "Contents" => content, "Resources" => dictionary! { "XObject" => dictionary! { "Im1" => img_id } },
    });
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![Object::Reference(page)], "Count" => 1 }));
    let info = doc.add_object(dictionary! { "Author" => Object::string_literal("Jeanne Martin") });
    let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", cat);
    doc.trailer.set("Info", info);
    doc.save(path).unwrap();
}

#[test]
fn analyse_et_compression() {
    let dir = tmpdir("compression");
    let src = dir.join("Lourd.pdf");
    heavy_doc(&src);
    let a = analyze(&src).unwrap();
    assert_eq!(a.image_list.len(), 1);
    let ppi = a.image_list[0].ppi.unwrap();
    assert!((ppi - 720.0).abs() < 1.0, "résolution affichée : {ppi}");
    assert!(a.images > a.total / 2, "les images dominent : {a:?}");
    assert_eq!(a.images + a.fonts + a.content + a.meta, a.total);

    let out = dir.join("Lourd (compressé).pdf");
    let opts = CompressOptions { dpi: 150, quality: 75, gray: false, subset: true, unembed: true, meta: true, thumbs: true, attach: true, compact: true };
    let r = compress(&src, &opts, &out, &Job::silent()).unwrap();
    assert!(r.size * 10 < a.total, "{} → {}", a.total, r.size);
    let b = analyze(&out).unwrap();
    let img = &b.image_list[0];
    assert!(img.jpeg && (img.w as i32 - 417).abs() <= 1, "image rééchantillonnée à 150 ppp : {} px", img.w);
    let doc = Document::load(&out).unwrap();
    assert!(!doc.trailer.has(b"Info"), "métadonnées supprimées");
    assert_eq!(doc.get_pages().len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn compression_du_rapport_reste_lisible() {
    let src = fixture("Rapport Q1 2025.pdf");
    if !src.exists() {
        return;
    }
    let dir = tmpdir("rapport");
    let out = dir.join("r.pdf");
    let opts = CompressOptions { dpi: 150, quality: 75, gray: false, subset: true, unembed: false, meta: false, thumbs: true, attach: false, compact: true };
    compress(&src, &opts, &out, &Job::silent()).unwrap();
    let info = inspect(&out).unwrap();
    assert_eq!(info.pages, 14);
    assert!(info.marks.len() >= 7, "signets conservés");
    let _ = std::fs::remove_dir_all(dir);
}
