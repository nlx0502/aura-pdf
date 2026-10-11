//! Caviardage définitif.
//!
//! Le flux de contenu de la page est interprété (état graphique, état du
//! texte, polices) pour situer chaque glyphe :
//! - les glyphes sous une zone sont retirés du flux ; les suivants gardent leur
//!   position exacte (le déplacement est compensé dans `TJ`) ;
//! - les pixels des images sous une zone sont remplacés (image recodée), ou
//!   l'image est retirée si son format ne peut pas être décodé ;
//! - les formulaires XObject sont traités récursivement (copie propre à la page) ;
//! - les annotations qui touchent une zone sont supprimées ;
//! - un rectangle noir est dessiné sur chaque zone.
//!
//! Le fichier est ensuite réécrit en entier (sans historique) et purgé des
//! objets devenus inutiles : le contenu retiré n'y figure plus.

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use super::fonts::{FontCache, FontMetrics};
use super::stamp::{own_resources, sub_dict};
use crate::annot::ap::n;
use crate::annot::geom::Rect;
use crate::annot::pdf::{deref, get, num, page_geom};

type M = [f32; 6];
const IDENTITY: M = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn mul(a: &M, b: &M) -> M {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn apply_m(m: &M, x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

fn invert(m: &M) -> Option<M> {
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < 1e-9 {
        return None;
    }
    let (a, b, c, d) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    Some([a, b, c, d, -(m[4] * a + m[5] * c), -(m[4] * b + m[5] * d)])
}

fn translate(tx: f32, ty: f32) -> M {
    [1.0, 0.0, 0.0, 1.0, tx, ty]
}

/// Rectangle en espace utilisateur.
#[derive(Debug, Clone, Copy)]
pub struct Area {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Area {
    fn from_rect(r: &Rect) -> Self {
        Area { x0: r.x, y0: r.y, x1: r.x + r.w, y1: r.y + r.h }
    }
    fn intersects(&self, o: &Area) -> bool {
        self.x0 < o.x1 && o.x0 < self.x1 && self.y0 < o.y1 && o.y0 < self.y1
    }
    fn overlap(&self, o: &Area) -> f32 {
        let w = (self.x1.min(o.x1) - self.x0.max(o.x0)).max(0.0);
        let h = (self.y1.min(o.y1) - self.y0.max(o.y0)).max(0.0);
        w * h
    }
    fn area(&self) -> f32 {
        (self.x1 - self.x0).max(0.0) * (self.y1 - self.y0).max(0.0)
    }
    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
    /// Boîte englobante d'un parallélogramme (image d'un rectangle par une matrice).
    fn of_box(m: &M, x0: f32, y0: f32, x1: f32, y1: f32) -> Area {
        let pts = [apply_m(m, x0, y0), apply_m(m, x1, y0), apply_m(m, x0, y1), apply_m(m, x1, y1)];
        let (mut a, mut b, mut c, mut d) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (x, y) in pts {
            a = a.min(x);
            b = b.min(y);
            c = c.max(x);
            d = d.max(y);
        }
        Area { x0: a, y0: b, x1: c, y1: d }
    }
}

#[derive(Clone)]
struct GState {
    ctm: M,
    font: Option<Vec<u8>>,
    fs: f32,
    tc: f32,
    tw: f32,
    th: f32,
    tl: f32,
    rise: f32,
}

impl GState {
    fn new(ctm: M) -> Self {
        GState { ctm, font: None, fs: 0.0, tc: 0.0, tw: 0.0, th: 1.0, tl: 0.0, rise: 0.0 }
    }
}

struct Redactor<'a> {
    doc: &'a mut Document,
    areas: Vec<Area>,
    fonts: FontCache,
    seq: u32,
    /// XObjects d'origine remplacés ou retirés (à ne plus référencer nulle part).
    replaced: Vec<ObjectId>,
}

/// Résultat du traitement d'un flux.
struct Processed {
    ops: Vec<Operation>,
    changed: bool,
}

/// Caviarde une page. Renvoie les XObjects d'origine remplacés par une copie
/// caviardée : l'appelant retire les références restantes des autres pages.
pub fn redact_page(doc: &mut Document, page: ObjectId, rects: &[Rect]) -> Result<Vec<ObjectId>, String> {
    let pd = doc.get_dictionary(page).map_err(|_| "page introuvable")?.clone();
    let g = page_geom(doc, &pd);
    let areas: Vec<Area> = rects.iter().filter(|r| r.w > 0.0 && r.h > 0.0).map(|r| Area::from_rect(&g.rect_to_user(r))).collect();
    if areas.is_empty() {
        return Ok(Vec::new());
    }

    let content = page_content(doc, &pd)?;
    let ops = if content.iter().all(|b| b.is_ascii_whitespace()) {
        Vec::new()
    } else {
        Content::decode(&content).map_err(|e| format!("contenu illisible, caviardage impossible ({e})"))?.operations
    };
    let mut res = own_resources(doc, page);

    let mut r = Redactor { doc, areas: areas.clone(), fonts: FontCache::default(), seq: 0, replaced: Vec::new() };
    let processed = r.process(ops, &mut res, IDENTITY, 0)?;

    let mut bytes = b"q\n".to_vec();
    bytes.extend_from_slice(&encode(&processed.ops)?);
    bytes.extend_from_slice(b"\nQ\n");
    for a in &areas {
        bytes.extend_from_slice(format!("q 0 0 0 rg {} {} {} {} re f Q\n", n(a.x0), n(a.y0), n(a.x1 - a.x0), n(a.y1 - a.y0)).as_bytes());
    }
    let mut stream = Stream::new(Dictionary::new(), bytes);
    let _ = stream.compress();
    let content_id = r.doc.add_object(stream);
    remove_annotations(r.doc, page, &areas);
    if let Ok(d) = r.doc.get_dictionary_mut(page) {
        d.set("Contents", content_id);
        d.set("Resources", res);
    }
    Ok(r.replaced)
}

/// Contenu décompressé de la page (flux concaténés).
fn page_content(doc: &Document, page: &Dictionary) -> Result<Vec<u8>, String> {
    let mut streams: Vec<&Stream> = Vec::new();
    match page.get(b"Contents").ok().map(|o| deref(doc, o)) {
        Some(Object::Stream(s)) => streams.push(s),
        Some(Object::Array(a)) => {
            for o in a {
                if let Object::Stream(s) = deref(doc, o) {
                    streams.push(s);
                }
            }
        }
        _ => {}
    }
    let mut out = Vec::new();
    for s in streams {
        let data = if s.dict.has(b"Filter") {
            s.decompressed_content().map_err(|e| format!("flux de contenu illisible ({e})"))?
        } else {
            s.content.clone()
        };
        out.extend_from_slice(&data);
        out.push(b'\n');
    }
    Ok(out)
}

fn f(o: &Object) -> f32 {
    num(o).unwrap_or(0.0)
}

fn matrix_of(ops: &[Object]) -> Option<M> {
    if ops.len() < 6 {
        return None;
    }
    Some([f(&ops[0]), f(&ops[1]), f(&ops[2]), f(&ops[3]), f(&ops[4]), f(&ops[5])])
}

impl Redactor<'_> {
    fn hits(&self, a: &Area) -> bool {
        self.areas.iter().any(|z| z.intersects(a))
    }

    /// Le glyphe doit-il disparaître ? (centre dans une zone, ou recouvert à 30 %)
    fn glyph_hit(&self, g: &Area) -> bool {
        let (cx, cy) = ((g.x0 + g.x1) / 2.0, (g.y0 + g.y1) / 2.0);
        let ga = g.area();
        self.areas.iter().any(|z| z.contains(cx, cy) || (ga > 0.0 && z.overlap(g) / ga >= 0.3))
    }

    fn fresh_name(&mut self, prefix: &str) -> Vec<u8> {
        self.seq += 1;
        format!("{prefix}{}", self.seq).into_bytes()
    }

    fn process(&mut self, ops: Vec<Operation>, res: &mut Dictionary, ctm: M, depth: u32) -> Result<Processed, String> {
        let mut out: Vec<Operation> = Vec::with_capacity(ops.len());
        let mut changed = false;
        let mut dropped: Vec<Vec<u8>> = Vec::new();
        let mut gs = GState::new(ctm);
        let mut stack: Vec<GState> = Vec::new();
        let mut tm = IDENTITY;
        let mut tlm = IDENTITY;

        for op in ops {
            let o = &op.operands;
            match op.operator.as_str() {
                "q" => stack.push(gs.clone()),
                "Q" => {
                    if let Some(s) = stack.pop() {
                        gs = s;
                    }
                }
                "cm" => {
                    if let Some(m) = matrix_of(o) {
                        gs.ctm = mul(&m, &gs.ctm);
                    }
                }
                "BT" => {
                    tm = IDENTITY;
                    tlm = IDENTITY;
                }
                "Tf" if o.len() >= 2 => {
                    gs.font = o[0].as_name().ok().map(|n| n.to_vec());
                    gs.fs = f(&o[1]);
                }
                "Tc" if !o.is_empty() => gs.tc = f(&o[0]),
                "Tw" if !o.is_empty() => gs.tw = f(&o[0]),
                "Tz" if !o.is_empty() => gs.th = f(&o[0]) / 100.0,
                "TL" if !o.is_empty() => gs.tl = f(&o[0]),
                "Ts" if !o.is_empty() => gs.rise = f(&o[0]),
                "Td" | "TD" if o.len() >= 2 => {
                    if op.operator == "TD" {
                        gs.tl = -f(&o[1]);
                    }
                    tlm = mul(&translate(f(&o[0]), f(&o[1])), &tlm);
                    tm = tlm;
                }
                "Tm" => {
                    if let Some(m) = matrix_of(o) {
                        tlm = m;
                        tm = m;
                    }
                }
                "T*" => {
                    tlm = mul(&translate(0.0, -gs.tl), &tlm);
                    tm = tlm;
                }
                "Tj" | "TJ" | "'" | "\"" => {
                    let (prefix, elems): (Vec<Operation>, Vec<Object>) = match op.operator.as_str() {
                        "Tj" => (vec![], o.first().cloned().into_iter().collect()),
                        "TJ" => (vec![], o.first().and_then(|a| a.as_array().ok()).cloned().unwrap_or_default()),
                        "'" => (vec![Operation::new("T*", vec![])], o.first().cloned().into_iter().collect()),
                        _ => {
                            if o.len() >= 3 {
                                gs.tw = f(&o[0]);
                                gs.tc = f(&o[1]);
                            }
                            (
                                vec![
                                    Operation::new("Tw", vec![o.first().cloned().unwrap_or(Object::Integer(0))]),
                                    Operation::new("Tc", vec![o.get(1).cloned().unwrap_or(Object::Integer(0))]),
                                    Operation::new("T*", vec![]),
                                ],
                                o.get(2).cloned().into_iter().collect(),
                            )
                        }
                    };
                    if !prefix.is_empty() {
                        tlm = mul(&translate(0.0, -gs.tl), &tlm);
                        tm = tlm;
                    }
                    let metrics = self.font_metrics(res, &gs);
                    match self.show(&elems, &gs, &mut tm, &metrics) {
                        None => out.push(op),
                        Some(new_elems) => {
                            changed = true;
                            out.extend(prefix);
                            if !new_elems.is_empty() {
                                out.push(Operation::new("TJ", vec![Object::Array(new_elems)]));
                            }
                        }
                    }
                    continue;
                }
                "Do" => {
                    if let Some(name) = o.first().and_then(|n| n.as_name().ok()).map(|n| n.to_vec()) {
                        match self.xobject(res, &name, &gs.ctm, depth)? {
                            XAction::Keep => out.push(op),
                            XAction::Remove => {
                                changed = true;
                                dropped.push(name);
                            }
                            XAction::Rename(new) => {
                                changed = true;
                                dropped.push(name);
                                out.push(Operation::new("Do", vec![Object::Name(new)]));
                            }
                        }
                        continue;
                    }
                }
                "BI" => {
                    let a = Area::of_box(&gs.ctm, 0.0, 0.0, 1.0, 1.0);
                    if o.is_empty() || self.hits(&a) {
                        // Image en ligne sous une zone (ou illisible) : retirée.
                        changed = true;
                        continue;
                    }
                }
                _ => {}
            }
            out.push(op);
        }
        // Les XObjects remplacés ou retirés ne doivent plus être référencés
        // (sinon l'original resterait dans le fichier).
        if !dropped.is_empty() {
            let used: std::collections::HashSet<Vec<u8>> = out
                .iter()
                .filter(|o| o.operator == "Do")
                .filter_map(|o| o.operands.first().and_then(|n| n.as_name().ok()).map(|n| n.to_vec()))
                .collect();
            let mut x = sub_dict(self.doc, res, b"XObject");
            for name in dropped {
                if !used.contains(&name) {
                    x.remove(&name);
                }
            }
            res.set("XObject", x);
        }
        Ok(Processed { ops: out, changed })
    }

    fn font_metrics(&mut self, res: &Dictionary, gs: &GState) -> FontMetrics {
        let Some(name) = &gs.font else { return FontMetrics::fallback() };
        let fonts = res.get(b"Font").ok().map(|o| deref(self.doc, o)).and_then(|o| o.as_dict().ok());
        match fonts.and_then(|d| d.get(name).ok()) {
            Some(fo) => {
                let fo = fo.clone();
                self.fonts.get(self.doc, &fo)
            }
            None => FontMetrics::fallback(),
        }
    }

    /// Parcourt une opération d'affichage de texte. Renvoie `None` si rien n'est
    /// retiré, sinon le nouveau tableau `TJ` (vide si tout disparaît).
    fn show(&self, elems: &[Object], gs: &GState, tm: &mut M, m: &FontMetrics) -> Option<Vec<Object>> {
        let trm0: M = [gs.fs * gs.th, 0.0, 0.0, gs.fs, 0.0, gs.rise];
        let (asc, desc) = (m.ascent / 1000.0, m.descent / 1000.0);
        let unit = gs.fs * gs.th;

        // Codage incertain : on retire l'opération entière si elle touche une zone.
        if m.uncertain {
            let mut t = *tm;
            let mut bbox: Option<Area> = None;
            let mut total = 0.0f32;
            for e in elems {
                match e {
                    Object::String(s, _) => {
                        for (code, len) in m.split(s) {
                            let w0 = m.width(code) / 1000.0;
                            let full = mul(&mul(&trm0, &t), &gs.ctm);
                            let g = Area::of_box(&full, -0.2, desc - 0.3, w0.max(0.5) + 0.2, asc + 0.3);
                            bbox = Some(match bbox {
                                Some(b) => Area { x0: b.x0.min(g.x0), y0: b.y0.min(g.y0), x1: b.x1.max(g.x1), y1: b.y1.max(g.y1) },
                                None => g,
                            });
                            let sp = if !m.composite && len == 1 && code == 32 { gs.tw } else { 0.0 };
                            let adv = (w0 * gs.fs + gs.tc + sp) * gs.th;
                            total += adv;
                            t = mul(&translate(adv, 0.0), &t);
                        }
                    }
                    other => {
                        if let Some(v) = num(other) {
                            total += -v / 1000.0 * unit;
                            t = mul(&translate(-v / 1000.0 * unit, 0.0), &t);
                        }
                    }
                }
            }
            *tm = t;
            return match bbox {
                // Tout le texte disparaît ; le déplacement est conservé pour la suite de la ligne.
                Some(b) if self.hits(&b) => Some(if unit.abs() > 1e-6 {
                    vec![Object::String(Vec::new(), lopdf::StringFormat::Hexadecimal), Object::Real(-total / unit * 1000.0)]
                } else {
                    Vec::new()
                }),
                _ => None,
            };
        }

        let mut out: Vec<Object> = Vec::new();
        let mut cur: Vec<u8> = Vec::new();
        let mut pending = 0.0f32; // déplacement des glyphes retirés, en espace texte
        let mut removed_any = false;
        let mut kept_any = false;

        let flush_str = |out: &mut Vec<Object>, cur: &mut Vec<u8>| {
            if !cur.is_empty() {
                out.push(Object::String(std::mem::take(cur), lopdf::StringFormat::Hexadecimal));
            }
        };
        let push_num = |out: &mut Vec<Object>, v: f32| {
            if v.abs() < 1e-4 {
                return;
            }
            if let Some(Object::Real(last)) = out.last_mut() {
                *last += v;
            } else {
                out.push(Object::Real(v));
            }
        };

        for e in elems {
            match e {
                Object::String(s, _) => {
                    for (code, len) in m.split(s) {
                        let w0 = m.width(code) / 1000.0;
                        let full = mul(&mul(&trm0, tm), &gs.ctm);
                        let g = Area::of_box(&full, 0.0, desc, w0.max(0.25), asc);
                        let sp = if !m.composite && len == 1 && code == 32 { gs.tw } else { 0.0 };
                        let adv = (w0 * gs.fs + gs.tc + sp) * gs.th;
                        *tm = mul(&translate(adv, 0.0), tm);
                        let bytes = code_bytes(code, len);
                        if self.glyph_hit(&g) {
                            removed_any = true;
                            pending += adv;
                        } else {
                            if pending != 0.0 {
                                flush_str(&mut out, &mut cur);
                                if unit.abs() > 1e-6 {
                                    push_num(&mut out, -pending / unit * 1000.0);
                                }
                                pending = 0.0;
                            }
                            cur.extend_from_slice(&bytes);
                            kept_any = true;
                        }
                    }
                }
                other => {
                    if let Some(v) = num(other) {
                        *tm = mul(&translate(-v / 1000.0 * unit, 0.0), tm);
                        flush_str(&mut out, &mut cur);
                        if pending != 0.0 && unit.abs() > 1e-6 {
                            push_num(&mut out, -pending / unit * 1000.0);
                            pending = 0.0;
                        }
                        push_num(&mut out, v);
                    }
                }
            }
        }
        if !removed_any {
            return None;
        }
        flush_str(&mut out, &mut cur);
        if pending != 0.0 && unit.abs() > 1e-6 {
            push_num(&mut out, -pending / unit * 1000.0);
        }
        if !kept_any {
            // Plus aucun glyphe : on garde le déplacement (état de texte cohérent).
            out.retain(|o| matches!(o, Object::Real(_)));
            if out.is_empty() {
                return Some(Vec::new());
            }
            out.insert(0, Object::String(Vec::new(), lopdf::StringFormat::Hexadecimal));
        }
        Some(out)
    }

    fn xobject(&mut self, res: &mut Dictionary, name: &[u8], ctm: &M, depth: u32) -> Result<XAction, String> {
        let xobjs = sub_dict(self.doc, res, b"XObject");
        let Some(entry) = xobjs.get(name).ok().cloned() else { return Ok(XAction::Keep) };
        let action = self.xobject_action(res, &entry, ctm, depth)?;
        if !matches!(action, XAction::Keep) {
            if let Ok(id) = entry.as_reference() {
                self.replaced.push(id);
            }
        }
        Ok(action)
    }

    fn xobject_action(&mut self, res: &mut Dictionary, entry: &Object, ctm: &M, depth: u32) -> Result<XAction, String> {
        let xobjs = sub_dict(self.doc, res, b"XObject");
        let s = match deref(self.doc, entry) {
            Object::Stream(s) => s.clone(),
            _ => return Ok(XAction::Keep),
        };
        let subtype = s.dict.get(b"Subtype").and_then(|o| o.as_name()).unwrap_or(b"");
        match subtype {
            b"Image" => {
                let a = Area::of_box(ctm, 0.0, 0.0, 1.0, 1.0);
                if !self.hits(&a) {
                    return Ok(XAction::Keep);
                }
                match redact_image(self.doc, &s, ctm, &self.areas) {
                    None => Ok(XAction::Remove),
                    Some(new_stream) => {
                        let id = self.doc.add_object(new_stream);
                        let new = self.fresh_name("AuraRdIm");
                        let mut x = xobjs;
                        x.set(new.clone(), id);
                        res.set("XObject", x);
                        Ok(XAction::Rename(new))
                    }
                }
            }
            b"Form" => {
                let fm = get(self.doc, &s.dict, b"Matrix").and_then(|o| o.as_array().ok()).and_then(|a| matrix_of(a)).unwrap_or(IDENTITY);
                let fctm = mul(&fm, ctm);
                let bbox = get(self.doc, &s.dict, b"BBox")
                    .and_then(|o| o.as_array().ok())
                    .filter(|a| a.len() == 4)
                    .map(|a| [f(deref(self.doc, &a[0])), f(deref(self.doc, &a[1])), f(deref(self.doc, &a[2])), f(deref(self.doc, &a[3]))]);
                if let Some(b) = bbox {
                    if !self.hits(&Area::of_box(&fctm, b[0].min(b[2]), b[1].min(b[3]), b[0].max(b[2]), b[1].max(b[3]))) {
                        return Ok(XAction::Keep);
                    }
                }
                if depth > 12 {
                    return Ok(XAction::Remove);
                }
                let data = if s.dict.has(b"Filter") {
                    s.decompressed_content().map_err(|e| format!("formulaire illisible ({e})"))?
                } else {
                    s.content.clone()
                };
                let ops = if data.iter().all(|b| b.is_ascii_whitespace()) {
                    Vec::new()
                } else {
                    match Content::decode(&data) {
                        Ok(c) => c.operations,
                        // Contenu illisible sous une zone : retiré par prudence.
                        Err(_) => return Ok(XAction::Remove),
                    }
                };
                let mut fres = match s.dict.get(b"Resources").ok().map(|o| deref(self.doc, o)) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => res.clone(),
                };
                let p = self.process(ops, &mut fres, fctm, depth + 1)?;
                if !p.changed {
                    return Ok(XAction::Keep);
                }
                let mut dict = s.dict.clone();
                dict.remove(b"Filter");
                dict.remove(b"DecodeParms");
                dict.remove(b"Length");
                dict.set("Resources", fres);
                let mut ns = Stream::new(dict, encode(&p.ops)?);
                let _ = ns.compress();
                let id = self.doc.add_object(ns);
                let new = self.fresh_name("AuraRdFm");
                let mut x = sub_dict(self.doc, res, b"XObject");
                x.set(new.clone(), id);
                res.set("XObject", x);
                Ok(XAction::Rename(new))
            }
            _ => Ok(XAction::Keep),
        }
    }
}

enum XAction {
    Keep,
    Remove,
    Rename(Vec<u8>),
}

fn code_bytes(code: u32, len: usize) -> Vec<u8> {
    (0..len).rev().map(|i| ((code >> (8 * i)) & 0xFF) as u8).collect()
}

/// Nombre de composantes d'un espace de couleurs (None si non pris en charge).
fn components(doc: &Document, cs: Option<&Object>) -> Option<usize> {
    let cs = cs.map(|o| deref(doc, o))?;
    match cs {
        Object::Name(n) => match n.as_slice() {
            b"DeviceGray" | b"CalGray" | b"G" => Some(1),
            b"DeviceRGB" | b"CalRGB" | b"RGB" => Some(3),
            b"DeviceCMYK" | b"CMYK" => Some(4),
            _ => None,
        },
        Object::Array(a) => {
            let kind = a.first().and_then(|o| o.as_name().ok())?;
            match kind {
                b"ICCBased" => {
                    let s = deref(doc, a.get(1)?).as_stream().ok()?;
                    s.dict.get(b"N").ok().and_then(|o| o.as_i64().ok()).map(|v| v as usize)
                }
                b"Indexed" | b"I" | b"Separation" => Some(1),
                b"DeviceN" => deref(doc, a.get(1)?).as_array().ok().map(|x| x.len()),
                b"CalGray" => Some(1),
                b"CalRGB" | b"Lab" => Some(3),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Efface les pixels d'une image situés sous les zones. `None` : image à retirer.
fn redact_image(doc: &Document, s: &Stream, ctm: &M, areas: &[Area]) -> Option<Stream> {
    let d = &s.dict;
    if d.get(b"ImageMask").and_then(|o| o.as_bool()).unwrap_or(false) {
        return None;
    }
    let w = get(doc, d, b"Width").and_then(|o| o.as_i64().ok())? as usize;
    let h = get(doc, d, b"Height").and_then(|o| o.as_i64().ok())? as usize;
    if w == 0 || h == 0 || w * h > 80_000_000 {
        return None;
    }
    let filters: Vec<Vec<u8>> = match d.get(b"Filter").ok().map(|o| deref(doc, o)) {
        Some(Object::Name(n)) => vec![n.clone()],
        Some(Object::Array(a)) => a.iter().filter_map(|o| o.as_name().ok().map(|n| n.to_vec())).collect(),
        _ => vec![],
    };
    let mut dict = d.clone();
    let (mut data, comps, bpc) = if filters.last().map(|f| f.as_slice() == b"DCTDecode").unwrap_or(false) {
        // JPEG : décodé puis réécrit sans perte supplémentaire (Flate).
        let raw = if filters.len() > 1 {
            let mut tmp = s.clone();
            tmp.dict.set("Filter", Object::Array(filters[..filters.len() - 1].iter().map(|f| Object::Name(f.clone())).collect()));
            tmp.decompressed_content().ok()?
        } else {
            s.content.clone()
        };
        let img = image::load_from_memory_with_format(&raw, image::ImageFormat::Jpeg).ok()?;
        let (bytes, comps, cs) = if img.color().channel_count() == 1 {
            (img.into_luma8().into_raw(), 1, "DeviceGray")
        } else {
            (img.into_rgb8().into_raw(), 3, "DeviceRGB")
        };
        dict.set("ColorSpace", Object::Name(cs.as_bytes().to_vec()));
        dict.set("BitsPerComponent", 8);
        dict.remove(b"Decode");
        (bytes, comps, 8usize)
    } else {
        let supported = filters.iter().all(|f| matches!(f.as_slice(), b"FlateDecode" | b"Fl" | b"LZWDecode" | b"LZW" | b"ASCII85Decode" | b"A85" | b"ASCIIHexDecode" | b"AHx" | b"RunLengthDecode" | b"RL"));
        if !supported {
            return None;
        }
        let raw = if filters.is_empty() { s.content.clone() } else { s.decompressed_content().ok()? };
        let comps = components(doc, d.get(b"ColorSpace").ok())?;
        let bpc = get(doc, d, b"BitsPerComponent").and_then(|o| o.as_i64().ok()).unwrap_or(8) as usize;
        if !matches!(bpc, 1 | 2 | 4 | 8 | 16) {
            return None;
        }
        (raw, comps, bpc)
    };
    let row_bytes = (w * comps * bpc).div_ceil(8);
    if data.len() < row_bytes * h {
        return None;
    }
    // Zones → pixels de l'image (l'image occupe le carré unité, ligne 0 en haut).
    let inv = invert(ctm)?;
    for a in areas {
        let u = Area::of_box(&inv, a.x0, a.y0, a.x1, a.y1);
        if u.x1 <= 0.0 || u.x0 >= 1.0 || u.y1 <= 0.0 || u.y0 >= 1.0 {
            continue;
        }
        let px0 = ((u.x0 * w as f32).floor() as i64 - 1).clamp(0, w as i64) as usize;
        let px1 = ((u.x1 * w as f32).ceil() as i64 + 1).clamp(0, w as i64) as usize;
        let py0 = (((1.0 - u.y1) * h as f32).floor() as i64 - 1).clamp(0, h as i64) as usize;
        let py1 = (((1.0 - u.y0) * h as f32).ceil() as i64 + 1).clamp(0, h as i64) as usize;
        for y in py0..py1 {
            let row = &mut data[y * row_bytes..(y + 1) * row_bytes];
            if bpc % 8 == 0 {
                let bytes_px = comps * bpc / 8;
                row[px0 * bytes_px..px1 * bytes_px].fill(0);
            } else {
                for x in px0..px1 {
                    for c in 0..comps {
                        let bit = (x * comps + c) * bpc;
                        for b in bit..bit + bpc {
                            row[b / 8] &= !(0x80u8 >> (b % 8));
                        }
                    }
                }
            }
        }
    }
    // Masque de transparence : effacé aux mêmes endroits.
    if let Some(Object::Reference(smask_id)) = d.get(b"SMask").ok() {
        if let Ok(Object::Stream(ms)) = doc.get_object(*smask_id) {
            match redact_image(doc, ms, ctm, areas) {
                Some(ns) => {
                    dict.set("SMask", Object::Stream(ns));
                }
                None => {
                    dict.remove(b"SMask");
                }
            }
        }
    }
    dict.remove(b"Filter");
    dict.remove(b"DecodeParms");
    dict.remove(b"Length");
    let mut ns = Stream::new(dict, data);
    let _ = ns.compress();
    Some(ns)
}

/// Supprime les annotations qui touchent une zone (et efface la valeur des champs).
fn remove_annotations(doc: &mut Document, page: ObjectId, areas: &[Area]) {
    let Ok(pd) = doc.get_dictionary(page) else { return };
    let Some(list) = get(doc, pd, b"Annots").and_then(|a| a.as_array().ok()).cloned() else { return };
    let mut keep = Vec::new();
    let mut removed: Vec<ObjectId> = Vec::new();
    let mut widgets: Vec<ObjectId> = Vec::new();
    for o in &list {
        let Ok(d) = deref(doc, o).as_dict() else { continue };
        let rect = get(doc, d, b"Rect").and_then(|r| r.as_array().ok()).filter(|a| a.len() == 4).map(|a| {
            let v: Vec<f32> = a.iter().map(|x| f(deref(doc, x))).collect();
            Area { x0: v[0].min(v[2]), y0: v[1].min(v[3]), x1: v[0].max(v[2]), y1: v[1].max(v[3]) }
        });
        let hit = rect.map(|r| areas.iter().any(|a| a.intersects(&r))).unwrap_or(false);
        let parent_removed = matches!(d.get(b"Parent"), Ok(Object::Reference(p)) if removed.contains(p));
        if hit || parent_removed {
            if let Ok(id) = o.as_reference() {
                removed.push(id);
                if d.get(b"Subtype").and_then(|s| s.as_name()).ok() == Some(b"Widget".as_slice()) {
                    widgets.push(id);
                }
            }
            continue;
        }
        keep.push(o.clone());
    }
    if removed.is_empty() {
        return;
    }
    // Popups dont l'annotation parente a disparu (elles peuvent la précéder dans la liste).
    keep.retain(|o| {
        let parent = deref(doc, o).as_dict().ok().and_then(|d| d.get(b"Parent").ok()).and_then(|p| p.as_reference().ok());
        !matches!(parent, Some(p) if removed.contains(&p))
    });
    for id in widgets {
        let parent = doc.get_dictionary(id).ok().and_then(|d| d.get(b"Parent").ok()).and_then(|p| p.as_reference().ok());
        for target in std::iter::once(id).chain(parent) {
            if let Ok(d) = doc.get_dictionary_mut(target) {
                d.remove(b"V");
                d.remove(b"AP");
            }
        }
    }
    if let Ok(d) = doc.get_dictionary_mut(page) {
        d.set("Annots", keep);
    }
}

/// Encode des opérations de contenu (y compris les images en ligne, que
/// lopdf ne sait pas réécrire).
pub fn encode(ops: &[Operation]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut batch: Vec<Operation> = Vec::new();
    let flush = |batch: &mut Vec<Operation>, out: &mut Vec<u8>| -> Result<(), String> {
        if batch.is_empty() {
            return Ok(());
        }
        let c = Content { operations: std::mem::take(batch) };
        out.extend_from_slice(&c.encode().map_err(|e| e.to_string())?);
        out.push(b'\n');
        Ok(())
    };
    for op in ops {
        if op.operator == "BI" {
            flush(&mut batch, &mut out)?;
            let Some(Object::Stream(s)) = op.operands.first() else { continue };
            out.extend_from_slice(b"BI");
            for (k, v) in s.dict.iter().filter(|(k, _)| k.as_slice() != b"Length") {
                let c = Content { operations: vec![Operation::new("", vec![Object::Name(k.clone()), v.clone()])] };
                out.push(b' ');
                out.extend_from_slice(c.encode().map_err(|e| e.to_string())?.trim_ascii_end());
            }
            out.extend_from_slice(b" ID ");
            out.extend_from_slice(&s.content);
            out.extend_from_slice(b"\nEI\n");
        } else {
            batch.push(op.clone());
        }
    }
    flush(&mut batch, &mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrices() {
        let m = mul(&translate(10.0, 5.0), &[2.0, 0.0, 0.0, 2.0, 1.0, 1.0]);
        assert_eq!(apply_m(&m, 0.0, 0.0), (21.0, 11.0));
        let inv = invert(&m).unwrap();
        let (x, y) = apply_m(&inv, 21.0, 11.0);
        assert!(x.abs() < 1e-4 && y.abs() < 1e-4);
    }

    #[test]
    fn encodage_image_en_ligne() {
        let mut d = Dictionary::new();
        d.set("W", 1);
        d.set("H", 1);
        let ops = vec![Operation::new("q", vec![]), Operation::new("BI", vec![Object::Stream(Stream::new(d, vec![0xFF]))]), Operation::new("Q", vec![])];
        let b = encode(&ops).unwrap();
        let s = String::from_utf8_lossy(&b);
        assert!(s.contains("BI /W 1 /H 1 ID "), "{s}");
        assert!(s.ends_with("EI\nQ\n"), "{s}");
    }
}
