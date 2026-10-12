//! Écriture d'un document Word (.docx, WordprocessingML) à partir des pages
//! extraites. Deux modes :
//! - mise en page conservée : une ligne du PDF = un paragraphe, avec son
//!   retrait, son espacement et ses sauts de page ;
//! - texte continu : les lignes sont regroupées en paragraphes, les titres
//!   repérés à leur taille.

use std::io::Write;

use super::model::{Line, PageData, Span};
use super::zipfile;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn font(family: &str) -> &'static str {
    match family {
        "serif" => "Times New Roman",
        "mono" => "Courier New",
        _ => "Arial",
    }
}

fn run(s: &Span, text: &str) -> String {
    let mut pr = format!("<w:rFonts w:ascii=\"{0}\" w:hAnsi=\"{0}\" w:cs=\"{0}\"/>", font(&s.family));
    if s.bold {
        pr += "<w:b/>";
    }
    if s.italic {
        pr += "<w:i/>";
    }
    if s.color != "000000" && !s.color.is_empty() {
        pr += &format!("<w:color w:val=\"{}\"/>", s.color.to_uppercase());
    }
    let hp = (s.size * 2.0).round().clamp(2.0, 400.0) as i32;
    pr += &format!("<w:sz w:val=\"{hp}\"/><w:szCs w:val=\"{hp}\"/>");
    let mut out = String::new();
    for (i, part) in text.split('\t').enumerate() {
        if i > 0 {
            out += &format!("<w:r><w:rPr>{pr}</w:rPr><w:tab/></w:r>");
        }
        if !part.is_empty() {
            out += &format!("<w:r><w:rPr>{pr}</w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r>", esc(part));
        }
    }
    out
}

/// Segments d'une ligne, avec les espaces (ou tabulations) entre eux.
fn line_runs(l: &Line) -> String {
    let mut out = String::new();
    for (i, s) in l.spans.iter().enumerate() {
        let mut text = s.text.clone();
        if i > 0 {
            let prev = &l.spans[i - 1];
            let gap = s.x - (prev.x + prev.w);
            if gap > prev.size * 1.5 {
                text = format!("\t{text}");
            } else if gap > prev.size * 0.15 && !prev.text.ends_with(' ') {
                text = format!(" {text}");
            }
        }
        out += &run(s, &text);
    }
    out
}

fn twips(pt: f32) -> i32 {
    (pt * 20.0).round() as i32
}

struct Margins {
    left: f32,
    top: f32,
    right: f32,
}

fn margins(pages: &[PageData]) -> Margins {
    let mut left = f32::MAX;
    let mut top = f32::MAX;
    let mut right = 0.0f32;
    let mut pw = 595.0f32;
    for p in pages {
        pw = p.w;
        for l in &p.lines {
            left = left.min(l.x());
            top = top.min(l.y());
            right = right.max(l.right());
        }
    }
    if left == f32::MAX {
        return Margins { left: 72.0, top: 72.0, right: 72.0 };
    }
    Margins { left: left.clamp(18.0, 144.0), top: top.clamp(18.0, 144.0), right: (pw - right).clamp(18.0, 144.0) }
}

fn para(ppr: &str, runs: &str) -> String {
    format!("<w:p><w:pPr>{ppr}</w:pPr>{runs}</w:p>")
}

/// Corps du document.
fn body(pages: &[PageData], keep_layout: bool, m: &Margins) -> String {
    let mut out = String::new();
    if keep_layout {
        for (pi, p) in pages.iter().enumerate() {
            if pi > 0 {
                out += "<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>";
            }
            let mut prev_bottom = m.top;
            for l in &p.lines {
                let before = (l.y() - prev_bottom).max(0.0);
                let indent = (l.x() - m.left).max(0.0);
                let center = (l.x() + l.right()) / 2.0;
                let centered = indent > 40.0 && (center - p.w / 2.0).abs() < 6.0;
                // Taquets aux colonnes de la ligne (segments précédés d'un grand écart).
                let tabs: String = l
                    .spans
                    .windows(2)
                    .filter(|w| w[1].x - (w[0].x + w[0].w) > w[0].size * 1.5)
                    .map(|w| format!("<w:tab w:val=\"left\" w:pos=\"{}\"/>", twips((w[1].x - m.left).max(0.0))))
                    .collect();
                let ppr = format!(
                    "{}<w:spacing w:before=\"{}\" w:after=\"0\" w:line=\"{}\" w:lineRule=\"exact\"/>{}",
                    if tabs.is_empty() { String::new() } else { format!("<w:tabs>{tabs}</w:tabs>") },
                    twips(before),
                    twips((l.bottom() - l.y()).max(l.size() * 1.05)),
                    if centered { "<w:jc w:val=\"center\"/>".to_string() } else { format!("<w:ind w:left=\"{}\"/>", twips(indent)) }
                );
                out += &para(&ppr, &line_runs(l));
                prev_bottom = l.bottom();
            }
        }
        return out;
    }
    // Texte continu : regroupement des lignes en paragraphes.
    let all: Vec<&Line> = pages.iter().flat_map(|p| p.lines.iter()).collect();
    let body_size = {
        let mut sizes: Vec<(i32, usize)> = Vec::new();
        for l in &all {
            let k = (l.size() * 2.0).round() as i32;
            let n = l.text().chars().count();
            match sizes.iter_mut().find(|(s, _)| *s == k) {
                Some(e) => e.1 += n,
                None => sizes.push((k, n)),
            }
        }
        sizes.into_iter().max_by_key(|(_, n)| *n).map(|(k, _)| k as f32 / 2.0).unwrap_or(11.0)
    };
    let mut i = 0;
    while i < all.len() {
        let first = all[i];
        let heading = first.size() >= body_size * 1.25;
        let mut lines = vec![first];
        while i + 1 < all.len() {
            let a = *lines.last().unwrap();
            let b = all[i + 1];
            let gap = b.y() - a.bottom();
            let same_size = (a.size() - b.size()).abs() < 0.6;
            let aligned = (b.x() - first.x()).abs() < 14.0 || (lines.len() == 1 && (b.x() - a.x()).abs() < 40.0);
            // Lignes en colonnes (tableau) : jamais fusionnées.
            let columns = |l: &Line| l.text().contains('\t');
            if !heading && same_size && aligned && !columns(a) && !columns(b) && gap >= -2.0 && gap < a.size() * 0.75 {
                lines.push(b);
                i += 1;
            } else {
                break;
            }
        }
        // Assemblage : césure de fin de ligne retirée, sinon un espace.
        let mut runs = String::new();
        for (k, l) in lines.iter().enumerate() {
            let mut lr = line_runs(l);
            if k + 1 < lines.len() {
                let t = l.text();
                if t.ends_with('-') && lines[k + 1].text().chars().next().map(|c| c.is_lowercase()).unwrap_or(false) {
                    if let Some(pos) = lr.rfind("-</w:t>") {
                        lr.replace_range(pos..pos + 1, "");
                    }
                } else if let Some(s) = l.spans.last() {
                    lr += &run(s, " ");
                }
            }
            runs += &lr;
        }
        let ppr = if heading {
            "<w:keepNext/><w:spacing w:before=\"240\" w:after=\"120\"/>".to_string()
        } else {
            format!("<w:spacing w:before=\"0\" w:after=\"{}\"/>", twips(body_size * 0.6))
        };
        out += &para(&ppr, &runs);
        i += 1;
    }
    out
}

pub fn write(pages: &[PageData], keep_layout: bool, title: &str, out: &std::path::Path) -> Result<u64, String> {
    let m = margins(pages);
    let (pw, ph) = pages.first().map(|p| (p.w, p.h)).unwrap_or((595.28, 841.89));
    let body = body(pages, keep_layout, &m);
    let doc = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><w:body>{body}<w:sectPr><w:pgSz w:w=\"{}\" w:h=\"{}\"{}/><w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"0\" w:footer=\"0\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>",
        twips(pw),
        twips(ph),
        if pw > ph { " w:orient=\"landscape\"" } else { "" },
        twips(m.top),
        twips(m.right),
        twips(if keep_layout { 18.0 } else { m.top }),
        twips(m.left),
    );
    let styles = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Arial\" w:hAnsi=\"Arial\" w:cs=\"Arial\" w:eastAsia=\"Arial\"/><w:sz w:val=\"22\"/><w:lang w:val=\"fr-FR\"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style></w:styles>";
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let core = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><dc:title>{}</dc:title><dc:creator>Aura PDF</dc:creator><dcterms:created xsi:type=\"dcterms:W3CDTF\">{now}</dcterms:created></cp:coreProperties>",
        esc(title)
    );
    let files: Vec<(&str, String)> = vec![
        ("[Content_Types].xml", "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/><Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/></Types>".into()),
        ("_rels/.rels", "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/></Relationships>".into()),
        ("word/_rels/document.xml.rels", "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>".into()),
        ("word/document.xml", doc),
        ("word/styles.xml", styles.into()),
        ("docProps/core.xml", core),
    ];
    let bytes = zipfile(&files)?;
    let mut f = std::fs::File::create(out).map_err(|e| format!("écriture impossible ({e})"))?;
    f.write_all(&bytes).map_err(|e| format!("écriture impossible ({e})"))?;
    Ok(bytes.len() as u64)
}
