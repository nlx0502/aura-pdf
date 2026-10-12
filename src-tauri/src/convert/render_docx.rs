//! Conversion native d'un document Word (.docx) en PDF, sans Office :
//! paragraphes (styles de titre, alignement, retraits, espacements, listes),
//! mise en forme des caractères (police, taille, gras, italique, souligné,
//! couleur), sauts de ligne et de page, tableaux, images. Les polices
//! installées sont intégrées en sous-ensembles.
//!
//! Les zones de texte, colonnes, en-têtes et pieds de page, notes et champs
//! ne sont pas rendus (Office, lorsqu'il est installé, est utilisé à la place).

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::Path;

use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use roxmltree::Node;

use crate::sysfonts::{self, Loaded};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";

#[derive(Clone, Debug, PartialEq)]
struct Style {
    font: String,
    size: f32,
    bold: bool,
    italic: bool,
    underline: bool,
    color: [f32; 3],
}

impl Default for Style {
    fn default() -> Self {
        Style { font: "Calibri".into(), size: 11.0, bold: false, italic: false, underline: false, color: [0.0; 3] }
    }
}

enum Piece {
    Text(String, Style),
    Tab(Style),
    Break,
    PageBreak,
    Image(String, f32, f32),
}

struct Para {
    pieces: Vec<Piece>,
    align: u8,
    before: f32,
    after: f32,
    line: f32,
    left: f32,
    first: f32,
    bullet: Option<String>,
}

enum Block {
    Para(Para),
    Table(Vec<f32>, Vec<Vec<Vec<Para>>>),
}

fn attr<'a>(n: Node<'a, 'a>, name: &str) -> Option<&'a str> {
    n.attribute((W, name))
}

fn child<'a>(n: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    n.children().find(|c| c.has_tag_name((W, name)))
}

fn on(n: Option<Node>) -> Option<bool> {
    n.map(|n| !matches!(attr(n, "val"), Some("0") | Some("false") | Some("none")))
}

fn hex(s: &str) -> Option<[f32; 3]> {
    if s.len() != 6 {
        return None;
    }
    let p = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok().map(|v| v as f32 / 255.0);
    Some([p(0)?, p(2)?, p(4)?])
}

fn apply_rpr(st: &mut Style, rpr: Option<Node>) {
    let Some(rpr) = rpr else { return };
    if let Some(f) = child(rpr, "rFonts").and_then(|f| attr(f, "ascii").or_else(|| attr(f, "hAnsi"))) {
        st.font = f.to_string();
    }
    if let Some(sz) = child(rpr, "sz").and_then(|s| attr(s, "val")).and_then(|v| v.parse::<f32>().ok()) {
        st.size = sz / 2.0;
    }
    if let Some(b) = on(child(rpr, "b")) {
        st.bold = b;
    }
    if let Some(i) = on(child(rpr, "i")) {
        st.italic = i;
    }
    if let Some(u) = child(rpr, "u") {
        st.underline = attr(u, "val") != Some("none");
    }
    if let Some(c) = child(rpr, "color").and_then(|c| attr(c, "val")).and_then(hex) {
        st.color = c;
    }
}

struct Ctx {
    styles: HashMap<String, (Option<String>, Style, f32, f32)>,
    rels: HashMap<String, String>,
    base: Style,
}

fn read_zip(path: &Path) -> Result<HashMap<String, Vec<u8>>, String> {
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|_| "fichier Word illisible".to_string())?;
    let mut out = HashMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).map_err(|e| e.to_string())?;
        let name = e.name().to_string();
        if name.ends_with(".xml") || name.ends_with(".rels") || name.starts_with("word/media/") {
            let mut b = Vec::new();
            e.read_to_end(&mut b).map_err(|e| e.to_string())?;
            out.insert(name, b);
        }
    }
    Ok(out)
}

/// Styles de paragraphe : parent, mise en forme, espacements avant / après.
fn read_styles(xml: &str, base: &mut Style) -> HashMap<String, (Option<String>, Style, f32, f32)> {
    let mut out = HashMap::new();
    let Ok(doc) = roxmltree::Document::parse(xml) else { return out };
    if let Some(def) = doc.descendants().find(|n| n.has_tag_name((W, "rPrDefault"))) {
        apply_rpr(base, child(def, "rPr"));
    }
    for s in doc.descendants().filter(|n| n.has_tag_name((W, "style"))) {
        let Some(id) = attr(s, "styleId") else { continue };
        let parent = child(s, "basedOn").and_then(|b| attr(b, "val")).map(String::from);
        let mut st = Style { font: String::new(), size: 0.0, ..Style::default() };
        st.bold = false;
        apply_rpr(&mut st, child(s, "rPr"));
        let sp = child(s, "pPr").and_then(|p| child(p, "spacing"));
        let tw = |k: &str| sp.and_then(|s| attr(s, k)).and_then(|v| v.parse::<f32>().ok()).map(|v| v / 20.0).unwrap_or(-1.0);
        out.insert(id.to_string(), (parent, st, tw("before"), tw("after")));
    }
    out
}

impl Ctx {
    /// Style de caractère effectif d'un style de paragraphe (héritage compris).
    fn para_style(&self, id: Option<&str>) -> (Style, f32, f32) {
        let mut chain = Vec::new();
        let mut cur = id.map(String::from);
        while let Some(c) = cur {
            if chain.len() > 10 {
                break;
            }
            let Some(s) = self.styles.get(&c) else { break };
            chain.push(s);
            cur = s.0.clone();
        }
        let mut st = self.base.clone();
        let (mut before, mut after) = (0.0, 8.0);
        for s in chain.iter().rev() {
            if !s.1.font.is_empty() {
                st.font = s.1.font.clone();
            }
            if s.1.size > 0.0 {
                st.size = s.1.size;
            }
            st.bold |= s.1.bold;
            st.italic |= s.1.italic;
            if s.1.color != [0.0; 3] {
                st.color = s.1.color;
            }
            if s.2 >= 0.0 {
                before = s.2;
            }
            if s.3 >= 0.0 {
                after = s.3;
            }
        }
        // Titres sans style défini dans le fichier : tailles usuelles.
        if let Some(id) = id {
            let l = id.to_ascii_lowercase();
            if !self.styles.contains_key(id) || chain.iter().all(|s| s.1.size == 0.0) {
                let size = if l == "title" || l == "titre" {
                    Some(26.0)
                } else if l.starts_with("heading") || l.starts_with("titre") {
                    l.chars().last().and_then(|c| c.to_digit(10)).map(|n| match n {
                        1 => 16.0,
                        2 => 13.0,
                        _ => 12.0,
                    })
                } else {
                    None
                };
                if let Some(s) = size {
                    st.size = s;
                    st.bold = true;
                    before = before.max(12.0);
                }
            }
        }
        (st, before, after)
    }

    fn para(&self, p: Node) -> Para {
        let ppr = child(p, "pPr");
        let style_id = ppr.and_then(|x| child(x, "pStyle")).and_then(|s| attr(s, "val"));
        let (base, mut before, mut after) = self.para_style(style_id);
        let mut align = 0;
        let (mut left, mut first, mut line) = (0.0, 0.0, 1.0);
        let mut bullet = None;
        if let Some(ppr) = ppr {
            align = match child(ppr, "jc").and_then(|j| attr(j, "val")) {
                Some("center") => 1,
                Some("right") | Some("end") => 2,
                Some("both") | Some("distribute") => 3,
                _ => 0,
            };
            if let Some(sp) = child(ppr, "spacing") {
                if let Some(v) = attr(sp, "before").and_then(|v| v.parse::<f32>().ok()) {
                    before = v / 20.0;
                }
                if let Some(v) = attr(sp, "after").and_then(|v| v.parse::<f32>().ok()) {
                    after = v / 20.0;
                }
                if let (Some(v), rule) = (attr(sp, "line").and_then(|v| v.parse::<f32>().ok()), attr(sp, "lineRule")) {
                    if rule.unwrap_or("auto") == "auto" {
                        line = v / 240.0;
                    }
                }
            }
            if let Some(ind) = child(ppr, "ind") {
                let tw = |k: &str| attr(ind, k).and_then(|v| v.parse::<f32>().ok()).map(|v| v / 20.0);
                left = tw("left").or_else(|| tw("start")).unwrap_or(0.0);
                first = tw("firstLine").unwrap_or(0.0) - tw("hanging").unwrap_or(0.0);
            }
            if let Some(num) = child(ppr, "numPr") {
                let lvl = child(num, "ilvl").and_then(|l| attr(l, "val")).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
                bullet = Some(["•", "◦", "▪"][lvl % 3].to_string());
                if left == 0.0 {
                    left = 18.0 * (lvl as f32 + 1.0);
                }
            }
        }
        let mut pieces = Vec::new();
        for r in p.descendants().filter(|n| n.has_tag_name((W, "r"))) {
            let mut st = base.clone();
            apply_rpr(&mut st, child(r, "rPr"));
            for c in r.children().filter(|c| c.is_element()) {
                match c.tag_name().name() {
                    "t" => pieces.push(Piece::Text(c.text().unwrap_or("").to_string(), st.clone())),
                    "tab" => pieces.push(Piece::Tab(st.clone())),
                    "br" => pieces.push(if attr(c, "type") == Some("page") { Piece::PageBreak } else { Piece::Break }),
                    "cr" => pieces.push(Piece::Break),
                    "drawing" => {
                        let blip = c.descendants().find(|n| n.has_tag_name((A, "blip")));
                        let ext = c.descendants().find(|n| n.has_tag_name((WP, "extent")));
                        if let (Some(b), Some(e)) = (blip, ext) {
                            let id = b.attribute((R, "embed")).unwrap_or("");
                            let emu = |k: &str| e.attribute(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 12700.0;
                            if let Some(target) = self.rels.get(id) {
                                pieces.push(Piece::Image(target.clone(), emu("cx"), emu("cy")));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Para { pieces, align, before, after, line, left, first, bullet }
    }

    fn blocks(&self, body: Node) -> Vec<Block> {
        let mut out = Vec::new();
        for n in body.children().filter(|c| c.is_element()) {
            match n.tag_name().name() {
                "p" => out.push(Block::Para(self.para(n))),
                "tbl" => {
                    let grid: Vec<f32> = child(n, "tblGrid")
                        .map(|g| g.children().filter(|c| c.has_tag_name((W, "gridCol"))).filter_map(|c| attr(c, "w")).filter_map(|v| v.parse::<f32>().ok()).map(|v| v / 20.0).collect())
                        .unwrap_or_default();
                    let rows = n
                        .children()
                        .filter(|c| c.has_tag_name((W, "tr")))
                        .map(|tr| tr.children().filter(|c| c.has_tag_name((W, "tc"))).map(|tc| tc.children().filter(|c| c.has_tag_name((W, "p"))).map(|p| self.para(p)).collect()).collect())
                        .collect();
                    out.push(Block::Table(grid, rows));
                }
                _ => {}
            }
        }
        out
    }
}

// ---------------------------------------------------------------- mise en page

/// Polices chargées et caractères utilisés (pour les sous-ensembles).
struct Fonts {
    loaded: Vec<(String, bool, bool, Loaded)>,
    used: Vec<String>,
}

impl Fonts {
    fn key(&mut self, st: &Style) -> usize {
        if let Some(i) = self.loaded.iter().position(|(f, b, it, _)| *f == st.font && *b == st.bold && *it == st.italic) {
            return i;
        }
        let font = sysfonts::load(&st.font, st.bold, st.italic)
            .or_else(|| sysfonts::load("Calibri", st.bold, st.italic))
            .or_else(|| sysfonts::load("Arial", st.bold, st.italic))
            .expect("police système");
        self.loaded.push((st.font.clone(), st.bold, st.italic, font));
        self.used.push(String::new());
        self.loaded.len() - 1
    }
    fn width(&mut self, st: &Style, s: &str) -> f32 {
        let k = self.key(st);
        self.loaded[k].3.text_width(s, st.size)
    }
}

enum Op {
    Text { x: f32, y: f32, font: usize, size: f32, color: [f32; 3], text: String, underline: f32 },
    Image { x: f32, y: f32, w: f32, h: f32, path: String },
    Rect { x: f32, y: f32, w: f32, h: f32 },
}

struct Layout {
    page_w: f32,
    page_h: f32,
    margin: [f32; 4], // haut, droite, bas, gauche
    pages: Vec<Vec<Op>>,
    y: f32,
}

impl Layout {
    fn new_page(&mut self) {
        self.pages.push(Vec::new());
        self.y = self.margin[0];
    }
    fn bottom(&self) -> f32 {
        self.page_h - self.margin[2]
    }
    fn push(&mut self, op: Op) {
        self.pages.last_mut().unwrap().push(op);
    }
}

/// Mot ou élément insécable d'une ligne.
struct Item {
    text: String,
    st: Style,
    w: f32,
    space_after: bool,
    image: Option<(String, f32, f32)>,
    tab: bool,
}

/// Dispose un paragraphe dans [x0, x0 + width] ; `draw` faux : mesure seulement.
fn layout_para(p: &Para, x0: f32, width: f32, fonts: &mut Fonts, l: &mut Layout, draw: bool) -> f32 {
    let mut items: Vec<Item> = Vec::new();
    let mut breaks: Vec<usize> = Vec::new();
    let default = p.pieces.iter().find_map(|x| match x {
        Piece::Text(_, s) | Piece::Tab(s) => Some(s.clone()),
        _ => None,
    });
    for piece in &p.pieces {
        match piece {
            Piece::Text(t, st) => {
                let mut first = true;
                for word in t.split(' ') {
                    if !first {
                        if let Some(last) = items.last_mut() {
                            last.space_after = true;
                        }
                    }
                    first = false;
                    if word.is_empty() {
                        continue;
                    }
                    let w = fonts.width(st, word);
                    // Continuité d'un mot sur deux segments (changement de style).
                    items.push(Item { text: word.to_string(), st: st.clone(), w, space_after: false, image: None, tab: false });
                }
            }
            Piece::Tab(st) => items.push(Item { text: String::new(), st: st.clone(), w: 36.0, space_after: false, image: None, tab: true }),
            Piece::Break => breaks.push(items.len()),
            Piece::PageBreak => {
                breaks.push(items.len());
                if draw {
                    // Saut de page : traité à la fin du paragraphe (approximation).
                }
            }
            Piece::Image(path, w, h) => items.push(Item { text: String::new(), st: default.clone().unwrap_or_default(), w: w.min(width), space_after: true, image: Some((path.clone(), w.min(width), h * (w.min(width) / w.max(0.1)))), tab: false }),
        }
    }
    let base = default.unwrap_or_default();
    let mut height = p.before;
    let mut y = if draw { l.y + p.before } else { 0.0 };
    // Lignes.
    let mut i = 0;
    let mut line_no = 0;
    if items.is_empty() {
        let h = base.size * 1.17 * p.line;
        height += h + p.after;
        if draw {
            l.y = y + h + p.after;
        }
        return height;
    }
    while i < items.len() {
        let indent = p.left + if line_no == 0 { p.first } else { 0.0 };
        let avail = (width - indent).max(20.0);
        let mut j = i;
        let mut w = 0.0;
        while j < items.len() {
            let sp = if j > i && items[j - 1].space_after { fonts.width(&items[j - 1].st, " ") } else { 0.0 };
            if j > i && (w + sp + items[j].w > avail || breaks.contains(&j)) {
                break;
            }
            w += sp + items[j].w;
            j += 1;
        }
        let line_items = &items[i..j];
        let asc = line_items.iter().map(|it| it.image.as_ref().map(|im| im.2).unwrap_or(it.st.size * 0.95)).fold(base.size * 0.95, f32::max);
        let lh = line_items.iter().map(|it| it.image.as_ref().map(|im| im.2 + 2.0).unwrap_or(it.st.size * 1.17 * p.line)).fold(base.size * 1.17 * p.line, f32::max);
        if draw && y + lh > l.bottom() && y > l.margin[0] + 1.0 {
            l.new_page();
            y = l.y;
        }
        if draw {
            let mut x = x0 + indent + match p.align {
                1 => (avail - w) / 2.0,
                2 => avail - w,
                _ => 0.0,
            };
            if line_no == 0 {
                if let Some(b) = &p.bullet {
                    let k = fonts.key(&base);
                    fonts.used[k].push_str(b);
                    l.push(Op::Text { x: x - 12.0, y: y + asc, font: k, size: base.size, color: base.color, text: b.clone(), underline: 0.0 });
                }
            }
            for (n, it) in line_items.iter().enumerate() {
                if let Some((path, iw, ih)) = &it.image {
                    l.push(Op::Image { x, y: y + asc - ih, w: *iw, h: *ih, path: path.clone() });
                } else if !it.tab {
                    let k = fonts.key(&it.st);
                    fonts.used[k].push_str(&it.text);
                    let uw = if it.st.underline { it.w } else { 0.0 };
                    l.push(Op::Text { x, y: y + asc, font: k, size: it.st.size, color: it.st.color, text: it.text.clone(), underline: uw });
                }
                x += it.w;
                if n + 1 < line_items.len() && it.space_after {
                    x += fonts.width(&it.st, " ");
                }
            }
        }
        y += lh;
        height += lh;
        i = j;
        line_no += 1;
    }
    height += p.after;
    if draw {
        l.y = y + p.after;
        if p.pieces.iter().any(|x| matches!(x, Piece::PageBreak)) {
            l.new_page();
        }
    }
    height
}

fn layout(blocks: &[Block], fonts: &mut Fonts, l: &mut Layout) {
    let width = l.page_w - l.margin[1] - l.margin[3];
    let x0 = l.margin[3];
    for b in blocks {
        match b {
            Block::Para(p) => {
                layout_para(p, x0, width, fonts, l, true);
            }
            Block::Table(grid, rows) => {
                let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
                let total: f32 = grid.iter().sum();
                let cols: Vec<f32> = if grid.len() >= ncols && total > 0.0 {
                    let scale = (width / total).min(1.0);
                    grid.iter().take(ncols).map(|w| w * scale).collect()
                } else {
                    vec![width / ncols as f32; ncols]
                };
                let pad = 4.0;
                for row in rows {
                    let h = row
                        .iter()
                        .enumerate()
                        .map(|(c, cell)| cell.iter().map(|p| layout_para(p, 0.0, cols.get(c).copied().unwrap_or(40.0) - 2.0 * pad, fonts, l, false)).sum::<f32>())
                        .fold(12.0, f32::max)
                        + 2.0 * pad;
                    if l.y + h > l.bottom() && l.y > l.margin[0] + 1.0 {
                        l.new_page();
                    }
                    let top = l.y;
                    let mut x = x0;
                    for (c, cell) in row.iter().enumerate() {
                        let cw = cols.get(c).copied().unwrap_or(40.0);
                        l.push(Op::Rect { x, y: top, w: cw, h });
                        l.y = top + pad;
                        for p in cell {
                            layout_para(p, x + pad, cw - 2.0 * pad, fonts, l, true);
                        }
                        x += cw;
                    }
                    l.y = top + h;
                }
                l.y += 6.0;
            }
        }
    }
}

// ---------------------------------------------------------------- écriture

fn image_xobject(doc: &mut Document, bytes: &[u8]) -> Option<ObjectId> {
    let img = image::load_from_memory(bytes).ok()?;
    let (w, h) = (img.width() as i64, img.height() as i64);
    if bytes.starts_with(&[0xFF, 0xD8]) {
        let cs = if img.color().channel_count() == 1 { "DeviceGray" } else { "DeviceRGB" };
        return Some(doc.add_object(Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => w, "Height" => h, "ColorSpace" => cs, "BitsPerComponent" => 8, "Filter" => "DCTDecode" }, bytes.to_vec())));
    }
    let rgba = img.to_rgba8();
    let rgb: Vec<u8> = rgba.pixels().flat_map(|p| [p[0], p[1], p[2]]).collect();
    let alpha: Vec<u8> = rgba.pixels().map(|p| p[3]).collect();
    let mut d = dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => w, "Height" => h, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8 };
    if alpha.iter().any(|a| *a < 255) {
        let mut sm = Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => w, "Height" => h, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 }, alpha);
        let _ = sm.compress();
        d.set("SMask", doc.add_object(sm));
    }
    let mut s = Stream::new(d, rgb);
    let _ = s.compress();
    Some(doc.add_object(s))
}

pub fn to_pdf(input: &Path, out: &Path) -> Result<(), String> {
    let files = read_zip(input)?;
    let text = |n: &str| files.get(n).map(|b| String::from_utf8_lossy(b).into_owned());
    let mut base = Style::default();
    let styles = text("word/styles.xml").map(|x| read_styles(&x, &mut base)).unwrap_or_default();
    let mut rels = HashMap::new();
    if let Some(x) = text("word/_rels/document.xml.rels") {
        if let Ok(d) = roxmltree::Document::parse(&x) {
            for r in d.descendants().filter(|n| n.tag_name().name() == "Relationship") {
                if let (Some(id), Some(t)) = (r.attribute("Id"), r.attribute("Target")) {
                    rels.insert(id.to_string(), format!("word/{}", t.trim_start_matches('/').trim_start_matches("word/")));
                }
            }
        }
    }
    let xml = text("word/document.xml").ok_or("document Word vide")?;
    let dom = roxmltree::Document::parse(&xml).map_err(|_| "document Word illisible".to_string())?;
    let body = dom.descendants().find(|n| n.has_tag_name((W, "body"))).ok_or("document Word vide")?;
    let ctx = Ctx { styles, rels, base };
    let blocks = ctx.blocks(body);

    // Format de page et marges (dernière section).
    let sect = body.children().find(|c| c.has_tag_name((W, "sectPr")));
    let tw = |n: Option<Node>, k: &str, d: f32| n.and_then(|n| attr(n, k)).and_then(|v| v.parse::<f32>().ok()).map(|v| v / 20.0).unwrap_or(d);
    let pg = sect.and_then(|s| child(s, "pgSz"));
    let mar = sect.and_then(|s| child(s, "pgMar"));
    let title = input.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    write_pdf(
        &blocks,
        &files,
        [tw(pg, "w", 595.28), tw(pg, "h", 841.89)],
        [tw(mar, "top", 72.0), tw(mar, "right", 72.0), tw(mar, "bottom", 72.0), tw(mar, "left", 72.0)],
        &title,
        out,
    )
}

/// Met en page les blocs et écrit le PDF.
fn write_pdf(blocks: &[Block], files: &HashMap<String, Vec<u8>>, size: [f32; 2], margin: [f32; 4], title: &str, out: &Path) -> Result<(), String> {
    let mut l = Layout { page_w: size[0], page_h: size[1], margin, pages: Vec::new(), y: 0.0 };
    l.new_page();
    let mut fonts = Fonts { loaded: Vec::new(), used: Vec::new() };
    layout(blocks, &mut fonts, &mut l);

    // Document PDF.
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font_ids: Vec<ObjectId> = fonts.loaded.iter().zip(&fonts.used).map(|((_, _, _, f), used)| sysfonts::embed(&mut doc, f, if used.is_empty() { " " } else { used })).collect();
    let mut images: BTreeMap<String, ObjectId> = BTreeMap::new();
    let mut kids = Vec::new();
    for ops in &l.pages {
        let mut xo = lopdf::Dictionary::new();
        let mut content: Vec<u8> = Vec::new();
        for op in ops {
            match op {
                Op::Text { x, y, font, size, color, text, underline } => {
                    let by = l.page_h - y;
                    content.extend(format!("BT /F{font} {size:.2} Tf {:.3} {:.3} {:.3} rg 1 0 0 1 {x:.2} {by:.2} Tm ", color[0], color[1], color[2]).bytes());
                    if let Object::String(b, _) = sysfonts::encode(&fonts.loaded[*font].3, text) {
                        content.push(b'<');
                        content.extend(b.iter().flat_map(|v| format!("{v:02X}").into_bytes()));
                        content.push(b'>');
                    }
                    content.extend(b" Tj ET\n");
                    if *underline > 0.0 {
                        content.extend(format!("{:.3} {:.3} {:.3} RG {:.2} w {x:.2} {:.2} m {:.2} {:.2} l S\n", color[0], color[1], color[2], size * 0.06, by - size * 0.12, x + underline, by - size * 0.12).bytes());
                    }
                }
                Op::Image { x, y, w, h, path } => {
                    let id = match images.get(path) {
                        Some(id) => Some(*id),
                        None => files.get(path).and_then(|b| image_xobject(&mut doc, b)).inspect(|id| {
                            images.insert(path.clone(), *id);
                        }),
                    };
                    if let Some(id) = id {
                        let name = format!("Im{}", id.0);
                        xo.set(name.as_bytes().to_vec(), id);
                        content.extend(format!("q {w:.2} 0 0 {h:.2} {x:.2} {:.2} cm /{name} Do Q\n", l.page_h - y - h).bytes());
                    }
                }
                Op::Rect { x, y, w, h } => {
                    content.extend(format!("0.75 0.75 0.75 RG 0.5 w {x:.2} {:.2} {w:.2} {h:.2} re S\n", l.page_h - y - h).bytes());
                }
            }
        }
        let mut fd = lopdf::Dictionary::new();
        for (i, id) in font_ids.iter().enumerate() {
            fd.set(format!("F{i}").into_bytes(), *id);
        }
        let mut stream = Stream::new(dictionary! {}, content);
        let _ = stream.compress();
        let cid = doc.add_object(stream);
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), Object::Real(l.page_w), Object::Real(l.page_h)],
            "Contents" => cid,
            "Resources" => dictionary! { "Font" => fd, "XObject" => xo },
        });
        kids.push(Object::Reference(page));
    }
    let n = kids.len() as i64;
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => n }));
    let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", cat);
    let info = doc.add_object(dictionary! { "Title" => crate::annot::pdf::text_object(title), "Producer" => Object::string_literal("Aura PDF") });
    doc.trailer.set("Info", info);
    doc.compress();
    doc.save(out).map_err(|e| format!("écriture impossible ({e})"))?;
    Ok(())
}

/// PDF à partir d'un texte brut (presse-papiers) : un paragraphe par ligne.
pub fn text_to_pdf(text: &str, out: &Path) -> Result<(), String> {
    let st = Style { font: "Arial".into(), size: 11.0, ..Style::default() };
    let blocks: Vec<Block> = text
        .replace("\r\n", "\n")
        .split('\n')
        .map(|l| Block::Para(Para { pieces: vec![Piece::Text(l.replace('\t', "    "), st.clone())], align: 0, before: 0.0, after: 4.0, line: 1.0, left: 0.0, first: 0.0, bullet: None }))
        .collect();
    write_pdf(&blocks, &HashMap::new(), [595.28, 841.89], [56.0, 56.0, 56.0, 56.0], "Presse-papiers", out)
}
