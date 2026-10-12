//! Archivage PDF/A (niveaux 1b, 2b, 3b) : métadonnées XMP, profil de sortie
//! sRGB, retrait du contenu dynamique ; contrôle de conformité.
//!
//! Les polices standard non intégrées (Helvetica, Times, Courier) sont
//! remplacées par leurs équivalents Windows métriquement compatibles (Arial,
//! Times New Roman, Courier New), intégrés. Les autres polices manquantes et la
//! transparence (interdite en PDF/A-1) sont signalées par le contrôle.

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream, StringFormat};
use serde::Serialize;

use super::Meta;
use crate::annot::pdf::{deref, get};

/// Profil ICC sRGB v2 compact (domaine public, CC0).
const SRGB: &[u8] = include_bytes!("../../resources/icc/sRGB-v2-magic.icc");

pub const LEVELS: [&str; 3] = ["1b", "2b", "3b"];

fn split_level(level: &str) -> Result<(u8, char), String> {
    if !LEVELS.contains(&level) {
        return Err(format!("Niveau PDF/A inconnu : {level}."));
    }
    let mut c = level.chars();
    let part = c.next().and_then(|d| d.to_digit(10)).unwrap_or(2) as u8;
    Ok((part, c.next().unwrap_or('b').to_ascii_uppercase()))
}

fn catalog_id(doc: &Document) -> Option<ObjectId> {
    doc.trailer.get(b"Root").ok().and_then(|o| o.as_reference().ok())
}

fn catalog_mut(doc: &mut Document) -> Option<&mut Dictionary> {
    let id = catalog_id(doc)?;
    doc.get_object_mut(id).ok().and_then(|o| o.as_dict_mut().ok())
}

fn xmp_bytes(doc: &Document) -> Option<Vec<u8>> {
    let cat = doc.catalog().ok()?;
    let s = get(doc, cat, b"Metadata")?.as_stream().ok()?;
    Some(s.decompressed_content().unwrap_or_else(|_| s.content.clone()))
}

/// Valeur d'une propriété XMP, en attribut (`pdfaid:part="2"`) ou en élément.
fn xmp_value(xmp: &str, key: &str) -> Option<String> {
    if let Some(i) = xmp.find(&format!("{key}=\"")) {
        let rest = &xmp[i + key.len() + 2..];
        return rest.find('"').map(|j| rest[..j].trim().to_string());
    }
    let open = format!("<{key}>");
    let i = xmp.find(&open)?;
    let rest = &xmp[i + open.len()..];
    rest.find('<').map(|j| rest[..j].trim().to_string())
}

/// Niveau PDF/A déclaré dans les métadonnées XMP (« 2b »…).
pub fn declared(doc: &Document) -> Option<String> {
    let xmp = String::from_utf8_lossy(&xmp_bytes(doc)?).into_owned();
    let part = xmp_value(&xmp, "pdfaid:part")?;
    let conf = xmp_value(&xmp, "pdfaid:conformance").unwrap_or_else(|| "B".into());
    Some(format!("{}{}", part, conf.to_ascii_lowercase()))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Écrit (ou remplace) le flux de métadonnées XMP du catalogue.
pub fn write_xmp(doc: &mut Document, meta: &Meta, level: Option<&str>) {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let mut body = String::new();
    let alt = |v: &str| format!("<rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt>", esc(v));
    if !meta.title.trim().is_empty() {
        body += &format!("<dc:title>{}</dc:title>", alt(meta.title.trim()));
    }
    if !meta.author.trim().is_empty() {
        body += &format!("<dc:creator><rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq></dc:creator>", esc(meta.author.trim()));
    }
    if !meta.subject.trim().is_empty() {
        body += &format!("<dc:description>{}</dc:description>", alt(meta.subject.trim()));
    }
    if !meta.keywords.trim().is_empty() {
        body += &format!("<pdf:Keywords>{}</pdf:Keywords>", esc(meta.keywords.trim()));
    }
    body += "<pdf:Producer>Aura PDF</pdf:Producer>";
    body += &format!("<xmp:ModifyDate>{now}</xmp:ModifyDate><xmp:MetadataDate>{now}</xmp:MetadataDate>");
    if let Some((part, conf)) = level.and_then(|l| split_level(l).ok()) {
        body += &format!("<pdfaid:part>{part}</pdfaid:part><pdfaid:conformance>{conf}</pdfaid:conformance>");
    }
    let xmp = format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
<rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\">\n\
{body}\n\
</rdf:Description>\n\
</rdf:RDF>\n\
</x:xmpmeta>\n\
<?xpacket end=\"w\"?>"
    );
    // Le flux XMP reste non compressé (exigé par PDF/A, lisible par les indexeurs).
    let mut stream = Stream::new(dictionary! { "Type" => "Metadata", "Subtype" => "XML" }, xmp.into_bytes());
    stream.allows_compression = false;
    let id = doc.add_object(stream);
    if let Some(cat) = catalog_mut(doc) {
        cat.set("Metadata", id);
    }
}

/// Convertit le document au niveau PDF/A demandé.
pub fn convert(doc: &mut Document, level: &str, meta: &Meta) -> Result<(), String> {
    let (part, _) = split_level(level)?;
    doc.version = if part == 1 { "1.4".into() } else { "1.7".into() };
    write_xmp(doc, meta, Some(level));
    embed_standard_fonts(doc);

    // Profil de sortie sRGB.
    let mut icc = Stream::new(dictionary! { "N" => 3 }, SRGB.to_vec());
    let _ = icc.compress();
    let icc_id = doc.add_object(icc);
    let intent = dictionary! {
        "Type" => "OutputIntent",
        "S" => "GTS_PDFA1",
        "OutputConditionIdentifier" => Object::string_literal("sRGB IEC61966-2.1"),
        "Info" => Object::string_literal("sRGB IEC61966-2.1"),
        "RegistryName" => Object::string_literal("http://www.color.org"),
        "DestOutputProfile" => icc_id,
    };
    let intent_id = doc.add_object(intent);

    // Contenu dynamique et éléments interdits.
    let cat = catalog_mut(doc).ok_or("Catalogue du document introuvable.")?;
    cat.set("OutputIntents", vec![Object::Reference(intent_id)]);
    cat.remove(b"AA");
    if let Ok(Object::Dictionary(oa)) = cat.get(b"OpenAction") {
        if !matches!(oa.get(b"S"), Ok(Object::Name(n)) if n == b"GoTo") {
            cat.remove(b"OpenAction");
        }
    }
    let names_id = cat.get(b"Names").ok().and_then(|o| o.as_reference().ok());
    if let Ok(Object::Dictionary(names)) = cat.get_mut(b"Names") {
        names.remove(b"JavaScript");
        if part < 3 {
            names.remove(b"EmbeddedFiles");
        }
    }
    if let Some(id) = names_id {
        if let Ok(names) = doc.get_object_mut(id).and_then(|o| o.as_dict_mut()) {
            names.remove(b"JavaScript");
            if part < 3 {
                names.remove(b"EmbeddedFiles");
            }
        }
    }
    let oa_id = catalog_mut(doc).and_then(|c| c.get(b"OpenAction").ok()).and_then(|o| o.as_reference().ok());
    if let Some(id) = oa_id {
        let goto = doc.get_dictionary(id).ok().map(|d| matches!(d.get(b"S"), Ok(Object::Name(n)) if n == b"GoTo")).unwrap_or(true);
        if !goto {
            if let Some(c) = catalog_mut(doc) {
                c.remove(b"OpenAction");
            }
        }
    }

    for (_, obj) in doc.objects.iter_mut() {
        let dict = match obj {
            Object::Dictionary(d) => d,
            Object::Stream(s) => &mut s.dict,
            _ => continue,
        };
        // Actions automatiques (pages, champs, annotations).
        dict.remove(b"AA");
        let is_annot = dict.has_type(b"Annot") || dict.has(b"Subtype") && dict.has(b"Rect");
        if is_annot {
            // Actions JavaScript ou de lancement sur les liens et widgets.
            if let Ok(Object::Dictionary(a)) = dict.get(b"A") {
                if matches!(a.get(b"S"), Ok(Object::Name(n)) if n == b"JavaScript" || n == b"Launch") {
                    dict.remove(b"A");
                }
            }
            // Les annotations doivent être imprimables et visibles.
            let f = dict.get(b"F").and_then(|o| o.as_i64()).unwrap_or(0);
            dict.set("F", (f | 4) & !(1 | 2 | 32));
        }
        if part < 3 && dict.has_type(b"Filespec") {
            dict.remove(b"EF");
        }
    }
    // Pièces jointes en annotation : interdites en 1b et 2b.
    if part < 3 {
        let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
        for p in pages {
            let annots: Vec<Object> = doc
                .get_dictionary(p)
                .ok()
                .and_then(|d| get(doc, d, b"Annots"))
                .and_then(|a| a.as_array().ok())
                .cloned()
                .unwrap_or_default();
            let keep: Vec<Object> = annots
                .into_iter()
                .filter(|a| !deref(doc, a).as_dict().map(|d| matches!(d.get(b"Subtype"), Ok(Object::Name(n)) if n == b"FileAttachment")).unwrap_or(false))
                .collect();
            if let Ok(d) = doc.get_dictionary_mut(p) {
                if d.has(b"Annots") {
                    d.set("Annots", keep);
                }
            }
        }
    }
    // Le /ID est exigé.
    if !doc.trailer.has(b"ID") {
        let mut id = [0u8; 16];
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut id);
        let s = Object::String(id.to_vec(), StringFormat::Hexadecimal);
        doc.trailer.set("ID", vec![s.clone(), s]);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    /// fonts, colors, metadata, dynamic, encryption, transparency, attachments.
    pub id: &'static str,
    pub ok: bool,
    /// Nombre d'éléments en cause (polices non intégrées…).
    pub count: usize,
    /// Corrigé automatiquement lors de la conversion.
    pub fixable: bool,
}

/// Contrôle de conformité au niveau demandé, avant ou après conversion.
pub fn check(doc: &Document, level: &str, encrypted: bool) -> Result<Vec<Check>, String> {
    let (part, _) = split_level(level)?;
    let mut fonts = 0usize;
    let mut fonts_fixable = 0usize;
    let mut transparency = 0usize;
    let mut dynamic = 0usize;
    let mut attachments = 0usize;
    // Images CMJN : incompatibles avec un profil de sortie RVB (sRGB).
    let mut cmyk = 0usize;
    for obj in doc.objects.values() {
        let dict = match obj {
            Object::Dictionary(d) => d,
            Object::Stream(s) => &s.dict,
            _ => continue,
        };
        if dict.has_type(b"Font") && !font_embedded(doc, dict) {
            fonts += 1;
            if substitute(doc, dict).is_some() {
                fonts_fixable += 1;
            }
        }
        if dict.has(b"AA") {
            dynamic += 1;
        }
        if let Some(a) = get(doc, dict, b"A").and_then(|a| a.as_dict().ok()) {
            if matches!(a.get(b"S"), Ok(Object::Name(n)) if n == b"JavaScript" || n == b"Launch") {
                dynamic += 1;
            }
        }
        if dict.has_type(b"Filespec") && dict.has(b"EF") {
            attachments += 1;
        }
        // Transparence : masques doux, opacité, modes de fusion, groupes.
        if dict.has_type(b"ExtGState") {
            let smask = matches!(get(doc, dict, b"SMask"), Some(o) if !matches!(o, Object::Name(n) if n == b"None"));
            let alpha = [b"CA".as_slice(), b"ca"].iter().any(|k| get(doc, dict, k).and_then(|o| o.as_float().ok()).map(|v| v < 0.999).unwrap_or(false));
            let blend = matches!(get(doc, dict, b"BM"), Some(Object::Name(n)) if n != b"Normal" && n != b"Compatible");
            if smask || alpha || blend {
                transparency += 1;
            }
        }
        if matches!(dict.get(b"Subtype"), Ok(Object::Name(n)) if n == b"Image") {
            if dict.has(b"SMask") {
                transparency += 1;
            }
            if matches!(get(doc, dict, b"ColorSpace"), Some(Object::Name(n)) if n == b"DeviceCMYK") {
                cmyk += 1;
            }
        }
        if let Some(g) = get(doc, dict, b"Group").and_then(|g| g.as_dict().ok()) {
            if matches!(g.get(b"S"), Ok(Object::Name(n)) if n == b"Transparency") {
                transparency += 1;
            }
        }
    }
    let cat = doc.catalog().map_err(|e| format!("Catalogue introuvable ({e})."))?;
    if cat.has(b"AA") {
        dynamic += 1;
    }
    if let Some(names) = get(doc, cat, b"Names").and_then(|n| n.as_dict().ok()) {
        if names.has(b"JavaScript") {
            dynamic += 1;
        }
        if part < 3 && names.has(b"EmbeddedFiles") {
            attachments += 1;
        }
    }
    let intent = get(doc, cat, b"OutputIntents")
        .and_then(|a| a.as_array().ok())
        .map(|a| a.iter().any(|i| deref(doc, i).as_dict().map(|d| d.has(b"DestOutputProfile")).unwrap_or(false)))
        .unwrap_or(false);
    let declared = declared(doc);
    let mut checks = vec![
        Check { id: "fonts", ok: fonts == 0, count: fonts, fixable: fonts == fonts_fixable },
        Check { id: "colors", ok: intent && cmyk == 0, count: cmyk, fixable: cmyk == 0 },
        Check { id: "metadata", ok: declared.as_deref() == Some(level), count: 0, fixable: true },
        Check { id: "dynamic", ok: dynamic == 0, count: dynamic, fixable: true },
        Check { id: "encryption", ok: !encrypted, count: 0, fixable: true },
    ];
    if part < 3 {
        checks.push(Check { id: "attachments", ok: attachments == 0, count: attachments, fixable: true });
    }
    if part == 1 {
        checks.push(Check { id: "transparency", ok: transparency == 0, count: transparency, fixable: false });
    }
    Ok(checks)
}

/// Police système équivalente à une police standard non intégrée (mêmes
/// métriques) : famille, gras, italique.
fn substitute(doc: &Document, font: &Dictionary) -> Option<(&'static str, bool, bool)> {
    if !matches!(font.get(b"Subtype"), Ok(Object::Name(n)) if n == b"Type1" || n == b"TrueType") || font_embedded(doc, font) {
        return None;
    }
    // Codage simple uniquement (WinAnsi ou standard) : pas de /Differences.
    match get(doc, font, b"Encoding") {
        None => {}
        Some(Object::Name(n)) if n == b"WinAnsiEncoding" || n == b"StandardEncoding" => {}
        _ => return None,
    }
    let base = font.get(b"BaseFont").ok().and_then(|o| o.as_name().ok()).map(|n| String::from_utf8_lossy(n).into_owned())?;
    let base = base.split('+').last().unwrap_or(&base).replace(['-', ','], "").to_ascii_lowercase();
    let bold = base.contains("bold");
    let italic = base.contains("italic") || base.contains("oblique");
    let family = if base.starts_with("helvetica") || base.starts_with("arial") {
        "Arial"
    } else if base.starts_with("times") {
        "Times New Roman"
    } else if base.starts_with("courier") {
        "Courier New"
    } else {
        return None;
    };
    crate::sysfonts::load(family, bold, italic).map(|_| (family, bold, italic))
}

/// Intègre les polices standard (Helvetica, Times, Courier) au moyen de leurs
/// équivalents système métriquement compatibles. Renvoie le nombre de polices.
fn embed_standard_fonts(doc: &mut Document) -> usize {
    let targets: Vec<(ObjectId, (&'static str, bool, bool))> = doc
        .objects
        .iter()
        .filter_map(|(id, o)| {
            let d = o.as_dict().ok()?;
            if !d.has_type(b"Font") {
                return None;
            }
            substitute(doc, d).map(|s| (*id, s))
        })
        .collect();
    let mut cache: std::collections::HashMap<(&str, bool, bool), Dictionary> = Default::default();
    let mut n = 0;
    for (id, key) in targets {
        let dict = match cache.get(&key) {
            Some(d) => d.clone(),
            None => {
                let Some(font) = crate::sysfonts::load(key.0, key.1, key.2) else { continue };
                let d = crate::sysfonts::embed_winansi(doc, &font);
                cache.insert(key, d.clone());
                d
            }
        };
        if let Ok(o) = doc.get_object_mut(id) {
            *o = Object::Dictionary(dict);
            n += 1;
        }
    }
    n
}

/// Police intégrée (les polices Type 3 sont décrites dans le document).
fn font_embedded(doc: &Document, font: &Dictionary) -> bool {
    match font.get(b"Subtype") {
        Ok(Object::Name(n)) if n == b"Type3" => return true,
        Ok(Object::Name(n)) if n == b"Type0" => {
            return get(doc, font, b"DescendantFonts")
                .and_then(|a| a.as_array().ok())
                .map(|a| a.iter().all(|d| deref(doc, d).as_dict().map(|d| font_embedded(doc, d)).unwrap_or(false)))
                .unwrap_or(false);
        }
        _ => {}
    }
    get(doc, font, b"FontDescriptor")
        .and_then(|d| d.as_dict().ok())
        .map(|d| d.has(b"FontFile") || d.has(b"FontFile2") || d.has(b"FontFile3"))
        .unwrap_or(false)
}
