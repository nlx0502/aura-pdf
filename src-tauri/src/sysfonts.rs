//! Polices installées sur le système (TrueType) : recensement, mesure du texte,
//! extraction d'un sous-ensemble et intégration dans un PDF.
//!
//! Seules les polices à contours TrueType (`glyf`) dont la licence autorise
//! l'intégration sont proposées. Le PDF ne reçoit qu'un sous-ensemble : les
//! glyphes inutilisés sont vidés, ce qui garde des fichiers légers.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream, StringFormat};
use parking_lot::Mutex;
use serde::Serialize;
use ttf_parser::{Face, GlyphId, Tag};

#[derive(Debug, Clone)]
struct FaceRef {
    path: PathBuf,
    index: u32,
    bold: bool,
    italic: bool,
}

/// Famille proposée dans l'interface.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyInfo {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    pub mono: bool,
}

struct Catalog {
    families: BTreeMap<String, Vec<FaceRef>>,
    info: Vec<FamilyInfo>,
}

fn font_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(w) = std::env::var_os("WINDIR") {
        v.push(PathBuf::from(w).join("Fonts"));
    }
    if let Some(l) = dirs::data_local_dir() {
        v.push(l.join("Microsoft").join("Windows").join("Fonts"));
    }
    v
}

/// Nom de famille (typographique si présent), en anglais de préférence.
fn family_name(face: &Face) -> Option<String> {
    let mut best: Option<(u8, String)> = None;
    for n in face.names() {
        let rank = match n.name_id {
            16 => 2,
            1 => 1,
            _ => continue,
        };
        if !n.is_unicode() {
            continue;
        }
        let Some(s) = n.to_string() else { continue };
        let english = n.language_id == 0x0409;
        let score = rank * 2 + english as u8;
        if best.as_ref().map(|b| score > b.0).unwrap_or(true) {
            best = Some((score, s));
        }
    }
    best.map(|b| b.1.trim().to_string()).filter(|s| !s.is_empty() && !s.starts_with('@') && !s.starts_with('.'))
}

fn scan() -> Catalog {
    let mut families: BTreeMap<String, Vec<FaceRef>> = BTreeMap::new();
    let mut mono: HashMap<String, bool> = HashMap::new();
    for dir in font_dirs() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
            if !matches!(ext.as_str(), "ttf" | "ttc" | "otf") {
                continue;
            }
            // Les très grosses polices (idéogrammes) ralentiraient le recensement.
            if e.metadata().map(|m| m.len() > 40_000_000).unwrap_or(true) {
                continue;
            }
            let Ok(data) = std::fs::read(&p) else { continue };
            let count = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
            for index in 0..count {
                let Ok(face) = Face::parse(&data, index) else { continue };
                if face.tables().glyf.is_none() || face.permissions() == Some(ttf_parser::Permissions::Restricted) {
                    continue;
                }
                // Polices de symboles : sans lettres latines, inutiles pour du texte.
                if face.glyph_index('a').is_none() {
                    continue;
                }
                let Some(name) = family_name(&face) else { continue };
                let bold = face.weight().to_number() >= 600 || face.is_bold();
                let italic = face.is_italic() || face.is_oblique();
                mono.entry(name.clone()).or_insert(face.is_monospaced());
                let list = families.entry(name).or_default();
                if !list.iter().any(|f| f.bold == bold && f.italic == italic) {
                    list.push(FaceRef { path: p.clone(), index, bold, italic });
                }
            }
        }
    }
    // Familles sans style normal : on garde celles qui ont au moins un style.
    let info = families
        .iter()
        .map(|(k, v)| FamilyInfo {
            family: k.clone(),
            bold: v.iter().any(|f| f.bold && !f.italic),
            italic: v.iter().any(|f| f.italic && !f.bold),
            mono: mono.get(k).copied().unwrap_or(false),
        })
        .collect();
    Catalog { families, info }
}

fn catalog() -> &'static Catalog {
    static CAT: OnceLock<Catalog> = OnceLock::new();
    CAT.get_or_init(scan)
}

/// Familles installées utilisables (le premier appel parcourt le dossier des polices).
pub fn list() -> Vec<FamilyInfo> {
    catalog().info.clone()
}

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

/// Famille installée correspondant à un nom de police PDF (« ABCDEF+Calibri-Bold » → « Calibri »).
pub fn match_family(pdf_name: &str) -> Option<String> {
    let base = pdf_name.split('+').next_back().unwrap_or(pdf_name);
    let base = base.split([',', '-']).next().unwrap_or(base);
    let key = norm(base);
    if key.len() < 3 {
        return None;
    }
    let cat = catalog();
    cat.families.keys().find(|f| norm(f) == key).cloned().or_else(|| {
        // « TimesNewRomanPSMT » → « Times New Roman »
        cat.families.keys().filter(|f| key.starts_with(&norm(f)) && norm(f).len() >= 5).max_by_key(|f| f.len()).cloned()
    })
}

/// Police chargée en mémoire.
pub struct Loaded {
    data: Arc<Vec<u8>>,
    index: u32,
    pub bold: bool,
    pub italic: bool,
    /// Gras ou italique demandé mais absent de la famille : à simuler.
    pub fake_bold: bool,
    pub fake_italic: bool,
    pub family: String,
}

impl Loaded {
    fn face(&self) -> Face<'_> {
        Face::parse(&self.data, self.index).expect("police déjà validée")
    }

    pub fn glyph(&self, c: char) -> u16 {
        self.face().glyph_index(c).map(|g| g.0).unwrap_or(0)
    }

    /// Avance d'un caractère, en 1/1000 em.
    pub fn advance(&self, c: char) -> f32 {
        let f = self.face();
        let gid = f.glyph_index(c).unwrap_or(GlyphId(0));
        f.glyph_hor_advance(gid).unwrap_or(0) as f32 * 1000.0 / f.units_per_em() as f32
    }

    pub fn text_width(&self, s: &str, size: f32) -> f32 {
        let f = self.face();
        let upem = f.units_per_em() as f32;
        s.chars().map(|c| f.glyph_hor_advance(f.glyph_index(c).unwrap_or(GlyphId(0))).unwrap_or(0) as f32).sum::<f32>() * size / upem
    }

    /// Sous-ensemble autonome (une seule police, glyphes inutiles vidés).
    pub fn subset(&self, text: &str) -> Vec<u8> {
        let face = self.face();
        let mut keep: BTreeSet<u16> = BTreeSet::new();
        keep.insert(0);
        for c in text.chars().chain(" ".chars()) {
            if let Some(g) = face.glyph_index(c) {
                keep.insert(g.0);
            }
        }
        subset_face(&face, &keep)
    }

    fn ps_name(&self) -> String {
        let f = self.face();
        let n = f
            .names()
            .into_iter()
            .find(|n| n.name_id == 6 && n.is_unicode())
            .and_then(|n| n.to_string())
            .unwrap_or_else(|| self.family.clone());
        n.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect()
    }
}

static LOADED: OnceLock<Mutex<HashMap<(PathBuf, u32), Arc<Vec<u8>>>>> = OnceLock::new();

/// Charge la face d'une famille la plus proche du style demandé.
pub fn load(family: &str, bold: bool, italic: bool) -> Option<Loaded> {
    let faces = catalog().families.get(family)?;
    let pick = faces
        .iter()
        .find(|f| f.bold == bold && f.italic == italic)
        .or_else(|| faces.iter().find(|f| f.bold == bold && !f.italic))
        .or_else(|| faces.iter().find(|f| !f.bold && f.italic == italic))
        .or_else(|| faces.iter().find(|f| !f.bold && !f.italic))
        .or_else(|| faces.first())?;
    let cache = LOADED.get_or_init(Default::default);
    let key = (pick.path.clone(), pick.index);
    let data = {
        let mut c = cache.lock();
        match c.get(&key) {
            Some(d) => d.clone(),
            None => {
                let d = Arc::new(std::fs::read(&pick.path).ok()?);
                c.insert(key, d.clone());
                d
            }
        }
    };
    Some(Loaded {
        data,
        index: pick.index,
        bold: pick.bold,
        italic: pick.italic,
        fake_bold: bold && !pick.bold,
        fake_italic: italic && !pick.italic,
        family: family.to_string(),
    })
}

/// Police fournie par l'application (octets embarqués), style normal.
pub fn from_bytes(family: &str, data: &'static [u8]) -> Option<Loaded> {
    Face::parse(data, 0).ok()?;
    Some(Loaded { data: Arc::new(data.to_vec()), index: 0, bold: false, italic: false, fake_bold: false, fake_italic: false, family: family.to_string() })
}

#[allow(dead_code)]
pub fn exists(family: &str) -> bool {
    catalog().families.contains_key(family)
}

// ---------------------------------------------------------------- sous-ensemble TrueType

fn be16(d: &[u8], o: usize) -> u16 {
    d.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]])).unwrap_or(0)
}
fn be32(d: &[u8], o: usize) -> u32 {
    d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0)
}

/// Tables inutiles dans un PDF (mise en forme avancée, signature, bitmaps).
const DROP: [&[u8; 4]; 12] = [b"DSIG", b"GSUB", b"GPOS", b"GDEF", b"BASE", b"JSTF", b"MATH", b"hdmx", b"VDMX", b"LTSH", b"EBDT", b"EBLC"];

/// Sous-ensemble d'une police TrueType quelconque (glyphes `keep`, plus .notdef).
/// `None` si la police n'a pas de contours TrueType.
pub fn subset_bytes(data: &[u8], keep: &BTreeSet<u16>) -> Option<Vec<u8>> {
    let face = Face::parse(data, 0).ok()?;
    face.tables().glyf?;
    let mut k = keep.clone();
    k.insert(0);
    Some(subset_face(&face, &k))
}

fn subset_face(face: &Face, keep: &BTreeSet<u16>) -> Vec<u8> {
    let raw = face.raw_face();
    let table = |tag: &[u8; 4]| raw.table(Tag::from_bytes(tag));
    let head = table(b"head").unwrap_or(&[]);
    let long_loca = be16(head, 50) == 1;
    let loca = table(b"loca").unwrap_or(&[]);
    let glyf = table(b"glyf").unwrap_or(&[]);
    let n = face.number_of_glyphs() as usize;
    let off = |g: usize| -> usize {
        if long_loca {
            be32(loca, g * 4) as usize
        } else {
            be16(loca, g * 2) as usize * 2
        }
    };
    // Glyphes composites : leurs composants sont conservés aussi.
    let mut all: BTreeSet<u16> = keep.clone();
    let mut stack: Vec<u16> = keep.iter().copied().collect();
    while let Some(g) = stack.pop() {
        let (a, b) = (off(g as usize), off(g as usize + 1));
        if b <= a || b > glyf.len() || (be16(glyf, a) as i16) >= 0 {
            continue;
        }
        let mut p = a + 10;
        loop {
            let flags = be16(glyf, p);
            let comp = be16(glyf, p + 2);
            if (comp as usize) < n && all.insert(comp) {
                stack.push(comp);
            }
            p += 4 + if flags & 0x0001 != 0 { 4 } else { 2 };
            p += if flags & 0x0008 != 0 { 2 } else if flags & 0x0040 != 0 { 4 } else if flags & 0x0080 != 0 { 8 } else { 0 };
            if flags & 0x0020 == 0 || p >= b {
                break;
            }
        }
    }
    let mut new_glyf: Vec<u8> = Vec::new();
    let mut new_loca: Vec<u8> = Vec::with_capacity((n + 1) * 4);
    for g in 0..n {
        new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
        if all.contains(&(g as u16)) {
            let (a, b) = (off(g), off(g + 1));
            if b > a && b <= glyf.len() {
                new_glyf.extend_from_slice(&glyf[a..b]);
                while new_glyf.len() % 4 != 0 {
                    new_glyf.push(0);
                }
            }
        }
    }
    new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());

    let mut tables: Vec<([u8; 4], Vec<u8>)> = Vec::new();
    for rec in raw.table_records {
        let tag = rec.tag.to_bytes();
        if DROP.iter().any(|d| **d == tag) {
            continue;
        }
        let data = match &tag {
            b"glyf" => new_glyf.clone(),
            b"loca" => new_loca.clone(),
            b"head" => {
                let mut h = head.to_vec();
                if h.len() >= 54 {
                    h[8..12].copy_from_slice(&[0, 0, 0, 0]); // checkSumAdjustment
                    h[50..52].copy_from_slice(&1u16.to_be_bytes()); // loca longue
                }
                h
            }
            _ => match raw.table(rec.tag) {
                Some(d) => d.to_vec(),
                None => continue,
            },
        };
        tables.push((tag, data));
    }
    tables.sort_by(|a, b| a.0.cmp(&b.0));
    write_sfnt(&tables)
}

fn checksum(d: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    for chunk in d.chunks(4) {
        let mut b = [0u8; 4];
        b[..chunk.len()].copy_from_slice(chunk);
        sum = sum.wrapping_add(u32::from_be_bytes(b));
    }
    sum
}

fn write_sfnt(tables: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let num = tables.len() as u16;
    let mut pow = 1u16;
    let mut log = 0u16;
    while pow * 2 <= num {
        pow *= 2;
        log += 1;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&num.to_be_bytes());
    out.extend_from_slice(&(pow * 16).to_be_bytes());
    out.extend_from_slice(&log.to_be_bytes());
    out.extend_from_slice(&(num * 16 - pow * 16).to_be_bytes());
    let mut offset = 12 + 16 * tables.len();
    let mut body = Vec::new();
    for (tag, data) in tables {
        out.extend_from_slice(tag);
        out.extend_from_slice(&checksum(data).to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset = 12 + 16 * tables.len() + body.len();
    }
    out.extend_from_slice(&body);
    out
}

// ---------------------------------------------------------------- intégration PDF (lopdf)

/// Police composite (Type0, Identity-H) intégrée à `doc` pour afficher `text`.
/// Le texte s'encode ensuite avec [`encode`].
pub fn embed(doc: &mut Document, font: &Loaded, text: &str) -> ObjectId {
    let face = font.face();
    let upem = face.units_per_em() as f32;
    let scale = |v: f32| v * 1000.0 / upem;
    let sub = font.subset(text);
    let mut ff = Stream::new(dictionary! { "Length1" => sub.len() as i64 }, sub);
    let _ = ff.compress();
    let ff_id = doc.add_object(ff);

    let mut used: BTreeMap<u16, char> = BTreeMap::new();
    for c in text.chars() {
        let g = font.glyph(c);
        if g != 0 {
            used.entry(g).or_insert(c);
        }
    }
    let bb = face.global_bounding_box();
    let name = format!("AURAFT+{}", font.ps_name());
    let mut flags = 32;
    if font.italic || font.fake_italic {
        flags |= 64;
    }
    let desc = doc.add_object(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(name.clone().into_bytes()),
        "Flags" => flags,
        "FontBBox" => vec![scale(bb.x_min as f32).into(), scale(bb.y_min as f32).into(), scale(bb.x_max as f32).into(), scale(bb.y_max as f32).into()],
        "ItalicAngle" => Object::Real(face.italic_angle()),
        "Ascent" => Object::Real(scale(face.ascender() as f32)),
        "Descent" => Object::Real(scale(face.descender() as f32)),
        "CapHeight" => Object::Real(scale(face.capital_height().unwrap_or(face.ascender()) as f32)),
        "StemV" => 80,
        "FontFile2" => ff_id,
    });
    let mut w: Vec<Object> = Vec::new();
    for (&g, _) in &used {
        let adv = face.glyph_hor_advance(GlyphId(g)).unwrap_or(0) as f32;
        w.push((g as i64).into());
        w.push(Object::Array(vec![Object::Real(scale(adv))]));
    }
    let cid = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => Object::Name(name.clone().into_bytes()),
        "CIDSystemInfo" => dictionary! { "Registry" => Object::string_literal("Adobe"), "Ordering" => Object::string_literal("Identity"), "Supplement" => 0 },
        "FontDescriptor" => desc,
        "DW" => 1000,
        "W" => w,
        "CIDToGIDMap" => "Identity",
    });
    let mut cmap = String::from("/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /Aura-UCS def /CMapType 2 def 1 begincodespacerange <0000> <FFFF> endcodespacerange\n");
    let entries: Vec<(u16, char)> = used.into_iter().collect();
    for chunk in entries.chunks(100) {
        cmap += &format!("{} beginbfchar\n", chunk.len());
        for (g, c) in chunk {
            let u: String = c.encode_utf16(&mut [0u16; 2]).iter().map(|x| format!("{x:04X}")).collect();
            cmap += &format!("<{g:04X}> <{u}>\n");
        }
        cmap += "endbfchar\n";
    }
    cmap += "endcmap CMapName currentdict /CMap defineresource pop end end";
    let mut tu = Stream::new(dictionary! {}, cmap.into_bytes());
    let _ = tu.compress();
    let tu_id = doc.add_object(tu);
    doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => Object::Name(name.into_bytes()),
        "Encoding" => "Identity-H",
        "DescendantFonts" => vec![Object::Reference(cid)],
        "ToUnicode" => tu_id,
    })
}

/// Caractère d'un code WinAnsi (128 à 159 : table propre ; au-delà : Latin-1).
pub fn winansi_char(code: u8) -> Option<char> {
    const HIGH: [u16; 32] = [
        0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0, 0x017D, 0, //
        0, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0, 0x017E, 0x0178,
    ];
    match code {
        32..=126 | 160..=255 => Some(code as char),
        128..=159 => char::from_u32(HIGH[(code - 128) as usize] as u32).filter(|c| *c != '\0'),
        _ => None,
    }
}

/// Police TrueType simple (codage WinAnsi, codes 32 à 255) intégrée à `doc` :
/// remplace une police standard non intégrée (archivage PDF/A).
pub fn embed_winansi(doc: &mut Document, font: &Loaded) -> Dictionary {
    let face = font.face();
    let upem = face.units_per_em() as f32;
    let scale = |v: f32| v * 1000.0 / upem;
    let all: String = (32u8..=255).filter_map(winansi_char).collect();
    let sub = font.subset(&all);
    let mut ff = Stream::new(dictionary! { "Length1" => sub.len() as i64 }, sub);
    let _ = ff.compress();
    let ff_id = doc.add_object(ff);
    let bb = face.global_bounding_box();
    let name = format!("AURAPA+{}", font.ps_name());
    let mut flags = 32;
    if font.italic {
        flags |= 64;
    }
    let desc = doc.add_object(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(name.clone().into_bytes()),
        "Flags" => flags,
        "FontBBox" => vec![scale(bb.x_min as f32).into(), scale(bb.y_min as f32).into(), scale(bb.x_max as f32).into(), scale(bb.y_max as f32).into()],
        "ItalicAngle" => Object::Real(face.italic_angle()),
        "Ascent" => Object::Real(scale(face.ascender() as f32)),
        "Descent" => Object::Real(scale(face.descender() as f32)),
        "CapHeight" => Object::Real(scale(face.capital_height().unwrap_or(face.ascender()) as f32)),
        "StemV" => if font.bold { 140 } else { 80 },
        "FontFile2" => ff_id,
    });
    let widths: Vec<Object> = (32u8..=255)
        .map(|c| match winansi_char(c) {
            Some(ch) if font.glyph(ch) != 0 => Object::Real(font.advance(ch).round()),
            _ => Object::Integer(0),
        })
        .collect();
    dictionary! {
        "Type" => "Font",
        "Subtype" => "TrueType",
        "BaseFont" => Object::Name(name.into_bytes()),
        "FirstChar" => 32,
        "LastChar" => 255,
        "Widths" => widths,
        "FontDescriptor" => desc,
        "Encoding" => "WinAnsiEncoding",
    }
}

/// Chaîne PDF (identifiants de glyphes sur 2 octets) pour une police intégrée.
pub fn encode(font: &Loaded, text: &str) -> Object {
    let mut b = Vec::with_capacity(text.len() * 2);
    for c in text.chars() {
        b.extend_from_slice(&font.glyph(c).to_be_bytes());
    }
    Object::String(b, StringFormat::Hexadecimal)
}

/// Chemin de la police d'une famille (pour les messages et les tests).
#[allow(dead_code)]
pub fn path_of(family: &str) -> Option<&'static Path> {
    catalog().families.get(family).and_then(|v| v.first()).map(|f| f.path.as_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polices_du_systeme() {
        let list = list();
        if !list.iter().any(|f| f.family == "Arial") {
            return; // poste sans Arial (hors Windows)
        }
        let f = load("Arial", true, false).unwrap();
        assert!(f.bold && !f.fake_bold);
        assert!((f.advance('A') - 722.0).abs() < 5.0, "largeur Arial Bold A : {}", f.advance('A'));
        // Sous-ensemble : nettement plus petit, toujours lisible et cohérent.
        let sub = f.subset("Bonjour à tous");
        let full = std::fs::metadata(path_of("Arial").unwrap()).unwrap().len() as usize;
        assert!(sub.len() < full / 2, "{} / {}", sub.len(), full);
        let face = Face::parse(&sub, 0).unwrap();
        assert_eq!(face.glyph_index('à').map(|g| g.0), Some(f.glyph('à')));
        assert_eq!(match_family("ABCDEF+Arial-BoldMT").as_deref(), Some("Arial"));
        assert_eq!(match_family("ArialMT").as_deref(), Some("Arial"));
    }
}
