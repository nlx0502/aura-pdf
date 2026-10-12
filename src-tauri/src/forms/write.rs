//! Enregistrement d'un formulaire par mise à jour incrémentale : chaque champ
//! modifié ou créé est réécrit en entier (drapeaux, valeur, apparence standard
//! /MK /BS /DA /Q, position, apparences /AP) ; ses libellés visibles sont
//! reconstruits.

use std::collections::{HashMap, HashSet};

use lopdf::{dictionary, Dictionary, Document, IncrementalDocument, Object, ObjectId};
use serde::Deserialize;

use super::appearance as ap;
use super::style::{base_font, rgb_op};
use super::*;
use crate::annot::pdf::text_object;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormPlan {
    /// Champs modifiés (état complet) ou créés (`isNew`).
    #[serde(default)]
    pub upsert: Vec<Field>,
    #[serde(default)]
    pub delete: Vec<String>,
}

/// Polices standard des apparences : celles de /DR si elles existent, sinon
/// ajoutées au document (et à /DR à la fin).
struct Fonts {
    map: HashMap<String, Object>,
    added: Vec<String>,
}

impl Fonts {
    fn new(doc: &Document) -> Fonts {
        let map = acroform(doc)
            .and_then(|f| get(doc, f, b"DR"))
            .and_then(|d| d.as_dict().ok())
            .and_then(|d| get(doc, d, b"Font"))
            .and_then(|f| f.as_dict().ok())
            .map(|f| f.iter().map(|(k, v)| (String::from_utf8_lossy(k).into_owned(), v.clone())).collect())
            .unwrap_or_default();
        Fonts { map, added: Vec::new() }
    }

    fn get(&mut self, inc: &mut IncrementalDocument, res: &str) -> Object {
        if let Some(o) = self.map.get(res) {
            return o.clone();
        }
        let mut d = dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => base_font(res) };
        if res != "ZaDb" {
            d.set("Encoding", "WinAnsiEncoding");
        }
        let o = Object::Reference(inc.new_document.add_object(d));
        self.map.insert(res.to_string(), o.clone());
        self.added.push(res.to_string());
        o
    }

    fn res(&mut self, inc: &mut IncrementalDocument, names: &[&str]) -> Dictionary {
        let mut d = Dictionary::new();
        for n in names {
            d.set(n.as_bytes().to_vec(), self.get(inc, n));
        }
        d
    }
}

fn dict_mut(inc: &mut IncrementalDocument, id: ObjectId) -> Result<&mut Dictionary, String> {
    inc.opt_clone_object_to_new_document(id).map_err(|e| e.to_string())?;
    inc.new_document.get_dictionary_mut(id).map_err(|e| e.to_string())
}

fn current(inc: &IncrementalDocument, id: ObjectId) -> Option<Dictionary> {
    inc.new_document.get_dictionary(id).ok().cloned().or_else(|| inc.get_prev_documents().get_dictionary(id).ok().cloned())
}

fn user_rect(page_ids: &[ObjectId], geoms: &HashMap<ObjectId, PageGeom>, page: u32, r: &Rect) -> Result<(ObjectId, Rect), String> {
    let pid = *page_ids.get(page as usize).ok_or("page introuvable")?;
    let g = geoms.get(&pid).copied().unwrap_or(PageGeom::new(0.0, 0.0, 612.0, 792.0, 0));
    Ok((pid, g.rect_to_user(r)))
}

fn rect_obj(r: &Rect) -> Object {
    Object::Array(vec![Object::Real(r.x), Object::Real(r.y), Object::Real(r.x + r.w), Object::Real(r.y + r.h)])
}

fn unique(used: &mut HashSet<String>, base: &str) -> String {
    let base = if base.is_empty() { "champ" } else { base };
    let mut name = base.to_string();
    let mut k = 2;
    while used.contains(&name) {
        name = format!("{base}_{k}");
        k += 1;
    }
    used.insert(name.clone());
    name
}

pub fn save(bytes: Vec<u8>, password: Option<&str>, plan: &FormPlan) -> Result<Vec<u8>, String> {
    let prev = load_mem(&bytes, password)?;
    // Champs sans apparence (formulaires « NeedAppearances ») : leur apparence est
    // générée aussi, pour que tous les lecteurs affichent celles d'Aura.
    let mut upserts: Vec<Field> = plan.upsert.clone();
    {
        let wanted: HashSet<&str> = plan.upsert.iter().map(|f| f.name.as_str()).chain(plan.delete.iter().map(|s| s.as_str())).collect();
        let lacking: HashSet<String> = nodes(&prev)
            .into_iter()
            .filter(|n| n.caption_of().is_none() && n.widgets.iter().any(|w| prev.get_dictionary(*w).map(|d| !d.has(b"AP")).unwrap_or(false)))
            .map(|n| n.name)
            .collect();
        for f in list(&prev) {
            if lacking.contains(&f.name) && !wanted.contains(f.name.as_str()) && matches!(f.kind.as_str(), "text" | "email" | "date" | "select" | "list" | "check" | "radio") {
                upserts.push(f);
            }
        }
    }
    let plan = &FormPlan { upsert: upserts, delete: plan.delete.clone() };
    let all = nodes(&prev);
    let pages = widget_pages(&prev);
    let page_ids: Vec<ObjectId> = prev.get_pages().values().copied().collect();
    let geoms = geoms(&prev);
    let form_id = prev.catalog().ok().and_then(|c| c.get(b"AcroForm").ok()).and_then(|o| o.as_reference().ok());
    let root_id = prev.trailer.get(b"Root").and_then(|o| o.as_reference()).map_err(|e| e.to_string())?;
    let has_form = acroform(&prev).is_some();
    let mut used: HashSet<String> = all.iter().map(|n| n.name.clone()).collect();
    let mut fonts = Fonts::new(&prev);
    let mut inc = IncrementalDocument::create_from(bytes, prev);

    // ---- Éléments retirés : champs supprimés, libellés des champs réécrits ou supprimés.
    let touched: HashSet<&str> = plan.upsert.iter().filter(|f| !f.is_new).map(|f| f.name.as_str()).chain(plan.delete.iter().map(|s| s.as_str())).collect();
    let mut removed: HashSet<ObjectId> = HashSet::new();
    for n in &all {
        let gone = match n.caption_of() {
            Some(owner) => touched.contains(owner.as_str()),
            None => plan.delete.contains(&n.name),
        };
        if gone {
            removed.insert(n.id);
            removed.extend(n.widgets.iter().copied());
        }
    }

    // ---- Champs réécrits ou créés, puis leurs libellés.
    let mut page_add: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
    let mut new_top: Vec<ObjectId> = Vec::new();
    let mut sig = false;
    for f in &plan.upsert {
        let (field_id, widget_ids, name) = if f.is_new {
            if f.widgets.is_empty() {
                continue;
            }
            let name = unique(&mut used, &slug(&f.label));
            if f.kind == "radio" || f.widgets.len() > 1 {
                let parent = inc.new_document.new_object_id();
                let mut kids = Vec::new();
                for w in &f.widgets {
                    let (pid, _) = user_rect(&page_ids, &geoms, w.page, &w.rect)?;
                    let kid = inc.new_document.add_object(dictionary! { "Parent" => parent });
                    page_add.entry(pid).or_default().push(kid);
                    kids.push(kid);
                }
                inc.new_document.set_object(parent, dictionary! { "T" => text_object(&name), "Kids" => kids.iter().map(|k| Object::Reference(*k)).collect::<Vec<_>>() });
                new_top.push(parent);
                (parent, kids, name)
            } else {
                let (pid, _) = user_rect(&page_ids, &geoms, f.widgets[0].page, &f.widgets[0].rect)?;
                let id = inc.new_document.add_object(dictionary! { "T" => text_object(&name) });
                page_add.entry(pid).or_default().push(id);
                new_top.push(id);
                (id, vec![id], name)
            }
        } else {
            let Some(n) = all.iter().find(|n| n.name == f.name && n.caption_of().is_none()) else { continue };
            (n.id, n.widgets.clone(), n.name.clone())
        };
        sig |= f.kind == "sig";
        write_field(&mut inc, &mut fonts, field_id, &widget_ids, f, &page_ids, &geoms)?;

        let mut caps: Vec<(i64, &Caption)> = f.caption.iter().map(|c| (-1, c)).collect();
        caps.extend(f.widgets.iter().enumerate().filter_map(|(i, w)| w.caption.as_ref().map(|c| (i as i64, c))));
        for (wi, c) in caps {
            if c.text.trim().is_empty() {
                continue;
            }
            let (pid, r) = user_rect(&page_ids, &geoms, c.page, &c.rect)?;
            let res = if c.bold { "HeBo" } else { "Helv" };
            let font = fonts.res(&mut inc, &[res]);
            let stream = ap::form(ap::caption(c, r.h), r.w, r.h, font);
            let ap_id = inc.new_document.add_object(stream);
            let cap_name = unique(&mut used, &format!("aura_libelle_{}", slug(&name)));
            let id = inc.new_document.add_object(dictionary! {
                "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn", "Ff" => FF_PUSH | FF_READONLY,
                "T" => text_object(&cap_name), "TU" => text_object(&c.text), "F" => 4, "P" => pid, "Rect" => rect_obj(&r),
                "DA" => Object::string_literal(format!("/{res} {} Tf {}", style::fmt(c.size), rgb_op(&c.color, false).unwrap_or_else(|| "0 g".into()))),
                "MK" => Dictionary::new(),
                "AP" => dictionary! { "N" => ap_id },
                "AuraCaptionOf" => text_object(&name),
                "AuraCaptionWidget" => wi,
                "AuraCaption" => dictionary! { "Text" => text_object(&c.text), "Size" => Object::Real(c.size), "Color" => Object::string_literal(c.color.clone()), "Bold" => c.bold },
            });
            page_add.entry(pid).or_default().push(id);
            new_top.push(id);
        }
    }

    // ---- Pages : annotations retirées ou ajoutées.
    let mut affected: HashSet<ObjectId> = page_add.keys().copied().collect();
    for r in &removed {
        if let Some((_, pid, _)) = pages.get(r) {
            affected.insert(*pid);
        }
    }
    for pid in affected {
        let current_annots: Vec<Object> = {
            let d = current(&inc, pid);
            let prev = inc.get_prev_documents();
            d.and_then(|p| get(prev, &p, b"Annots").and_then(|a| a.as_array().ok()).cloned().or_else(|| p.get(b"Annots").ok().and_then(|a| a.as_array().ok()).cloned()))
                .unwrap_or_default()
        };
        let mut list: Vec<Object> = current_annots.into_iter().filter(|o| !o.as_reference().map(|r| removed.contains(&r)).unwrap_or(false)).collect();
        list.extend(page_add.get(&pid).into_iter().flatten().map(|id| Object::Reference(*id)));
        dict_mut(&mut inc, pid)?.set("Annots", list);
    }

    // ---- Arbre des champs.
    if !removed.is_empty() {
        let parents: Vec<(ObjectId, Vec<Object>)> = {
            let prev = inc.get_prev_documents();
            prev.objects
                .iter()
                .filter_map(|(id, o)| {
                    let d = o.as_dict().ok()?;
                    let kids = d.get(b"Kids").ok()?.as_array().ok()?;
                    kids.iter().any(|k| k.as_reference().map(|r| removed.contains(&r)).unwrap_or(false)).then(|| (*id, kids.clone()))
                })
                .collect()
        };
        for (pid, kids) in parents {
            let keep: Vec<Object> = kids.into_iter().filter(|k| !k.as_reference().map(|r| removed.contains(&r)).unwrap_or(false)).collect();
            dict_mut(&mut inc, pid)?.set("Kids", keep);
        }
    }
    let current_fields: Vec<Object> = {
        let prev = inc.get_prev_documents();
        acroform(prev).and_then(|f| get(prev, f, b"Fields")).and_then(|a| a.as_array().ok()).cloned().unwrap_or_default()
    };
    let mut fields_arr: Vec<Object> = current_fields.into_iter().filter(|o| !o.as_reference().map(|r| removed.contains(&r)).unwrap_or(false)).collect();
    fields_arr.extend(new_top.iter().map(|id| Object::Reference(*id)));
    let fd_id = match form_id {
        Some(fid) => fid,
        None => {
            let fd = if has_form {
                let prev = inc.get_prev_documents();
                acroform(prev).cloned().unwrap_or_default()
            } else {
                dictionary! { "DA" => Object::string_literal("/Helv 0 Tf 0 g") }
            };
            let fid = inc.new_document.add_object(fd);
            dict_mut(&mut inc, root_id)?.set("AcroForm", fid);
            fid
        }
    };
    // Polices des apparences ajoutées aux ressources du formulaire.
    let added: Vec<(String, Object)> = fonts.added.iter().map(|n| (n.clone(), fonts.map[n].clone())).collect();
    let dr_ref = current(&inc, fd_id).and_then(|fd| fd.get(b"DR").ok().and_then(|o| o.as_reference().ok()));
    let font_ref = {
        let dr = match dr_ref {
            Some(id) => current(&inc, id),
            None => current(&inc, fd_id).and_then(|fd| fd.get(b"DR").ok().and_then(|o| o.as_dict().ok()).cloned()),
        };
        dr.and_then(|d| d.get(b"Font").ok().and_then(|o| o.as_reference().ok()))
    };
    if let Some(fid) = font_ref {
        let f = dict_mut(&mut inc, fid)?;
        for (n, o) in &added {
            f.set(n.as_bytes().to_vec(), o.clone());
        }
    } else {
        let target = match dr_ref {
            Some(id) => dict_mut(&mut inc, id)?,
            None => {
                let fd = dict_mut(&mut inc, fd_id)?;
                if !fd.has(b"DR") {
                    fd.set("DR", Dictionary::new());
                }
                fd.get_mut(b"DR").and_then(|o| o.as_dict_mut()).map_err(|e| e.to_string())?
            }
        };
        let mut fonts_d = target.get(b"Font").ok().and_then(|o| o.as_dict().ok()).cloned().unwrap_or_default();
        for (n, o) in &added {
            fonts_d.set(n.as_bytes().to_vec(), o.clone());
        }
        target.set("Font", fonts_d);
    }
    let fd = dict_mut(&mut inc, fd_id)?;
    fd.set("Fields", fields_arr);
    // Les apparences sont fournies : les lecteurs ne doivent pas les recalculer.
    fd.remove(b"NeedAppearances");
    if !fd.has(b"DA") {
        fd.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
    }
    if sig && !fd.has(b"SigFlags") {
        fd.set("SigFlags", 1);
    }

    let mut out = Vec::new();
    inc.save_to(&mut out).map_err(|e| format!("écriture du PDF : {e}"))?;
    Ok(out)
}

/// Réécrit un champ et ses widgets selon `f`.
fn write_field(inc: &mut IncrementalDocument, fonts: &mut Fonts, field_id: ObjectId, widget_ids: &[ObjectId], f: &Field, page_ids: &[ObjectId], geoms: &HashMap<ObjectId, PageGeom>) -> Result<(), String> {
    let kind = f.kind.as_str();
    let text_like = matches!(kind, "text" | "email" | "date");
    let choice = matches!(kind, "select" | "list");
    let button = matches!(kind, "check" | "radio");

    // États « activés » des cases et boutons radio.
    let ons: Vec<String> = widget_ids
        .iter()
        .enumerate()
        .map(|(i, wid)| {
            f.widgets.get(i).and_then(|w| w.on.clone()).filter(|s| !s.is_empty()).or_else(|| {
                let d = current(inc, *wid)?;
                on_state(inc.get_prev_documents(), &d)
            })
            .unwrap_or_else(|| if kind == "radio" { f.options.get(i).map(|o| o.value.clone()).unwrap_or_else(|| format!("Choix{}", i + 1)) } else { "Oui".into() })
        })
        .collect();

    // ---- Champ
    {
        let d = dict_mut(inc, field_id)?;
        let own = d.get(b"Ff").and_then(|o| o.as_i64()).unwrap_or(0);
        let mut ff = own & !(FF_REQUIRED | FF_READONLY | FF_MULTILINE | FF_COMBO | FF_RADIO | FF_NO_TOGGLE_OFF);
        if f.required {
            ff |= FF_REQUIRED;
        }
        if f.readonly {
            ff |= FF_READONLY;
        }
        match kind {
            "select" | "list" => {
                d.set("FT", "Ch");
                if kind == "select" {
                    ff |= FF_COMBO;
                }
                d.set(
                    "Opt",
                    f.options
                        .iter()
                        .map(|o| if o.value == o.label { text_object(&o.value) } else { Object::Array(vec![text_object(&o.value), text_object(&o.label)]) })
                        .collect::<Vec<_>>(),
                );
            }
            "radio" => {
                d.set("FT", "Btn");
                ff |= FF_RADIO | FF_NO_TOGGLE_OFF;
            }
            "check" => d.set("FT", "Btn"),
            "sig" => d.set("FT", "Sig"),
            "button" => {}
            _ => {
                d.set("FT", "Tx");
                if f.multiline {
                    ff |= FF_MULTILINE;
                }
            }
        }
        d.set("Ff", ff);
        d.set("TU", text_object(&f.label));
        if text_like || choice {
            if f.value.is_empty() {
                d.remove(b"V");
            } else {
                d.set("V", text_object(&f.value));
            }
        } else if button {
            let v = if f.value.is_empty() {
                "Off".to_string()
            } else if kind == "check" {
                ons.first().cloned().unwrap_or_else(|| "Oui".into())
            } else {
                f.value.clone()
            };
            d.set("V", Object::Name(v.into_bytes()));
        }
        if text_like {
            match f.max_len {
                Some(n) if n > 0 => d.set("MaxLen", n as i64),
                _ => {
                    d.remove(b"MaxLen");
                }
            }
            if f.placeholder.is_empty() {
                d.remove(b"AuraPlaceholder");
            } else {
                d.set("AuraPlaceholder", text_object(&f.placeholder));
            }
            if kind == "text" {
                d.remove(b"AuraKind");
            } else {
                d.set("AuraKind", Object::Name(kind.as_bytes().to_vec()));
            }
            if kind == "date" && !d.has(b"AA") {
                let js = |n: &str| dictionary! { "S" => "JavaScript", "JS" => Object::string_literal(format!("{n}(\"dd/mm/yyyy\");")) };
                d.set("AA", dictionary! { "F" => js("AFDate_FormatEx"), "K" => js("AFDate_KeystrokeEx") });
            }
        }
        if kind != "sig" {
            d.set("DA", Object::string_literal(f.style.da()));
            d.set("Q", f.style.align as i64);
        }
    }

    // ---- Widgets
    let shown = if choice { f.options.iter().find(|o| o.value == f.value).map(|o| o.label.clone()).unwrap_or_else(|| f.value.clone()) } else { f.value.clone() };
    for (i, wid) in widget_ids.iter().enumerate() {
        let Some(spec) = f.widgets.get(i) else { continue };
        let (pid, r) = user_rect(page_ids, geoms, spec.page, &spec.rect)?;
        let (bw, bh) = (r.w.max(1.0), r.h.max(1.0));
        let res = f.style.font_res();
        let ap_obj: Option<Object> = if text_like || choice {
            let fnt = fonts.res(inc, &[res]);
            let content = ap::text(&f.style, bw, bh, &shown, if choice { "" } else { &f.placeholder }, f.multiline && text_like);
            Some(Object::Reference(inc.new_document.add_object(ap::form(content, bw, bh, fnt))))
        } else if button {
            let fnt = fonts.res(inc, &["ZaDb"]);
            let (on, off) = ap::check(&f.style, bw, bh, kind == "radio");
            let on_id = inc.new_document.add_object(ap::form(on, bw, bh, fnt.clone()));
            let off_id = inc.new_document.add_object(ap::form(off, bw, bh, fnt));
            let mut n = Dictionary::new();
            n.set(ons[i].as_bytes().to_vec(), on_id);
            n.set("Off", off_id);
            Some(Object::Dictionary(dictionary! { "N" => n }))
        } else if kind == "sig" && (f.style.background.is_some() || f.style.border_width > 0.0) {
            let content = ap::frame(&f.style, bw, bh, false).into_bytes();
            Some(Object::Reference(inc.new_document.add_object(ap::form(content, bw, bh, Dictionary::new()))))
        } else {
            None
        };
        let w = dict_mut(inc, *wid)?;
        w.set("Type", "Annot");
        w.set("Subtype", "Widget");
        w.set("Rect", rect_obj(&r));
        w.set("P", pid);
        let flags = w.get(b"F").and_then(|o| o.as_i64()).unwrap_or(0);
        w.set("F", (flags | 4) & !(1 | 2 | 32));
        let mut mk = f.style.mk(kind);
        if let Some(rot) = w.get(b"MK").ok().and_then(|o| o.as_dict().ok()).and_then(|m| m.get(b"R").ok()).cloned() {
            mk.set("R", rot);
        }
        w.set("MK", mk);
        w.set("BS", f.style.bs());
        if kind != "sig" {
            w.set("DA", Object::string_literal(f.style.da()));
        }
        match ap_obj {
            Some(Object::Dictionary(d)) => w.set("AP", d),
            Some(o) => w.set("AP", dictionary! { "N" => o }),
            None => {}
        }
        if button {
            let on = !f.value.is_empty() && (kind == "check" || f.value == ons[i]);
            w.set("AS", Object::Name(if on { ons[i].clone() } else { "Off".into() }.into_bytes()));
        }
    }
    Ok(())
}
