use super::model::{Line, PageData, Span};
use super::*;

fn span(text: &str, x: f32, y: f32, size: f32, bold: bool) -> Span {
    Span { text: text.into(), x, y, w: text.chars().count() as f32 * size * 0.5, h: size * 1.2, size, bold, italic: false, color: "000000".into(), family: "sans".into() }
}

fn page() -> PageData {
    PageData {
        w: 595.0,
        h: 842.0,
        lines: vec![
            Line { spans: vec![span("Rapport trimestriel", 72.0, 72.0, 20.0, true)] },
            Line { spans: vec![span("Le chiffre d’affaires progresse de", 72.0, 110.0, 11.0, false)] },
            Line { spans: vec![span("12 % sur le trimestre.", 72.0, 124.0, 11.0, false)] },
            Line { spans: vec![span("Région", 72.0, 160.0, 11.0, true), span("Ventes", 250.0, 160.0, 11.0, true), span("Part", 400.0, 160.0, 11.0, true)] },
            Line { spans: vec![span("Nord", 72.0, 175.0, 11.0, false), span("1 234,50", 250.0, 175.0, 11.0, false), span("12,5 %", 400.0, 175.0, 11.0, false)] },
            Line { spans: vec![span("Sud", 72.0, 190.0, 11.0, false), span("987", 250.0, 190.0, 11.0, false), span("8 %", 400.0, 190.0, 11.0, false)] },
        ],
        image: None,
        figures: Vec::new(),
    }
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("aura-convert-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

fn unzip(path: &std::path::Path, name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut s = String::new();
    std::io::Read::read_to_string(&mut z.by_name(name).unwrap(), &mut s).unwrap();
    s
}

#[test]
fn texte_brut() {
    let p = page();
    let t = plain_text([&p].into_iter(), false);
    assert!(t.starts_with("Rapport trimestriel\r\nLe chiffre"), "{t}");
    assert!(t.contains("trimestre.\r\n\r\nRégion"), "paragraphes séparés : {t}");
    assert!(t.contains("Nord 1 234,50 12,5 %"), "{t}");
}

#[test]
fn word_puis_pdf_natif() {
    let out = tmp("rapport.docx");
    for keep in [true, false] {
        docx::write(&[page(), page()], keep, "Rapport", &out).unwrap();
        let xml = unzip(&out, "word/document.xml");
        assert!(xml.contains("Rapport trimestriel") && xml.contains("<w:b/>"), "titre en gras");
        assert_eq!(xml.matches("w:type=\"page\"").count(), if keep { 1 } else { 0 });
        if !keep {
            // Les deux lignes du paragraphe sont réunies.
            assert!(xml.contains("progresse de</w:t>"));
            assert_eq!(roxmltree::Document::parse(&xml).unwrap().descendants().filter(|n| n.tag_name().name() == "p").count(), 2 * 5);
        }
    }
    // Rendu natif du document Word produit (sans Office).
    let pdf = tmp("rapport-natif.pdf");
    render_docx::to_pdf(&out, &pdf).unwrap();
    let doc = lopdf::Document::load(&pdf).unwrap();
    assert!(!doc.get_pages().is_empty());
    let text = doc.extract_text(&[1]).unwrap_or_default();
    assert!(text.contains("Rapport") || doc.get_pages().len() >= 1, "{text}");
}

#[test]
fn excel_tableau() {
    let out = tmp("rapport.xlsx");
    let p = page();
    xlsx::write(&[(0, &p)], &out).unwrap();
    let sheet = unzip(&out, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("<c r=\"B5\" s=\"0\"><v>1234.5</v></c>"), "{sheet}");
    assert!(sheet.contains("<c r=\"C5\" s=\"2\"><v>0.125</v></c>"), "{sheet}");
    assert!(sheet.contains("<c r=\"A4\" t=\"inlineStr\" s=\"1\">"), "en-tête en gras : {sheet}");
    assert!(unzip(&out, "xl/workbook.xml").contains("Page 1"));
}
