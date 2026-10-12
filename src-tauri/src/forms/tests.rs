use super::*;
use lopdf::Document;

fn fixture() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("tests").join("fixtures").join("Demande d’ouverture.pdf")
}

fn bytes() -> Option<Vec<u8>> {
    std::fs::read(fixture()).ok()
}

fn by_name<'a>(f: &'a [Field], n: &str) -> &'a Field {
    f.iter().find(|x| x.name == n || x.label == n).unwrap_or_else(|| panic!("champ {n}"))
}

pub(crate) fn new_field(kind: &str, label: &str, rects: &[Rect]) -> Field {
    Field {
        name: String::new(),
        is_new: true,
        label: label.into(),
        kind: kind.into(),
        value: String::new(),
        options: vec![],
        required: false,
        readonly: false,
        multiline: false,
        max_len: None,
        placeholder: String::new(),
        style: style::FieldStyle::default_for(kind),
        widgets: rects.iter().map(|r| Widget { page: 0, rect: *r, on: None, caption: None, index: None }).collect(),
        caption: None,
    }
}

fn caption(text: &str, x: f32, y: f32) -> Caption {
    Caption { text: text.into(), rect: Rect { x, y, w: 120.0, h: 13.0 }, size: 10.0, color: "#3a3b40".into(), bold: true, page: 0, index: None }
}

fn reload(out: &[u8]) -> (Document, Vec<Field>) {
    let doc = Document::load_mem(out).unwrap();
    let f = list(&doc);
    (doc, f)
}

#[test]
fn lecture_des_champs() {
    let Some(b) = bytes() else { return };
    let f = list(&Document::load_mem(&b).unwrap());
    assert_eq!(f.iter().map(|x| x.kind.as_str()).collect::<Vec<_>>(), vec!["text", "email", "select", "radio", "date", "check", "sig"]);
    let nom = by_name(&f, "nom");
    assert_eq!((nom.label.as_str(), nom.value.as_str(), nom.required), ("Nom complet", "Camille Laurent", true));
    assert!(nom.widgets[0].rect.y > 180.0 && nom.widgets[0].rect.y < 215.0, "coordonnées d'affichage");
    assert!(nom.widgets[0].index.is_some());
    assert_eq!(nom.style.background.as_deref(), Some("#ffffff"));
    let t = by_name(&f, "type");
    assert_eq!(t.options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), vec!["Particulier", "Professionnel", "Association"]);
    let r = by_name(&f, "frequence");
    assert_eq!((r.value.as_str(), r.widgets.len(), r.options.len()), ("Mensuelle", 3, 3));
    let c = by_name(&f, "cgu");
    assert_eq!((c.value.as_str(), c.widgets[0].on.as_deref()), ("", Some("Oui")));
}

#[test]
fn remplissage_style_creation_suppression() {
    let Some(b) = bytes() else { return };
    let size = b.len();
    let orig = list(&Document::load_mem(&b).unwrap());
    let mut email = by_name(&orig, "email").clone();
    email.value = "camille@exemple.fr".into();
    email.style.background = Some("#eef2ff".into());
    email.style.border = Some("#6e6af6".into());
    email.style.border_width = 2.0;
    email.style.border_style = "dashed".into();
    email.style.text_color = "#1f2a6b".into();
    email.style.font = "serif".into();
    email.style.font_size = 11.0;
    email.style.align = 1;
    email.placeholder = "nom@exemple.fr".into();
    email.widgets[0].rect.x += 20.0;
    email.caption = Some(caption("Adresse e-mail *", 64.0, 240.0));
    let mut kind = by_name(&orig, "type").clone();
    kind.value = "Professionnel".into();
    let mut freq = by_name(&orig, "frequence").clone();
    freq.value = "Annuelle".into();
    let mut cgu = by_name(&orig, "cgu").clone();
    cgu.value = "Oui".into();
    cgu.style.check_style = "cross".into();
    let mut naissance = by_name(&orig, "naissance").clone();
    naissance.value = "14/03/1990".into();
    naissance.label = "Né(e) le".into();
    naissance.required = true;

    let mut conseiller = new_field("email", "E-mail du conseiller", &[Rect { x: 64.0, y: 700.0, w: 200.0, h: 22.0 }]);
    conseiller.required = true;
    conseiller.placeholder = "conseiller@banque.fr".into();
    conseiller.caption = Some(caption("E-mail du conseiller *", 64.0, 684.0));
    let mut news = new_field("check", "Newsletter", &[Rect { x: 300.0, y: 700.0, w: 14.0, h: 14.0 }]);
    news.style.check_style = "star".into();
    news.caption = Some(caption("Recevoir la newsletter", 320.0, 700.0));
    let mut agence = new_field("select", "Agence", &[Rect { x: 64.0, y: 740.0, w: 200.0, h: 22.0 }]);
    agence.options = ["Paris", "Lyon"].iter().map(|v| FieldOption { value: v.to_string(), label: v.to_string() }).collect();
    agence.value = "Lyon".into();
    let mut contact = new_field("radio", "Contact préféré", &[Rect { x: 300.0, y: 740.0, w: 12.0, h: 12.0 }, Rect { x: 380.0, y: 740.0, w: 12.0, h: 12.0 }]);
    contact.options = vec![FieldOption { value: "Telephone".into(), label: "Téléphone".into() }, FieldOption { value: "Courrier".into(), label: "Courrier".into() }];
    contact.widgets[0].caption = Some(caption("Téléphone", 316.0, 740.0));
    contact.widgets[1].caption = Some(caption("Courrier", 396.0, 740.0));
    contact.value = "Courrier".into();

    let plan = FormPlan { upsert: vec![email, kind, freq, cgu, naissance, conseiller, news, agence, contact], delete: vec!["signature".into()] };
    let out = save(b.clone(), None, &plan).unwrap();
    // Mise à jour incrémentale : l'original est intact en tête.
    assert_eq!(&out[..size - 64], &b[..size - 64]);
    let (doc, f) = reload(&out);

    let e = by_name(&f, "email");
    assert_eq!(e.value, "camille@exemple.fr");
    assert_eq!(e.style.background.as_deref(), Some("#eef2ff"));
    assert_eq!((e.style.border.as_deref(), e.style.border_width, e.style.border_style.as_str()), (Some("#6e6af6"), 2.0, "dashed"));
    assert_eq!((e.style.font.as_str(), e.style.font_size, e.style.align, e.style.text_color.as_str()), ("serif", 11.0, 1, "#1f2a6b"));
    assert_eq!(e.placeholder, "nom@exemple.fr");
    assert!((e.widgets[0].rect.x - 84.0).abs() < 0.1, "champ déplacé");
    assert_eq!(e.caption.as_ref().map(|c| c.text.as_str()), Some("Adresse e-mail *"));
    assert_eq!(by_name(&f, "type").value, "Professionnel");
    assert_eq!(by_name(&f, "frequence").value, "Annuelle");
    let c = by_name(&f, "cgu");
    assert_eq!((c.value.as_str(), c.style.check_style.as_str()), ("Oui", "cross"));
    let n = by_name(&f, "naissance");
    assert_eq!((n.label.as_str(), n.required, n.kind.as_str()), ("Né(e) le", true, "date"));
    assert!(f.iter().all(|x| x.name != "signature"), "champ supprimé");
    let ec = by_name(&f, "e_mail_du_conseiller");
    assert_eq!((ec.kind.as_str(), ec.required, ec.placeholder.as_str()), ("email", true, "conseiller@banque.fr"));
    assert!((ec.widgets[0].rect.y - 700.0).abs() < 0.5);
    let nw = by_name(&f, "newsletter");
    assert_eq!((nw.kind.as_str(), nw.style.check_style.as_str()), ("check", "star"));
    assert_eq!(nw.caption.as_ref().unwrap().text, "Recevoir la newsletter");
    assert_eq!(by_name(&f, "agence").value, "Lyon");
    let ct = by_name(&f, "contact_prefere");
    assert_eq!((ct.kind.as_str(), ct.value.as_str(), ct.widgets.len()), ("radio", "Courrier", 2));
    assert_eq!(ct.options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), vec!["Téléphone", "Courrier"]);
    // Les libellés ne sont pas listés comme des champs.
    assert!(f.iter().all(|x| !x.name.starts_with("aura_libelle")));

    // Le champ e-mail a une apparence contenant sa valeur, en Times, cadre pointillé.
    let node = nodes(&doc).into_iter().find(|n| n.name == "email").unwrap();
    let w = doc.get_dictionary(node.widgets[0]).unwrap();
    let ap = appearance_of(&doc, w).unwrap();
    let s = doc.get_object(ap).unwrap().as_stream().unwrap();
    let content = String::from_utf8_lossy(&s.decompressed_content().unwrap_or_else(|_| s.content.clone())).into_owned();
    assert!(content.contains("camille@exemple.fr") && content.contains("/TiRo") && content.contains("[3 2] 0 d"), "{content}");

    // Deuxième enregistrement : libellé modifié, puis retiré ; pas de doublon.
    let mut e2 = e.clone();
    e2.caption = Some(caption("Courriel", 64.0, 240.0));
    let out2 = save(out.clone(), None, &FormPlan { upsert: vec![e2.clone()], delete: vec![] }).unwrap();
    let (_, f2) = reload(&out2);
    assert_eq!(by_name(&f2, "email").caption.as_ref().unwrap().text, "Courriel");
    e2.caption = None;
    let out3 = save(out2, None, &FormPlan { upsert: vec![e2], delete: vec![] }).unwrap();
    let (doc3, f3) = reload(&out3);
    assert!(by_name(&f3, "email").caption.is_none());
    let labels = nodes(&doc3).iter().filter(|n| n.caption_of().as_deref() == Some("email")).count();
    assert_eq!(labels, 0, "ancien libellé retiré");
}

#[test]
fn aplatissement() {
    let Some(b) = bytes() else { return };
    let orig = list(&Document::load_mem(&b).unwrap());
    let mut email = by_name(&orig, "email").clone();
    email.value = "a@b.fr".into();
    email.caption = Some(caption("Courriel", 64.0, 240.0));
    let filled = save(b, None, &FormPlan { upsert: vec![email], delete: vec![] }).unwrap();
    let flat = flatten(&filled, None).unwrap();
    let doc = Document::load_mem(&flat).unwrap();
    assert!(list(&doc).is_empty(), "plus de formulaire");
    let page = doc.get_dictionary(*doc.get_pages().values().next().unwrap()).unwrap();
    assert!(page.get(b"Annots").is_err(), "widgets retirés");
    assert!(doc.extract_text(&[1]).unwrap_or_default().contains("Demande"), "contenu d'origine conservé");
    let xo = get(&doc, get(&doc, page, b"Resources").unwrap().as_dict().unwrap(), b"XObject").unwrap().as_dict().unwrap();
    assert!(xo.len() >= 7, "apparences et libellé dessinés dans la page");
}

#[test]
fn noms_de_champs() {
    assert_eq!(slug("E-mail du conseiller"), "e_mail_du_conseiller");
    assert_eq!(slug("Né(e) le"), "ne_e_le");
    assert_eq!(slug("  "), "");
}

/// Formulaire stylé à ouvrir dans Acrobat (vérification manuelle) :
/// `cargo test --lib exporter_formulaire_acrobat -- --ignored`.
#[test]
#[ignore]
fn exporter_formulaire_acrobat() {
    let b = bytes().unwrap();
    let orig = list(&Document::load_mem(&b).unwrap());
    let mut email = by_name(&orig, "email").clone();
    email.style.background = Some("#eef2ff".into());
    email.style.border = Some("#6e6af6".into());
    email.style.border_width = 1.5;
    email.style.border_style = "dashed".into();
    email.style.text_color = "#1f2a6b".into();
    email.style.font = "serif".into();
    email.placeholder = "nom@exemple.fr".into();
    let mut nom = by_name(&orig, "nom").clone();
    nom.style.border_style = "underline".into();
    nom.style.background = None;
    nom.style.align = 1;
    nom.style.bold = true;
    let mut cgu = by_name(&orig, "cgu").clone();
    cgu.value = "Oui".into();
    cgu.style.check_style = "star".into();
    cgu.style.text_color = "#d2453d".into();
    let mut contact = new_field("radio", "Contact préféré", &[Rect { x: 330.0, y: 700.0, w: 12.0, h: 12.0 }, Rect { x: 430.0, y: 700.0, w: 12.0, h: 12.0 }]);
    contact.options = vec![FieldOption { value: "Telephone".into(), label: "Téléphone".into() }, FieldOption { value: "Courrier".into(), label: "Courrier".into() }];
    contact.widgets[0].caption = Some(caption("Téléphone", 346.0, 700.0));
    contact.widgets[1].caption = Some(caption("Courrier", 446.0, 700.0));
    contact.caption = Some(caption("Contact préféré", 330.0, 680.0));
    contact.value = "Courrier".into();
    let mut conseiller = new_field("text", "Conseiller", &[Rect { x: 64.0, y: 700.0, w: 220.0, h: 24.0 }]);
    conseiller.placeholder = "Prénom Nom".into();
    conseiller.style.background = Some("#fff7e6".into());
    conseiller.style.border = Some("#f2b84b".into());
    conseiller.caption = Some(caption("Conseiller attitré", 64.0, 684.0));
    let out = save(b, None, &FormPlan { upsert: vec![email, nom, cgu, contact, conseiller], delete: vec![] }).unwrap();
    let dir = std::env::temp_dir().join("aura-acrobat");
    let _ = std::fs::create_dir_all(&dir);
    std::fs::write(dir.join("formulaire.pdf"), out).unwrap();
}
