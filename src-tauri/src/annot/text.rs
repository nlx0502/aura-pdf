//! Texte des apparences : encodage WinAnsi, largeurs Helvetica, découpage en lignes.

/// Largeurs Helvetica (1/1000 em) des caractères ASCII 32 à 126.
const HELV: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // espace à /
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // 0-9
    278, 278, 584, 584, 584, 556, 1015, // : ; < = > ? @
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944,
    667, 667, 611, // A-Z
    278, 278, 278, 469, 556, 333, // [ \ ] ^ _ `
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722,
    500, 500, 500, // a-z
    334, 260, 334, 584, // { | } ~
];

/// Largeurs Times-Roman (1/1000 em) des caractères ASCII 32 à 126.
const TIMES: [u16; 95] = [
    250, 333, 408, 500, 500, 833, 778, 180, 333, 333, 500, 564, 250, 333, 250, 278, // espace à /
    500, 500, 500, 500, 500, 500, 500, 500, 500, 500, // 0-9
    278, 278, 564, 564, 564, 444, 921, // : à @
    722, 667, 667, 722, 611, 556, 722, 722, 333, 389, 722, 611, 889, 722, 722, 556, 722, 667, 556, 611, 722, 722, 944,
    722, 722, 611, // A-Z
    333, 278, 333, 469, 500, 333, // [ à `
    444, 500, 444, 500, 444, 333, 500, 500, 278, 278, 500, 278, 778, 500, 500, 500, 500, 333, 389, 278, 500, 500, 722,
    500, 500, 444, // a-z
    480, 200, 480, 541, // { à ~
];

/// Largeur d'un caractère (1/1000 em) pour une famille : « sans » (Helvetica),
/// « serif » (Times) ou « mono » (Courier). `bold` élargit légèrement.
pub fn char_width_in(c: char, family: &str, bold: bool) -> f32 {
    match family {
        "mono" => 600.0,
        "serif" => {
            let b = base(c) as u32;
            let w = if (32..=126).contains(&b) { TIMES[(b - 32) as usize] as f32 } else { 500.0 };
            if bold { w * 1.04 } else { w }
        }
        _ => char_width(c, bold),
    }
}

pub fn text_width_in(s: &str, size: f32, family: &str, bold: bool) -> f32 {
    s.chars().map(|c| char_width_in(c, family, bold)).sum::<f32>() * size / 1000.0
}

/// Découpe un texte en lignes tenant dans `max_w` points, pour une police donnée.
pub fn wrap_in(text: &str, size: f32, max_w: f32, family: &str, bold: bool) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if !line.is_empty() && text_width_in(&candidate, size, family, bold) > max_w {
                out.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = candidate;
            }
        }
        out.push(line);
    }
    out
}

/// Lettre de base (sans accent) pour l'estimation des largeurs.
fn base(c: char) -> char {
    match c {
        'à' | 'â' | 'ä' | 'á' | 'ã' => 'a',
        'À' | 'Â' | 'Ä' | 'Á' | 'Ã' => 'A',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'É' | 'È' | 'Ê' | 'Ë' => 'E',
        'î' | 'ï' | 'í' => 'i',
        'Î' | 'Ï' | 'Í' => 'I',
        'ô' | 'ö' | 'ó' | 'õ' => 'o',
        'Ô' | 'Ö' | 'Ó' | 'Õ' => 'O',
        'ù' | 'û' | 'ü' | 'ú' => 'u',
        'Ù' | 'Û' | 'Ü' | 'Ú' => 'U',
        'ç' => 'c',
        'Ç' => 'C',
        'œ' => 'o',
        'Œ' => 'O',
        '’' | '‘' => '\'',
        '«' | '»' | '“' | '”' => '"',
        '—' | '–' => '-',
        _ => c,
    }
}

/// Largeur d'un caractère, en 1/1000 em (Helvetica ; `bold` élargit légèrement).
pub fn char_width(c: char, bold: bool) -> f32 {
    let w = match c {
        'œ' => 944.0,
        'Œ' => 1000.0,
        '—' => 1000.0,
        '–' => 556.0,
        '…' => 1000.0,
        _ => {
            let b = base(c) as u32;
            if (32..=126).contains(&b) {
                HELV[(b - 32) as usize] as f32
            } else {
                556.0
            }
        }
    };
    if bold {
        w * 1.06
    } else {
        w
    }
}

pub fn text_width(s: &str, size: f32, bold: bool, tracking: f32) -> f32 {
    let n = s.chars().count() as f32;
    s.chars().map(|c| char_width(c, bold)).sum::<f32>() * size / 1000.0 + tracking * (n - 1.0).max(0.0)
}

/// Découpe un paragraphe en lignes tenant dans `max_w` points.
pub fn wrap(text: &str, size: f32, max_w: f32) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if !line.is_empty() && text_width(&candidate, size, false, 0.0) > max_w {
                out.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = candidate;
            }
        }
        out.push(line);
    }
    out
}

/// Chaîne littérale PDF encodée en WinAnsi (caractères hors jeu remplacés par « ? »).
pub fn pdf_string(s: &str) -> Vec<u8> {
    let mut out = vec![b'('];
    for c in s.chars() {
        let b: u8 = match c {
            '€' => 0x80,
            '‚' => 0x82,
            '…' => 0x85,
            'Œ' => 0x8C,
            '‘' => 0x91,
            '’' => 0x92,
            '“' => 0x93,
            '”' => 0x94,
            '•' => 0x95,
            '–' => 0x96,
            '—' => 0x97,
            'œ' => 0x9C,
            c if (c as u32) < 0x80 => c as u8,
            c if (0xA0..=0xFF).contains(&(c as u32)) => c as u32 as u8,
            _ => b'?',
        };
        if matches!(b, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        out.push(b);
    }
    out.push(b')');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn largeurs() {
        assert!((text_width("AAA", 10.0, false, 0.0) - 20.01).abs() < 0.01);
        assert!(text_width("été", 10.0, false, 0.0) > 0.0);
    }

    #[test]
    fn decoupage() {
        let l = wrap("Point à clarifier avant la signature du contrat", 12.0, 100.0);
        assert!(l.len() > 1);
        assert!(l.iter().all(|x| text_width(x, 12.0, false, 0.0) <= 100.0 || !x.contains(' ')));
    }

    #[test]
    fn encodage_winansi() {
        assert_eq!(pdf_string("é(a)"), b"(\xE9\\(a\\))".to_vec());
        assert_eq!(pdf_string("l’été"), b"(l\x92\xE9t\xE9)".to_vec());
    }
}
