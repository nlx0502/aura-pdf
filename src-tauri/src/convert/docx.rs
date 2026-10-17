//! Écriture d'un document Word (.docx, WordprocessingML) à partir des pages
//! extraites. Deux modes :
//! - mise en page conservée : une ligne du PDF = un paragraphe, avec son
//!   retrait, son espacement et ses sauts de page ;
//! - texte continu : les lignes sont regroupées en paragraphes, les titres
//!   repérés à leur taille.
//!
//! Les illustrations (images, graphiques) sont insérées à leur place, en PNG.

use std::io::Write;

use super::model::{Figure, Line, PageData, Span};
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

/// Élément du flux : ligne de texte ou illustration.
enum Item<'a> {
    Line(&'a Line),
    Fig(&'a Figure),
}

impl Item<'_> {
    fn y(&self) -> f32 {
        match self {
            Item::Line(l) => l.y(),
            Item::Fig(f) => f.y,
        }
    }
}

/// Lignes et illustrations d'une page, dans l'ordre de lecture.
fn items(p: &PageData) -> Vec<Item<'_>> {
    let mut v: Vec<Item> = p.lines.iter().map(Item::Line).chain(p.figures.iter().map(Item::Fig)).collect();
    v.sort_by(|a, b| a.y().partial_cmp(&b.y()).unwrap_or(std::cmp::Ordering::Equal));
    v
}

/// Images du document, numérotées dans l'ordre d'insertion.
struct Images<'a> {
    list: Vec<&'a [u8]>,
    max_w: f32,
}

impl<'a> Images<'a> {
    /// Paragraphe contenant l'illustration (réduite à la largeur utile si besoin).
    fn para(&mut self, f: &'a Figure, ppr: &str) -> String {
        self.list.push(&f.png);
        let n = self.list.len();
        let k = if f.w > self.max_w { self.max_w / f.w } else { 1.0 };
        let (cx, cy) = (((f.w * k) * 12700.0).round() as i64, ((f.h * k) * 12700.0).round() as i64);
        let drawing = format!(
            "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{n}\" name=\"Illustration {n}\"/><a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:nvPicPr><pic:cNvPr id=\"{n}\" name=\"image{n}.png\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"rIdImg{n}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
        );
        para(ppr, &drawing)
    }
}

/// Corps du document.
fn body<'a>(pages: &'a [PageData], keep_layout: bool, m: &Margins, img: &mut Images<'a>) -> String {
    let mut out = String::new();
    if keep_layout {
        for (pi, p) in pages.iter().enumerate() {
            if pi > 0 {
                out += "<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>";
            }
            let mut prev_bottom = m.top;
            for it in items(p) {
                let l = match it {
                    Item::Line(l) => l,
                    Item::Fig(f) => {
                        let ppr = format!("<w:spacing w:before=\"{}\" w:after=\"0\"/><w:ind w:left=\"{}\"/>", twips((f.y - prev_bottom).max(0.0)), twips((f.x - m.left).max(0.0)));
                        out += &img.para(f, &ppr);
                        prev_bottom = f.y + f.h;
                        continue;
                    }
                };
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
    // Texte continu : regroupement des lignes en paragraphes ; les
    // illustrations interrompent le regroupement.
    let flow: Vec<Item> = pages.iter().flat_map(items).collect();
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
    // Découpage du flux en suites de lignes séparées par les illustrations.
    let mut runs_of: Vec<(Vec<&Line>, Option<&Figure>)> = vec![(Vec::new(), None)];
    for it in &flow {
        match it {
            Item::Line(l) => runs_of.last_mut().unwrap().0.push(l),
            Item::Fig(f) => {
                runs_of.last_mut().unwrap().1 = Some(f);
                runs_of.push((Vec::new(), None));
            }
        }
    }
    for (all, fig) in runs_of {
        out += &paragraphs(&all, body_size);
        if let Some(f) = fig {
            out += &img.para(f, &format!("<w:jc w:val=\"center\"/><w:spacing w:before=\"120\" w:after=\"{}\"/>", twips(body_size * 0.6)));
        }
    }
    out
}

/// Lignes consécutives regroupées en paragraphes (titres repérés à leur taille).
fn paragraphs(all: &[&Line], body_size: f32) -> String {
    let mut out = String::new();
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
    let mut img = Images { list: Vec::new(), max_w: (pw - m.left - m.right).max(72.0) };
    let body = body(pages, keep_layout, &m, &mut img);
    let doc = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\"><w:body>{body}<w:sectPr><w:pgSz w:w=\"{}\" w:h=\"{}\"{}/><w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"0\" w:footer=\"0\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>",
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
    let rels: String = (1..=img.list.len())
        .map(|n| format!("<Relationship Id=\"rIdImg{n}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/image{n}.png\"/>"))
        .collect();
    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("[Content_Types].xml".to_string(), "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/><Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/></Types>".to_string().into_bytes()),
        ("_rels/.rels".to_string(), "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/></Relationships>".to_string().into_bytes()),
        ("word/_rels/document.xml.rels".to_string(), "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>{rels}</Relationships>".replace("{rels}", &rels).into_bytes()),
        ("word/document.xml".to_string(), doc.into_bytes()),
        ("word/styles.xml".to_string(), styles.as_bytes().to_vec()),
        ("docProps/core.xml".to_string(), core.into_bytes()),
    ];
    for (n, png) in img.list.iter().enumerate() {
        files.push((format!("word/media/image{}.png", n + 1), png.to_vec()));
    }
    let refs: Vec<(&str, &[u8])> = files.iter().map(|(n, c)| (n.as_str(), c.as_slice())).collect();
    let bytes = zipfile(&refs)?;
    let mut f = std::fs::File::create(out).map_err(|e| format!("écriture impossible ({e})"))?;
    f.write_all(&bytes).map_err(|e| format!("écriture impossible ({e})"))?;
    Ok(bytes.len() as u64)
}
