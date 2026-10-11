//! Recherche plein texte indépendante de PDFium (fonctions pures, testées).
//!
//! La recherche ignore la casse et les accents (« ecole » trouve « École »),
//! ce qui correspond aux attentes d'un lecteur francophone.

/// Ramène un caractère à sa forme de base, en minuscule et sans accent.
pub fn fold_char(c: char) -> char {
    let c = c.to_lowercase().next().unwrap_or(c);
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ī' => 'i',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' => 'u',
        'ý' | 'ÿ' => 'y',
        'ž' | 'ź' | 'ż' => 'z',
        'š' | 'ś' => 's',
        'ř' => 'r',
        'ł' => 'l',
        // Apostrophes et guillemets typographiques : équivalents ASCII.
        '\u{2019}' | '\u{2018}' | '\u{02BC}' => '\'',
        '\u{201C}' | '\u{201D}' | '«' | '»' => '"',
        // Espaces insécables et fines : espace simple.
        '\u{00A0}' | '\u{202F}' | '\u{2009}' => ' ',
        // Tirets typographiques : trait d'union.
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' => '-',
        other => other,
    }
}

/// Normalise une chaîne de recherche : repli des caractères et espaces fusionnés.
pub fn fold_query(q: &str) -> Vec<char> {
    let mut out = Vec::with_capacity(q.len());
    let mut last_space = false;
    for c in q.trim().chars() {
        let f = fold_char(c);
        let is_space = f.is_whitespace();
        if is_space {
            if !last_space {
                out.push(' ');
            }
        } else {
            out.push(f);
        }
        last_space = is_space;
    }
    out
}

/// Recherche toutes les occurrences (non chevauchantes) de `needle` dans `hay`.
///
/// `hay` doit déjà être replié avec [`fold_char`]. Les blancs (espaces, retours à
/// la ligne) de `hay` correspondent à n'importe quel espace de la requête, ce qui
/// permet de trouver une expression coupée en fin de ligne.
/// Renvoie les intervalles `[début, fin)` en indices de caractères.
pub fn find_all(hay: &[char], needle: &[char]) -> Vec<(usize, usize)> {
    let mut hits = Vec::new();
    if needle.is_empty() || hay.is_empty() {
        return hits;
    }
    let mut i = 0;
    while i < hay.len() {
        if let Some(end) = match_at(hay, i, needle) {
            hits.push((i, end));
            i = end.max(i + 1);
        } else {
            i += 1;
        }
    }
    hits
}

fn match_at(hay: &[char], start: usize, needle: &[char]) -> Option<usize> {
    let mut h = start;
    for &n in needle {
        if n == ' ' {
            // Au moins un blanc dans le texte (espace, saut de ligne, etc.).
            if h >= hay.len() || !hay[h].is_whitespace() {
                return None;
            }
            while h < hay.len() && hay[h].is_whitespace() {
                h += 1;
            }
        } else {
            if h >= hay.len() || hay[h] != n {
                return None;
            }
            h += 1;
        }
    }
    Some(h)
}

/// Construit un extrait autour d'une occurrence : (avant, occurrence, après).
/// Les blancs successifs sont fusionnés et l'extrait est borné à des mots entiers.
pub fn snippet(original: &[char], start: usize, end: usize, ctx: usize) -> (String, String, String) {
    let clean = |s: &[char]| -> String {
        let mut out = String::new();
        let mut last_space = false;
        for &c in s {
            if c.is_whitespace() || c == '\u{0}' {
                if !last_space {
                    out.push(' ');
                }
                last_space = true;
            } else {
                out.push(c);
                last_space = false;
            }
        }
        out
    };

    let mut a = start.saturating_sub(ctx);
    // Commencer sur un début de mot.
    if a > 0 {
        while a < start && !original[a - 1].is_whitespace() {
            a += 1;
        }
    }
    let mut b = (end + ctx).min(original.len());
    if b < original.len() {
        while b > end && !original[b].is_whitespace() {
            b -= 1;
        }
    }

    let mut before = clean(&original[a..start]).trim_start().to_string();
    if a > 0 {
        before.insert(0, '…');
    }
    let matched = clean(&original[start..end]);
    let mut after = clean(&original[end..b]).trim_end().to_string();
    if b < original.len() {
        after.push('…');
    }
    (before, matched, after)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fold(s: &str) -> Vec<char> {
        s.chars().map(fold_char).collect()
    }

    #[test]
    fn ignore_case_et_accents() {
        let hay = fold("L’École est à Paris. ÉCOLE !");
        let hits = find_all(&hay, &fold_query("ecole"));
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn espace_correspond_a_un_saut_de_ligne() {
        let hay = fold("annexes\r\ntechniques");
        let hits = find_all(&hay, &fold_query("annexes techniques"));
        assert_eq!(hits, vec![(0, 19)]);
    }

    #[test]
    fn apostrophe_typographique() {
        let hay = fold("Point d’attention");
        assert_eq!(find_all(&hay, &fold_query("d'attention")).len(), 1);
    }

    #[test]
    fn requete_vide() {
        assert!(find_all(&fold("abc"), &fold_query("   ")).is_empty());
    }

    #[test]
    fn occurrences_non_chevauchantes() {
        let hay = fold("aaaa");
        assert_eq!(find_all(&hay, &fold_query("aa")), vec![(0, 2), (2, 4)]);
    }

    #[test]
    fn extrait_borne_aux_mots() {
        let text: Vec<char> = "Les annexes techniques restent la principale source".chars().collect();
        let hay: Vec<char> = text.iter().map(|&c| fold_char(c)).collect();
        let (s, e) = find_all(&hay, &fold_query("techniques"))[0];
        let (before, m, after) = snippet(&text, s, e, 10);
        assert_eq!(m, "techniques");
        assert!(before.ends_with("annexes "));
        assert!(after.starts_with(" restent"));
        assert!(before.starts_with('…'));
    }
}
