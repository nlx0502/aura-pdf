//! Formulaires AcroForm : lecture des champs (valeur, propriétés, apparence,
//! libellés visibles), enregistrement (mise à jour incrémentale) et
//! aplatissement (réécriture complète).
//!
//! Le libellé visible d'un champ est un bouton en lecture seule placé sur la
//! page (`/AuraCaptionOf` désigne son champ) : il reste un élément du
//! formulaire (et non un commentaire), se masque pendant l'édition dans Aura et
//! s'aplatit avec le reste.

mod appearance;
mod flatten;
pub mod style;
#[cfg(test)]
mod tests;
mod write;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::annot::geom::{PageGeom, Rect};
use crate::annot::pdf::{deref, get, num, page_geom, text};
pub use flatten::flatten;
use style::FieldStyle;
pub use write::{save, FormPlan};

const FF_READONLY: i64 = 1;
const FF_REQUIRED: i64 = 1 << 1;
const FF_MULTILINE: i64 = 1 << 12;
const FF_NO_TOGGLE_OFF: i64 = 1 << 14;
const FF_RADIO: i64 = 1 << 15;
const FF_PUSH: i64 = 1 << 16;
const FF_COMBO: i64 = 1 << 17;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldOption {
    pub value: String,
    pub label: String,
}

/// Libellé visible sur la page.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Caption {
    pub text: String,
    /// Points d'affichage.
    pub rect: Rect,
    pub size: f32,
    pub color: String,
    pub bold: bool,
    #[serde(default)]
    pub page: u32,
    /// Indice de l'annotation dans la page (lecture seule).
    #[serde(default, skip_deserializing)]
    pub index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Widget {
    pub page: u32,
    /// Rectangle en points d'affichage.
    pub rect: Rect,
    /// État « activé » d'une case ou d'un bouton radio.
    #[serde(default)]
    pub on: Option<String>,
    /// Libellé propre à ce widget (option d'un bouton radio).
    #[serde(default)]
    pub caption: Option<Caption>,
    /// Indice de l'annotation dans la page (lecture seule).
    #[serde(default, skip_deserializing)]
    pub index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    /// Nom complet (« parent.enfant »), identifiant du champ.
    pub name: String,
    /// Champ à créer (absent du fichier).
    #[serde(default)]
    pub is_new: bool,
    /// Libellé (/TU, info-bulle), sinon le nom.
    pub label: String,
    /// text, email, date, select, list, radio, check, sig, button.
    pub kind: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub options: Vec<FieldOption>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub readonly: bool,
    #[serde(default)]
    pub multiline: bool,
    #[serde(default)]
    pub max_len: Option<u32>,
    /// Texte indicatif affiché quand le champ est vide.
    #[serde(default)]
    pub placeholder: String,
    pub style: FieldStyle,
    pub widgets: Vec<Widget>,
    #[serde(default)]
    pub caption: Option<Caption>,
}

/// Nœud terminal de l'arbre des champs, avec ses widgets.
struct Node {
    id: ObjectId,
    name: String,
    /// Dictionnaire fusionné avec les attributs hérités (FT, Ff, V, DA, Q, Opt).
    merged: Dictionary,
    widgets: Vec<ObjectId>,
}

impl Node {
    fn caption_of(&self) -> Option<String> {
        self.merged.get(b"AuraCaptionOf").ok().map(text)
    }
}

fn acroform(doc: &Document) -> Option<&Dictionary> {
    let cat = doc.catalog().ok()?;
    get(doc, cat, b"AcroForm")?.as_dict().ok()
}

fn nodes(doc: &Document) -> Vec<Node> {
    let mut out = Vec::new();
    let Some(form) = acroform(doc) else { return out };
    let Some(fields) = get(doc, form, b"Fields").and_then(|f| f.as_array().ok()) else { return out };
    fn walk(doc: &Document, o: &Object, parent: &str, inherited: &Dictionary, out: &mut Vec<Node>, depth: u32) {
        let Ok(id) = o.as_reference() else { return };
        let Ok(d) = doc.get_dictionary(id) else { return };
        if depth > 32 {
            return;
        }
        let mut merged = inherited.clone();
        for k in [b"FT".as_slice(), b"Ff", b"V", b"DA", b"Q", b"Opt", b"TU", b"MaxLen", b"AA", b"AuraKind", b"AuraPlaceholder", b"AuraCaptionOf", b"AuraCaptionWidget", b"AuraCaption"] {
            if let Some(v) = get(doc, d, k) {
                merged.set(k.to_vec(), v.clone());
            }
        }
        let part = d.get(b"T").map(text).unwrap_or_default();
        let name = match (parent.is_empty(), part.is_empty()) {
            (_, true) => parent.to_string(),
            (true, false) => part.clone(),
            (false, false) => format!("{parent}.{part}"),
        };
        let kids: Vec<&Object> = get(doc, d, b"Kids").and_then(|k| k.as_array().ok()).map(|a| a.iter().collect()).unwrap_or_default();
        // Enfants qui sont des champs (ils ont un nom) : on descend.
        let field_kids: Vec<&Object> = kids
            .iter()
            .copied()
            .filter(|k| deref(doc, k).as_dict().map(|kd| kd.has(b"T")).unwrap_or(false))
            .collect();
        if !field_kids.is_empty() {
            // Le TU d'un parent ne s'applique pas à ses enfants nommés.
            merged.remove(b"TU");
            for k in field_kids {
                walk(doc, k, &name, &merged, out, depth + 1);
            }
            return;
        }
        let widgets: Vec<ObjectId> = if kids.is_empty() {
            vec![id]
        } else {
            kids.iter().filter_map(|k| k.as_reference().ok()).collect()
        };
        out.push(Node { id, name, merged, widgets });
    }
    for f in fields {
        walk(doc, f, "", &Dictionary::new(), &mut out, 0);
    }
    out
}

fn flags(d: &Dictionary) -> i64 {
    d.get(b"Ff").and_then(|o| o.as_i64()).unwrap_or(0)
}

fn name_str(o: &Object) -> String {
    match o {
        Object::Name(n) => String::from_utf8_lossy(n).into_owned(),
        o => text(o),
    }
}

/// États d'apparence « activés » d'un widget (/AP /N hors /Off).
fn on_state(doc: &Document, w: &Dictionary) -> Option<String> {
    let ap = get(doc, w, b"AP")?.as_dict().ok()?;
    let n = get(doc, ap, b"N")?.as_dict().ok()?;
    n.iter().map(|(k, _)| String::from_utf8_lossy(k).into_owned()).find(|k| k != "Off")
}

fn kind_of(doc: &Document, n: &Node) -> String {
    let d = &n.merged;
    let ft = d.get(b"FT").and_then(|o| o.as_name()).unwrap_or(b"");
    let ff = flags(d);
    match ft {
        b"Tx" => {
            if let Ok(k) = d.get(b"AuraKind").and_then(|o| o.as_name()) {
                return String::from_utf8_lossy(k).into_owned();
            }
            let js = get(doc, d, b"AA")
                .and_then(|aa| aa.as_dict().ok())
                .map(|aa| {
                    aa.iter()
                        .filter_map(|(_, a)| deref(doc, a).as_dict().ok())
                        .filter_map(|a| get(doc, a, b"JS"))
                        .map(|js| match js {
                            Object::Stream(s) => String::from_utf8_lossy(&s.decompressed_content().unwrap_or_default()).into_owned(),
                            o => text(o),
                        })
                        .collect::<String>()
                })
                .unwrap_or_default();
            if js.contains("AFDate") {
                return "date".into();
            }
            let hint = format!("{} {}", n.name, d.get(b"TU").map(text).unwrap_or_default()).to_lowercase();
            if hint.contains("mail") || hint.contains("courriel") {
                "email".into()
            } else {
                "text".into()
            }
        }
        b"Ch" => if ff & FF_COMBO != 0 { "select" } else { "list" }.into(),
        b"Btn" => {
            if ff & FF_PUSH != 0 {
                "button"
            } else if ff & FF_RADIO != 0 {
                "radio"
            } else {
                "check"
            }
        }
        .into(),
        b"Sig" => "sig".into(),
        _ => "text".into(),
    }
}

/// Page (index), objet page et indice dans /Annots de chaque widget.
fn widget_pages(doc: &Document) -> HashMap<ObjectId, (u32, ObjectId, u32)> {
    let mut m = HashMap::new();
    for (pno, pid) in doc.get_pages() {
        let Ok(page) = doc.get_dictionary(pid) else { continue };
        if let Some(arr) = get(doc, page, b"Annots").and_then(|a| a.as_array().ok()) {
            for (i, a) in arr.iter().enumerate() {
                if let Ok(id) = a.as_reference() {
                    m.insert(id, (pno - 1, pid, i as u32));
                }
            }
        }
    }
    m
}

fn rect_of(doc: &Document, d: &Dictionary) -> Option<Rect> {
    let a = get(doc, d, b"Rect")?.as_array().ok()?;
    if a.len() < 4 {
        return None;
    }
    let v: Vec<f32> = a.iter().filter_map(|o| num(deref(doc, o))).collect();
    (v.len() == 4).then(|| Rect::from_corners(v[0], v[1], v[2], v[3]))
}

fn geoms(doc: &Document) -> HashMap<ObjectId, PageGeom> {
    doc.get_pages().values().filter_map(|pid| doc.get_dictionary(*pid).ok().map(|p| (*pid, page_geom(doc, p)))).collect()
}

/// Libellé visible lu depuis son bouton (`/AuraCaption`).
fn read_caption(doc: &Document, node: &Node, pages: &HashMap<ObjectId, (u32, ObjectId, u32)>, geoms: &HashMap<ObjectId, PageGeom>) -> Option<Caption> {
    let w = *node.widgets.first()?;
    let wd = doc.get_dictionary(w).ok()?;
    let &(page, pid, index) = pages.get(&w)?;
    let r = rect_of(doc, wd)?;
    let spec = node.merged.get(b"AuraCaption").ok().and_then(|o| deref(doc, o).as_dict().ok());
    let t = |k: &[u8]| spec.and_then(|s| s.get(k).ok());
    Some(Caption {
        text: t(b"Text").map(text).unwrap_or_default(),
        rect: geoms.get(&pid)?.rect_to_display(&r),
        size: t(b"Size").and_then(num).unwrap_or(10.0),
        color: t(b"Color").map(text).unwrap_or_else(|| "#3a3b40".into()),
        bold: t(b"Bold").and_then(|o| o.as_bool().ok()).unwrap_or(true),
        page,
        index: Some(index),
    })
}

/// Champs du formulaire, dans l'ordre du document.
pub fn list(doc: &Document) -> Vec<Field> {
    let pages = widget_pages(doc);
    let geoms = geoms(doc);
    let all = nodes(doc);
    // Libellés visibles : rattachés à leur champ (et à leur widget pour les options).
    let mut captions: HashMap<String, Vec<(i64, Caption)>> = HashMap::new();
    for n in &all {
        if let Some(owner) = n.caption_of() {
            if let Some(c) = read_caption(doc, n, &pages, &geoms) {
                let wi = n.merged.get(b"AuraCaptionWidget").and_then(|o| o.as_i64()).unwrap_or(-1);
                captions.entry(owner).or_default().push((wi, c));
            }
        }
    }
    let mut out = Vec::new();
    for n in all.iter().filter(|n| n.caption_of().is_none()) {
        let kind = kind_of(doc, n);
        let d = &n.merged;
        let ff = flags(d);
        let caps = captions.remove(&n.name).unwrap_or_default();
        let mut widgets = Vec::new();
        let mut first: Option<&Dictionary> = None;
        for (i, w) in n.widgets.iter().enumerate() {
            let Ok(wd) = doc.get_dictionary(*w) else { continue };
            let Some(&(page, pid, index)) = pages.get(w) else { continue };
            let Some(r) = rect_of(doc, wd) else { continue };
            first.get_or_insert(wd);
            let g = geoms.get(&pid).copied().unwrap_or(PageGeom::new(0.0, 0.0, 612.0, 792.0, 0));
            widgets.push(Widget {
                page,
                rect: g.rect_to_display(&r),
                on: if kind == "check" || kind == "radio" { on_state(doc, wd) } else { None },
                caption: caps.iter().find(|(wi, _)| *wi == i as i64).map(|(_, c)| c.clone()),
                index: Some(index),
            });
        }
        let mut options: Vec<FieldOption> = get(doc, d, b"Opt")
            .and_then(|o| o.as_array().ok())
            .map(|a| {
                a.iter()
                    .map(|o| match deref(doc, o) {
                        Object::Array(pair) if pair.len() >= 2 => FieldOption { value: text(deref(doc, &pair[0])), label: text(deref(doc, &pair[1])) },
                        o => FieldOption { value: text(o), label: text(o) },
                    })
                    .collect()
            })
            .unwrap_or_default();
        if kind == "radio" && options.is_empty() {
            let mut seen = HashSet::new();
            for w in &widgets {
                if let Some(on) = &w.on {
                    if seen.insert(on.clone()) {
                        let label = w.caption.as_ref().map(|c| c.text.clone()).unwrap_or_else(|| on.clone());
                        options.push(FieldOption { value: on.clone(), label });
                    }
                }
            }
        }
        let mut value = get(doc, d, b"V").map(|v| match v {
            Object::Array(a) => a.first().map(|o| name_str(deref(doc, o))).unwrap_or_default(),
            v => name_str(v),
        });
        if value.as_deref() == Some("Off") {
            value = Some(String::new());
        }
        // Sans /V, les cases se lisent sur leur état d'apparence.
        if value.is_none() && (kind == "check" || kind == "radio") {
            value = n.widgets.iter().filter_map(|w| doc.get_dictionary(*w).ok()).filter_map(|w| w.get(b"AS").ok().map(name_str)).find(|s| s != "Off");
        }
        if kind == "sig" {
            value = Some(if d.has(b"V") { "signed".into() } else { String::new() });
        }
        let style = first.map(|w| FieldStyle::read(doc, d, w, &kind)).unwrap_or_else(|| FieldStyle::default_for(&kind));
        out.push(Field {
            label: d.get(b"TU").ok().map(text).filter(|s| !s.trim().is_empty()).unwrap_or_else(|| n.name.clone()),
            name: n.name.clone(),
            is_new: false,
            kind,
            value: value.unwrap_or_default(),
            options,
            required: ff & FF_REQUIRED != 0,
            readonly: ff & FF_READONLY != 0,
            multiline: ff & FF_MULTILINE != 0,
            max_len: d.get(b"MaxLen").and_then(|o| o.as_i64()).ok().map(|v| v as u32),
            placeholder: d.get(b"AuraPlaceholder").map(text).unwrap_or_default(),
            style,
            widgets,
            caption: caps.iter().find(|(wi, _)| *wi < 0).map(|(_, c)| c.clone()),
        });
    }
    out
}

fn load_mem(bytes: &[u8], password: Option<&str>) -> Result<Document, String> {
    match password {
        Some(p) => Document::load_mem_with_options(bytes, lopdf::LoadOptions::with_password(p)),
        None => Document::load_mem(bytes),
    }
    .map_err(|e| format!("lecture du PDF : {e}"))
}

pub fn read(path: &Path, password: Option<&str>) -> Result<Vec<Field>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok(list(&load_mem(&bytes, password)?))
}

/// Nom de champ sûr dérivé du libellé.
fn slug(label: &str) -> String {
    let mut s = String::new();
    for c in label.chars() {
        let c = match c {
            'à' | 'â' | 'ä' | 'À' | 'Â' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if !s.ends_with('_') && !s.is_empty() {
            s.push('_');
        }
    }
    s.trim_end_matches('_').chars().take(40).collect()
}

/// Apparence normale d'un widget (état /AS pour les cases).
fn appearance_of(doc: &Document, w: &Dictionary) -> Option<ObjectId> {
    let ap = get(doc, w, b"AP")?.as_dict().ok()?;
    let n = ap.get(b"N").ok()?;
    if let Ok(id) = n.as_reference() {
        if doc.get_object(id).ok()?.as_stream().is_ok() {
            return Some(id);
        }
    }
    let states = deref(doc, n).as_dict().ok()?;
    let st = w.get(b"AS").ok().and_then(|o| o.as_name().ok())?;
    states.get(st).ok()?.as_reference().ok()
}
