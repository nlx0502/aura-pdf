//! Écriture d'un classeur Excel (.xlsx, SpreadsheetML) : une feuille par page,
//! les lignes du PDF deviennent des rangées, les colonnes sont retrouvées par
//! l'alignement des segments ; les nombres (format français compris) sont
//! convertis en valeurs numériques.

use std::io::Write;

use super::model::{PageData, Span};
use super::zipfile;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Nombre lu dans une cellule : valeur et format (0 nombre, 1 pourcentage, 2 euros).
pub fn parse_number(s: &str) -> Option<(f64, u8)> {
    let t = s.trim();
    if t.is_empty() || t.len() > 40 {
        return None;
    }
    let (t, kind) = if let Some(r) = t.strip_suffix('%') {
        (r.trim_end(), 1)
    } else if let Some(r) = t.strip_suffix('€') {
        (r.trim_end(), 2)
    } else if let Some(r) = t.strip_prefix('€') {
        (r.trim_start(), 2)
    } else {
        (t, 0)
    };
    let neg = t.starts_with('-') || t.starts_with('−') || (t.starts_with('(') && t.ends_with(')'));
    let digits: String = t
        .chars()
        .filter(|c| !matches!(c, ' ' | '\u{a0}' | '\u{202f}' | '\'' | '(' | ')' | '-' | '−' | '+'))
        .collect();
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit() || c == ',' || c == '.') || !digits.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    // « 1.234,56 » (français) ou « 1,234.56 » (anglais) : le dernier séparateur est décimal.
    let last_comma = digits.rfind(',');
    let last_dot = digits.rfind('.');
    let normalized = match (last_comma, last_dot) {
        (Some(c), Some(d)) if c > d => digits.replace('.', "").replace(',', "."),
        (Some(_), Some(_)) => digits.replace(',', ""),
        // « 1,5 » : virgule décimale ; « 1,234,567 » : séparateurs de milliers.
        (Some(_), None) if digits.matches(',').count() == 1 => digits.replace(',', "."),
        (Some(_), None) => digits.replace(',', ""),
        (None, Some(_)) if digits.matches('.').count() > 1 => digits.replace('.', ""),
        _ => digits,
    };
    let mut v: f64 = normalized.parse().ok()?;
    if neg {
        v = -v;
    }
    if kind == 1 {
        v /= 100.0;
    }
    Some((v, kind))
}

/// Lettre(s) de colonne Excel.
fn col_name(mut i: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (i % 26) as u8) as char);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s
}

/// Cellules d'une page : (rangée, colonne, segments).
fn cells(page: &PageData) -> Vec<(usize, usize, Vec<&Span>)> {
    // Ancres de colonnes : bords gauches des segments, regroupés à 14 pt près.
    let mut xs: Vec<f32> = page.lines.iter().flat_map(|l| l.spans.iter().map(|s| s.x)).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut anchors: Vec<f32> = Vec::new();
    for x in xs {
        match anchors.last() {
            Some(a) if x - a < 14.0 => {}
            _ => anchors.push(x),
        }
    }
    let mut out = Vec::new();
    for (row, l) in page.lines.iter().enumerate() {
        let mut row_cells: Vec<(usize, Vec<&Span>)> = Vec::new();
        for s in &l.spans {
            let col = anchors.iter().rposition(|a| *a <= s.x + 1.0).unwrap_or(0);
            // Segments voisins sans grand écart : même cellule.
            if let Some(last) = row_cells.last_mut() {
                let prev = *last.1.last().unwrap();
                if s.x - (prev.x + prev.w) < prev.size * 1.5 {
                    last.1.push(s);
                    continue;
                }
                if col <= last.0 {
                    let next = last.0 + 1;
                    row_cells.push((next, vec![s]));
                    continue;
                }
            }
            row_cells.push((col, vec![s]));
        }
        for (col, spans) in row_cells {
            out.push((row, col, spans));
        }
    }
    out
}

fn cell_text(spans: &[&Span]) -> String {
    let mut t = String::new();
    for (i, s) in spans.iter().enumerate() {
        if i > 0 && !t.ends_with(' ') {
            t.push(' ');
        }
        t += &s.text;
    }
    t.trim().to_string()
}

fn sheet(page: &PageData) -> String {
    let cells = cells(page);
    let ncols = cells.iter().map(|c| c.1 + 1).max().unwrap_or(1);
    let mut widths = vec![8usize; ncols];
    let mut rows = String::new();
    let mut cur_row: Option<usize> = None;
    for (r, c, spans) in &cells {
        if cur_row != Some(*r) {
            if cur_row.is_some() {
                rows += "</row>";
            }
            rows += &format!("<row r=\"{}\">", r + 1);
            cur_row = Some(*r);
        }
        let text = cell_text(spans);
        let bold = spans.iter().all(|s| s.bold);
        let reference = format!("{}{}", col_name(*c), r + 1);
        widths[*c] = widths[*c].max(text.chars().count().min(60));
        match parse_number(&text) {
            Some((v, kind)) => {
                let style = match kind {
                    1 => 2,
                    2 => 3,
                    _ => {
                        if bold {
                            1
                        } else {
                            0
                        }
                    }
                };
                rows += &format!("<c r=\"{reference}\" s=\"{style}\"><v>{v}</v></c>");
            }
            None => {
                rows += &format!("<c r=\"{reference}\" t=\"inlineStr\"{}><is><t xml:space=\"preserve\">{}</t></is></c>", if bold { " s=\"1\"" } else { "" }, esc(&text));
            }
        }
    }
    if cur_row.is_some() {
        rows += "</row>";
    }
    let cols: String = widths.iter().enumerate().map(|(i, w)| format!("<col min=\"{0}\" max=\"{0}\" width=\"{1}\" customWidth=\"1\"/>", i + 1, (*w as f32 * 1.1 + 2.0).min(70.0))).collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><cols>{cols}</cols><sheetData>{rows}</sheetData></worksheet>"
    )
}

pub fn write(pages: &[(u32, &PageData)], out: &std::path::Path) -> Result<u64, String> {
    let mut files: Vec<(String, String)> = Vec::new();
    let mut sheets = String::new();
    let mut rels = String::new();
    let mut overrides = String::new();
    for (i, (pno, p)) in pages.iter().enumerate() {
        let n = i + 1;
        files.push((format!("xl/worksheets/sheet{n}.xml"), sheet(p)));
        sheets += &format!("<sheet name=\"Page {}\" sheetId=\"{n}\" r:id=\"rId{n}\"/>", pno + 1);
        rels += &format!("<Relationship Id=\"rId{n}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{n}.xml\"/>");
        overrides += &format!("<Override PartName=\"/xl/worksheets/sheet{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>");
    }
    let k = pages.len() + 1;
    rels += &format!("<Relationship Id=\"rId{k}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>");
    files.push(("[Content_Types].xml".into(), format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>{overrides}</Types>")));
    files.push(("_rels/.rels".into(), "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>".into()));
    files.push(("xl/workbook.xml".into(), format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets>{sheets}</sheets></workbook>")));
    files.push(("xl/_rels/workbook.xml.rels".into(), format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{rels}</Relationships>")));
    // Styles : 0 normal, 1 gras, 2 pourcentage, 3 euros.
    files.push(("xl/styles.xml".into(), "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><numFmts count=\"1\"><numFmt numFmtId=\"164\" formatCode=\"#,##0.00\\ &quot;€&quot;\"/></numFmts><fonts count=\"2\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font><font><b/><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts><fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills><borders count=\"1\"><border/></borders><cellStyleXfs count=\"1\"><xf/></cellStyleXfs><cellXfs count=\"4\"><xf numFmtId=\"0\" fontId=\"0\"/><xf numFmtId=\"0\" fontId=\"1\" applyFont=\"1\"/><xf numFmtId=\"10\" fontId=\"0\" applyNumberFormat=\"1\"/><xf numFmtId=\"164\" fontId=\"0\" applyNumberFormat=\"1\"/></cellXfs></styleSheet>".into()));
    let refs: Vec<(&str, String)> = files.iter().map(|(n, c)| (n.as_str(), c.clone())).collect();
    let bytes = zipfile(&refs)?;
    let mut f = std::fs::File::create(out).map_err(|e| format!("écriture impossible ({e})"))?;
    f.write_all(&bytes).map_err(|e| format!("écriture impossible ({e})"))?;
    Ok(bytes.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres() {
        assert_eq!(parse_number("1 234,56"), Some((1234.56, 0)));
        assert_eq!(parse_number("1.234,56"), Some((1234.56, 0)));
        assert_eq!(parse_number("1,234.56"), Some((1234.56, 0)));
        assert_eq!(parse_number("12,5 %"), Some((0.125, 1)));
        assert_eq!(parse_number("1 200 €"), Some((1200.0, 2)));
        assert_eq!(parse_number("-42"), Some((-42.0, 0)));
        assert_eq!(parse_number("(15)"), Some((-15.0, 0)));
        assert_eq!(parse_number("Q1 2025"), None);
        assert_eq!(parse_number("2025"), Some((2025.0, 0)));
        assert_eq!(col_name(0), "A");
        assert_eq!(col_name(27), "AB");
    }
}
