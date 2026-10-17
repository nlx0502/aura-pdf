//! Module Éditer côté moteur : recherche des données à caviarder, édition du
//! contenu en mémoire, enregistrement du plan, calques.

use std::sync::OnceLock;

use regex::Regex;
use serde::Serialize;

use super::*;
use crate::edit::layers::Layer;
use crate::edit::links::LinkInfo;
use crate::engine::content::{self, ContentItem, ContentOp};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactHit {
    pub page: u32,
    pub rects: Vec<Rect>,
    pub text: String,
    /// iban, email, tel ou text.
    pub kind: String,
}

fn re(kind: &str) -> &'static Regex {
    static IBAN: OnceLock<Regex> = OnceLock::new();
    static EMAIL: OnceLock<Regex> = OnceLock::new();
    static TEL: OnceLock<Regex> = OnceLock::new();
    static CARD: OnceLock<Regex> = OnceLock::new();
    match kind {
        "iban" => IBAN.get_or_init(|| Regex::new(r"\b[A-Z]{2}\d{2}(?:[ \u{00A0}]?[A-Z0-9]){11,30}\b").unwrap()),
        "email" => EMAIL.get_or_init(|| Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9\-]+(?:\.[A-Za-z0-9\-]+)*\.[A-Za-z]{2,}").unwrap()),
        "card" => CARD.get_or_init(|| Regex::new(r"\b\d{4}(?:[ \-]?\d{4}){2}[ \-]?\d{1,7}\b").unwrap()),
        _ => TEL.get_or_init(|| {
            // Numéros français (0X XX XX XX XX) et internationaux (+33 (0)1 …, 0044 …).
            Regex::new(r"\b0[1-9](?:[ .\-\u{00A0}]?\d{2}){4}\b|(?:\+|\b00)[1-9]\d{0,2}[ .\-\u{00A0}]?(?:\(0\)[ .\-\u{00A0}]?)?\d(?:[ .\-\u{00A0}]?\d{1,4}){2,6}\b").unwrap()
        }),
    }
}

/// Contrôle de l'IBAN (clé modulo 97).
pub fn iban_ok(s: &str) -> bool {
    let c: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if c.len() < 15 || c.len() > 34 {
        return false;
    }
    let moved = format!("{}{}", &c[4..], &c[..4]);
    let mut rem: u32 = 0;
    for ch in moved.chars() {
        let v = if ch.is_ascii_digit() { ch as u32 - '0' as u32 } else { ch.to_ascii_uppercase() as u32 - 'A' as u32 + 10 };
        rem = if v >= 10 { (rem * 100 + v) % 97 } else { (rem * 10 + v) % 97 };
    }
    rem == 1
}

/// Contrôle de Luhn (numéros de carte).
fn luhn_ok(s: &str) -> bool {
    let d: Vec<u32> = s.chars().filter_map(|c| c.to_digit(10)).collect();
    if d.len() < 13 || d.len() > 19 {
        return false;
    }
    let sum: u32 = d.iter().rev().enumerate().map(|(i, &x)| if i % 2 == 1 { let y = x * 2; if y > 9 { y - 9 } else { y } } else { x }).sum();
    sum % 10 == 0
}

/// Correspondances d'un motif dans un texte : (début, fin) en index de caractères.
pub fn find_matches(kind: &str, text: &str) -> Vec<(usize, usize)> {
    let byte_to_char: Vec<usize> = {
        let mut v = vec![0; text.len() + 1];
        let mut ci = 0;
        for (bi, _) in text.char_indices() {
            v[bi] = ci;
            ci += 1;
        }
        v[text.len()] = ci;
        // Remplir les octets intérieurs des caractères multi-octets.
        let mut last = 0;
        for (i, x) in v.iter_mut().enumerate() {
            if i > 0 && *x == 0 {
                *x = last;
            }
            last = *x;
        }
        v
    };
    // Un IBAN ou un numéro de carte contient des suites de chiffres qui
    // ressemblent à des numéros de téléphone.
    let taken: Vec<(usize, usize)> = if kind == "tel" {
        ["iban", "card"].iter().flat_map(|k| find_matches(k, text)).collect()
    } else {
        Vec::new()
    };
    re(kind)
        .find_iter(text)
        .filter(|m| match kind {
            "iban" => iban_ok(m.as_str()),
            "card" => luhn_ok(m.as_str()),
            "tel" => {
                let n = m.as_str().chars().filter(|c| c.is_ascii_digit()).count();
                (9..=15).contains(&n)
            }
            _ => true,
        })
        .map(|m| (byte_to_char[m.start()], byte_to_char[m.end()]))
        .filter(|&(s, e)| !taken.iter().any(|&(a, b)| s < b && a < e))
        .collect()
}

impl Worker {
    pub(super) fn redact_find(&self, doc: u32, patterns: &[String], terms: &[String]) -> AppResult<Vec<RedactHit>> {
        let d = self.get(doc)?;
        let mut out = Vec::new();
        let folded_terms: Vec<(String, Vec<char>)> = terms.iter().map(|t| t.trim()).filter(|t| !t.is_empty()).map(|t| (t.to_string(), fold_query(t))).collect();
        for index in 0..d.pages().len() as u32 {
            let Ok(page) = d.pages().get(index as PdfPageIndex) else { continue };
            let (chars, rects) = page_chars(&page);
            if chars.is_empty() {
                continue;
            }
            let text: String = chars.iter().map(|&c| if c == '\r' || c == '\n' { ' ' } else { c }).collect();
            let mut push = |s: usize, e: usize, kind: &str| {
                let r = merge_line_rects(rects[s..e].iter().flatten().copied());
                if !r.is_empty() {
                    out.push(RedactHit { page: index, rects: r, text: chars[s..e].iter().collect::<String>().trim().to_string(), kind: kind.to_string() });
                }
            };
            for p in patterns {
                for (s, e) in find_matches(p, &text) {
                    push(s, e, p);
                }
            }
            if !folded_terms.is_empty() {
                let folded: Vec<char> = chars.iter().map(|&c| fold_char(c)).collect();
                for (_, needle) in &folded_terms {
                    for (s, e) in find_all(&folded, needle) {
                        push(s, e, "text");
                    }
                }
            }
        }
        Ok(out)
    }

    pub(super) fn edit_links(&self, doc: u32) -> AppResult<Vec<Vec<LinkInfo>>> {
        let open = self.docs.get(&doc).ok_or_else(AppError::not_found)?;
        let d = crate::edit::load(&open.path, open.password.as_deref())?;
        Ok((0..d.get_pages().len() as u32).map(|i| crate::edit::links::list(&d, i)).collect())
    }

    /// Enregistre le plan : dans le fichier lui-même (`out` absent, le document
    /// est alors rouvert sous un nouvel identifiant) ou dans un autre fichier.
    pub(super) fn edit_apply(&mut self, doc: u32, plan: &crate::edit::Plan, out: Option<PathBuf>) -> AppResult<Option<DocInfo>> {
        let (path, password, dirty) = {
            let open = self.docs.get(&doc).ok_or_else(AppError::not_found)?;
            (open.path.clone(), open.password.clone(), open.content_dirty)
        };
        // Document chiffré : droits vérifiés, chiffrement d'origine conservé.
        let maybe_encrypted = std::fs::read(&path).map(|b| b.windows(8).any(|w| w == b"/Encrypt")).unwrap_or(false);
        let keep = if !maybe_encrypted {
            None
        } else {
            let orig = crate::edit::load(&path, password.as_deref())?;
            match orig.encryption_state {
                Some(state) => {
                    let perms = crate::security::inspect(&path, password.as_deref()).map_err(|e| AppError::new("edit", e))?.effective();
                    if !perms.edit && !perms.extract {
                        return Err(AppError::new("edit", "Le propriétaire du document n’autorise pas sa modification."));
                    }
                    Some(state)
                }
                None => None,
            }
        };
        // Modifications de contenu faites dans PDFium : écrites d'abord dans un fichier intermédiaire.
        let mut tmp: Option<PathBuf> = None;
        if dirty {
            let open = self.docs.get_mut(&doc).ok_or_else(AppError::not_found)?;
            // Les annotations masquées pendant l'édition ne doivent pas l'être dans le fichier.
            for (page, set) in open.hidden.drain() {
                if let Ok(mut p) = open.doc.pages().get(page as PdfPageIndex) {
                    for i in set {
                        if let Ok(mut a) = p.annotations_mut().get(i as PdfPageAnnotationIndex) {
                            let _ = a.set_is_hidden(false);
                        }
                    }
                }
            }
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
            let t = std::env::temp_dir().join(format!("aura-edit-{doc}-{nanos}.pdf"));
            open.doc.save_to_file(&t).map_err(|e| AppError::new("save", format!("Impossible d’enregistrer le contenu modifié ({e:?}).")))?;
            tmp = Some(t);
        }
        let src = tmp.clone().unwrap_or_else(|| path.clone());
        let dest = out.clone().unwrap_or_else(|| path.clone());
        let result = crate::edit::apply(&src, password.as_deref(), plan, &dest, keep.as_ref());
        if let Some(t) = tmp {
            let _ = std::fs::remove_file(t);
        }
        result?;
        if out.is_some() {
            return Ok(None);
        }
        // Le fichier a changé de structure : nouveau document, nouvel identifiant.
        if let Some(old) = self.docs.remove(&doc) {
            if let Some(r) = old.render_path {
                let _ = std::fs::remove_file(r);
            }
        }
        self.gens.write().remove(&doc);
        if self.search.as_ref().map(|s| s.doc == doc).unwrap_or(false) {
            self.search = None;
        }
        self.open(&path, password.as_deref()).map(Some)
    }

    pub(super) fn layers(&self, doc: u32) -> AppResult<Vec<Layer>> {
        let open = self.docs.get(&doc).ok_or_else(AppError::not_found)?;
        let d = crate::edit::load(&open.path, open.password.as_deref())?;
        Ok(crate::edit::layers::list(&d))
    }

    pub(super) fn layers_set(&mut self, doc: u32, hidden: &[String]) -> AppResult<()> {
        let (path, password, dirty, prev_render) = {
            let open = self.docs.get(&doc).ok_or_else(AppError::not_found)?;
            (open.path.clone(), open.password.clone(), open.content_dirty, open.render_path.clone())
        };
        if dirty {
            return Err(AppError::new("layers", "Enregistrez d’abord les modifications du contenu."));
        }
        let bytes = std::fs::read(&path)?;
        let out = crate::edit::layers::with_visibility(bytes, password.as_deref(), hidden).map_err(|e| AppError::new("layers", e))?;
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("aura-layers-{doc}-{nanos}.pdf"));
        std::fs::write(&tmp, out)?;
        let pdf = self.pdfium.load_pdf_from_file(&tmp, password.as_deref()).map_err(|e| load_error(e, password.is_some()))?;
        if let Some(open) = self.docs.get_mut(&doc) {
            open.doc = pdf;
            open.hidden.clear();
            open.render_path = Some(tmp);
            open.hidden_layers = Some(hidden.to_vec());
        }
        if let Some(p) = prev_render {
            let _ = std::fs::remove_file(p);
        }
        Ok(())
    }

    pub(super) fn content_list(&self, doc: u32, page: u32) -> AppResult<Vec<ContentItem>> {
        let d = self.get(doc)?;
        let p = d.pages().get(page as PdfPageIndex).map_err(AppError::internal)?;
        Ok(content::list(&p))
    }

    /// Applique des opérations de contenu. `reset` : repartir du fichier
    /// (annuler / rétablir rejoue toute la liste).
    pub(super) fn content_apply(&mut self, doc: u32, ops: &[ContentOp], reset: bool) -> AppResult<()> {
        if reset {
            let (src, password) = {
                let open = self.docs.get(&doc).ok_or_else(AppError::not_found)?;
                (open.render_path.clone().unwrap_or_else(|| open.path.clone()), open.password.clone())
            };
            let pdf = self.pdfium.load_pdf_from_file(&src, password.as_deref()).map_err(|e| load_error(e, password.is_some()))?;
            if let Some(open) = self.docs.get_mut(&doc) {
                open.doc = pdf;
                open.hidden.clear();
                open.content_dirty = false;
            }
        }
        let open = self.docs.get_mut(&doc).ok_or_else(AppError::not_found)?;
        for op in ops {
            content::apply(&mut open.doc, op).map_err(|e| AppError::new("content", format!("Modification impossible ({e}).")))?;
            open.content_dirty = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motifs() {
        let t = "IBAN : FR76 3000 6000 0112 3456 7890 189 — contact : jeanne.martin@exemple.fr, tél. 06 12 34 56 78 ou +33 (0)1 42 68 53 00.";
        assert_eq!(find_matches("iban", t).len(), 1);
        assert_eq!(find_matches("email", t).len(), 1);
        assert_eq!(find_matches("tel", t).len(), 2, "{:?}", find_matches("tel", t));
        // Clé IBAN fausse : ignorée.
        assert!(find_matches("iban", "FR76 3000 6000 0112 3456 7890 180").is_empty());
        let (s, e) = find_matches("email", t)[0];
        assert_eq!(t.chars().skip(s).take(e - s).collect::<String>(), "jeanne.martin@exemple.fr");
        assert_eq!(find_matches("card", "Carte 4111 1111 1111 1111").len(), 1);
    }
}
