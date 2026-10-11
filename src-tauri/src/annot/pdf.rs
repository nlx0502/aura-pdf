//! Aides lopdf : objets indirects, attributs hérités, chaînes de texte PDF.

use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat};

use super::geom::PageGeom;
use super::model::Kind;

pub fn num(o: &Object) -> Option<f32> {
    match o {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

/// Suit une référence éventuelle.
pub fn deref<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    doc.dereference(o).map(|(_, o)| o).unwrap_or(o)
}

pub fn get<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    d.get(key).ok().map(|o| deref(doc, o))
}

/// Attribut de page hérité de l'arbre des pages (MediaBox, CropBox, Rotate).
pub fn inherited<'a>(doc: &'a Document, page: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    let mut cur = page;
    for _ in 0..32 {
        if let Some(v) = get(doc, cur, key) {
            return Some(v);
        }
        cur = match get(doc, cur, b"Parent").and_then(|p| p.as_dict().ok()) {
            Some(p) => p,
            None => return None,
        };
    }
    None
}

fn rect_of(doc: &Document, o: &Object) -> Option<[f32; 4]> {
    let a = deref(doc, o).as_array().ok()?;
    if a.len() < 4 {
        return None;
    }
    Some([num(deref(doc, &a[0]))?, num(deref(doc, &a[1]))?, num(deref(doc, &a[2]))?, num(deref(doc, &a[3]))?])
}

pub fn page_geom(doc: &Document, page: &Dictionary) -> PageGeom {
    let media = inherited(doc, page, b"MediaBox").and_then(|o| rect_of(doc, o)).unwrap_or([0.0, 0.0, 612.0, 792.0]);
    let crop = inherited(doc, page, b"CropBox").and_then(|o| rect_of(doc, o)).unwrap_or(media);
    let rot = inherited(doc, page, b"Rotate").and_then(|o| o.as_i64().ok()).unwrap_or(0);
    PageGeom::new(crop[0], crop[1], crop[2], crop[3], rot)
}

/// Chaîne de texte PDF (UTF-16BE avec BOM, ou PDFDocEncoding ≈ Latin-1).
pub fn text(o: &Object) -> String {
    let bytes = match o {
        Object::String(b, _) => b.as_slice(),
        Object::Name(b) => b.as_slice(),
        _ => return String::new(),
    };
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let units: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        return String::from_utf16_lossy(&units);
    }
    if bytes.len() >= 3 && bytes[..3] == [0xEF, 0xBB, 0xBF] {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }
    bytes
        .iter()
        .map(|&b| match b {
            0x91 => '‘',
            0x92 => '’',
            0x93 => '“',
            0x94 => '”',
            0x96 => '–',
            0x97 => '—',
            0x85 => '…',
            b => b as char,
        })
        .collect()
}

/// Chaîne de texte PDF : littérale si ASCII, sinon UTF-16BE avec BOM.
pub fn text_object(s: &str) -> Object {
    if s.is_ascii() {
        Object::String(s.as_bytes().to_vec(), StringFormat::Literal)
    } else {
        let mut b = vec![0xFE, 0xFF];
        for u in s.encode_utf16() {
            b.extend_from_slice(&u.to_be_bytes());
        }
        Object::String(b, StringFormat::Hexadecimal)
    }
}

/// Date PDF de l'instant présent (UTC).
pub fn now_pdf_date() -> String {
    chrono::Utc::now().format("D:%Y%m%d%H%M%S+00'00'").to_string()
}

/// Annotations d'une page : (objet indirect éventuel, dictionnaire), dans l'ordre de /Annots.
pub fn page_annots<'a>(doc: &'a Document, page: &'a Dictionary) -> Vec<(Option<ObjectId>, &'a Dictionary)> {
    let Some(arr) = get(doc, page, b"Annots").and_then(|a| a.as_array().ok()) else { return Vec::new() };
    arr.iter()
        .filter_map(|o| {
            let id = o.as_reference().ok();
            deref(doc, o).as_dict().ok().map(|d| (id, d))
        })
        .collect()
}

/// Rôle d'une annotation existante.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Type pris en charge par l'éditeur.
    Editable(Kind),
    /// Réponse ou changement d'état rattaché à une autre annotation (/IRT).
    Reply,
    /// Fenêtre contextuelle (/Popup) d'une autre annotation.
    Popup,
    /// Autre type (lien, champ de formulaire, son…) : jamais modifié.
    Other,
}

pub fn classify(doc: &Document, d: &Dictionary) -> Role {
    let sub = get(doc, d, b"Subtype").and_then(|o| o.as_name().ok()).unwrap_or(b"");
    if d.has(b"IRT") {
        return Role::Reply;
    }
    match sub {
        b"Highlight" => Role::Editable(Kind::Highlight),
        b"Underline" => Role::Editable(Kind::Underline),
        b"StrikeOut" => Role::Editable(Kind::Strike),
        b"Squiggly" => Role::Editable(Kind::Squiggly),
        b"Text" => Role::Editable(Kind::Note),
        b"FreeText" => Role::Editable(Kind::Text),
        b"Ink" => Role::Editable(Kind::Ink),
        b"Square" => Role::Editable(Kind::Rect),
        b"Circle" => Role::Editable(Kind::Ellipse),
        b"Line" => {
            let arrow = get(doc, d, b"LE")
                .and_then(|o| o.as_array().ok())
                .map(|a| a.iter().any(|e| matches!(e.as_name(), Ok(n) if n.ends_with(b"Arrow"))))
                .unwrap_or(false);
            Role::Editable(if arrow { Kind::Arrow } else { Kind::Line })
        }
        b"Stamp" => Role::Editable(Kind::Stamp),
        b"Popup" => Role::Popup,
        _ => Role::Other,
    }
}
