//! Annotations : lecture, génération des apparences et enregistrement
//! incrémental, sans dépendre de PDFium (lopdf, licence MIT).

pub mod ap;
pub mod geom;
pub mod model;
pub mod pdf;
pub mod read;
pub mod text;
pub mod write;

use std::path::Path;

use crate::error::{AppError, AppResult};
pub use model::Annot;

fn load_doc(path: &Path, password: Option<&str>) -> AppResult<lopdf::Document> {
    let bytes = std::fs::read(path)?;
    match password {
        Some(p) => lopdf::Document::load_mem_with_password(&bytes, p),
        None => lopdf::Document::load_mem(&bytes),
    }
    .map_err(|e| AppError::new("annot", format!("Impossible de lire les annotations ({e}).")))
}

/// Annotations modifiables d'un fichier.
pub fn load(path: &Path, password: Option<&str>) -> AppResult<Vec<Annot>> {
    Ok(read::read(&load_doc(path, password)?))
}

/// Enregistre les annotations dans le fichier (mise à jour incrémentale, écriture atomique).
pub fn save(path: &Path, password: Option<&str>, annots: &[Annot]) -> AppResult<()> {
    let bytes = std::fs::read(path)?;
    let out = write::save(bytes, password, annots).map_err(|e| AppError::new("save", format!("Impossible d’enregistrer les annotations ({e}).")))?;
    let tmp = path.with_extension("aura-tmp");
    std::fs::write(&tmp, &out).map_err(|e| AppError::new("save", format!("Impossible d’écrire le fichier ({e}).")))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        AppError::new("save", format!("Le fichier est utilisé par un autre programme ({e})."))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::geom::Rect;
    use super::model::{Kind, Reply};
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("tests").join("fixtures").join(name)
    }

    fn sample(kind: Kind, id: &str) -> Annot {
        Annot {
            id: id.into(),
            page: 1,
            kind,
            color: "#4FCB91".into(),
            opacity: 0.45,
            author: "Testeur".into(),
            rects: vec![Rect { x: 56.0, y: 120.0, w: 200.0, h: 14.0 }],
            rect: Some(Rect { x: 300.0, y: 300.0, w: 140.0, h: 80.0 }),
            ink: vec![vec![[10.0, 10.0], [40.0, 30.0], [80.0, 12.0]]],
            line: Some([100.0, 500.0, 220.0, 460.0]),
            text: if kind == Kind::Stamp { "APPROUVÉ".into() } else { "Zone de texte".into() },
            contents: "Point à vérifier".into(),
            ..Default::default()
        }
    }

    #[test]
    fn aller_retour_complet() {
        let src = fixture("Rapport Q1 2025.pdf");
        if !src.exists() {
            return;
        }
        let path = std::env::temp_dir().join(format!("aura-annot-{}.pdf", std::process::id()));
        std::fs::copy(&src, &path).unwrap();
        let kinds = [Kind::Highlight, Kind::Underline, Kind::Strike, Kind::Note, Kind::Text, Kind::Ink, Kind::Arrow, Kind::Rect, Kind::Ellipse, Kind::Line, Kind::Stamp];
        let mut list: Vec<Annot> = kinds.iter().enumerate().map(|(i, k)| sample(*k, &format!("aura-{i}"))).collect();
        list[0].replies.push(Reply { author: "Relecteur".into(), text: "D’accord".into(), ..Default::default() });
        list[1].resolved = true;
        let size_before = std::fs::metadata(&path).unwrap().len();
        save(&path, None, &list).unwrap();
        // Mise à jour incrémentale : le début du fichier est inchangé.
        let after = std::fs::read(&path).unwrap();
        assert!(after.len() as u64 > size_before);
        assert_eq!(&after[..size_before as usize - 64], &std::fs::read(&src).unwrap()[..size_before as usize - 64]);

        let back = load(&path, None).unwrap();
        assert_eq!(back.len(), kinds.len(), "toutes les annotations relues");
        let hl = back.iter().find(|a| a.id == "aura-0").unwrap();
        assert_eq!(hl.kind, Kind::Highlight);
        assert_eq!(hl.contents, "Point à vérifier");
        assert_eq!(hl.replies.len(), 1);
        assert_eq!(hl.replies[0].text, "D’accord");
        assert!((hl.rects[0].y - 120.0).abs() < 0.1, "coordonnées conservées");
        assert!(back.iter().find(|a| a.id == "aura-1").unwrap().resolved);
        assert_eq!(back.iter().find(|a| a.id == "aura-6").unwrap().kind, Kind::Arrow);
        let stamp = back.iter().find(|a| a.kind == Kind::Stamp).unwrap();
        assert_eq!(stamp.text, "APPROUVÉ");
        assert!(!stamp.locked);
        assert_eq!(back.iter().find(|a| a.kind == Kind::Text).unwrap().text, "Zone de texte");

        // Zone de texte personnalisée : police, style, couleur, alignement, fond, bordure.
        let mut styled = back.clone();
        let tb = styled.iter_mut().find(|a| a.kind == Kind::Text).unwrap();
        tb.font = "serif".into();
        tb.bold = true;
        tb.italic = true;
        tb.align = 1;
        tb.font_size = 18.0;
        tb.color = "#D2453D".into();
        tb.fill = Some("#FFF4D6".into());
        tb.border_color = None;
        tb.geom_dirty = true;
        save(&path, None, &styled).unwrap();
        let t2 = load(&path, None).unwrap().into_iter().find(|a| a.kind == Kind::Text).unwrap();
        assert_eq!((t2.font.as_str(), t2.bold, t2.italic, t2.align), ("serif", true, true, 1));
        assert_eq!(t2.font_size, 18.0);
        assert_eq!(t2.color, "#D2453D");
        assert_eq!(t2.fill.as_deref(), Some("#FFF4D6"));
        assert_eq!(t2.border_color, None, "pas de bordure par défaut");
        let back = load(&path, None).unwrap();

        // Deuxième passe : suppression, déplacement, nouveau commentaire.
        let mut next = back.clone();
        next.retain(|a| a.kind != Kind::Ellipse);
        let r = next.iter_mut().find(|a| a.kind == Kind::Rect).unwrap();
        r.rect = Some(Rect { x: 10.0, y: 10.0, w: 50.0, h: 50.0 });
        r.geom_dirty = true;
        let n = next.iter_mut().find(|a| a.kind == Kind::Note).unwrap();
        n.contents = "Nouveau texte".into();
        n.meta_dirty = true;
        save(&path, None, &next).unwrap();
        let back2 = load(&path, None).unwrap();
        assert_eq!(back2.len(), kinds.len() - 1);
        assert!(back2.iter().all(|a| a.kind != Kind::Ellipse));
        // Boîte relue à l'identique (pas de dérive d'un enregistrement à l'autre).
        assert_eq!(back2.iter().find(|a| a.kind == Kind::Rect).unwrap().rect, Some(Rect { x: 10.0, y: 10.0, w: 50.0, h: 50.0 }));
        let s1 = back.iter().find(|a| a.kind == Kind::Stamp).unwrap().rect.unwrap();
        let s2 = back2.iter().find(|a| a.kind == Kind::Stamp).unwrap().rect.unwrap();
        assert!((s1.x - s2.x).abs() < 0.01 && (s1.w - s2.w).abs() < 0.01);
        assert_eq!(back2.iter().find(|a| a.kind == Kind::Note).unwrap().contents, "Nouveau texte");
        // Les réponses survivent à la régénération (même numéro d'objet).
        assert_eq!(back2.iter().find(|a| a.id == "aura-0").unwrap().replies.len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn zone_de_texte_en_police_du_systeme() {
        let src = fixture("Rapport Q1 2025.pdf");
        if !src.exists() || !crate::sysfonts::list().iter().any(|f| f.family == "Arial") {
            return;
        }
        let path = std::env::temp_dir().join(format!("aura-sysfont-{}.pdf", std::process::id()));
        std::fs::copy(&src, &path).unwrap();
        let mut a = sample(Kind::Text, "aura-sys");
        a.font = "Arial".into();
        a.italic = true;
        a.text = "Été 2025 — Œuvre".into();
        let before = std::fs::metadata(&path).unwrap().len();
        save(&path, None, &[a]).unwrap();
        let back = load(&path, None).unwrap();
        let t = back.iter().find(|x| x.kind == Kind::Text).unwrap();
        assert_eq!((t.font.as_str(), t.italic, t.text.as_str()), ("Arial", true, "Été 2025 — Œuvre"));
        // Sous-ensemble intégré : quelques dizaines de Ko au plus (Arial complet ≈ 1 Mo).
        let added = std::fs::metadata(&path).unwrap().len() - before;
        assert!(added < 150_000, "police trop lourde : {added} octets");
        let doc = lopdf::Document::load(&path).unwrap();
        assert!(doc.objects.values().any(|o| o.as_dict().map(|d| d.has(b"FontFile2")).unwrap_or(false)));
        let _ = std::fs::remove_file(&path);
    }
}
