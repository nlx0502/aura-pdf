use super::*;

#[test]
fn version_et_langues() {
    let v = unsafe { std::ffi::CStr::from_ptr(ffi::TessVersion()) }.to_string_lossy().to_string();
    assert!(v.starts_with('5'), "{v}");
    assert_eq!(available_langs(), vec!["fra", "eng", "deu", "nld"]);
}

#[test]
fn page_blanche() {
    let rgba = vec![255u8; 900 * 160 * 4];
    let mut t = Tess::new(&["fra".into()]).unwrap();
    let res = t.recognize(&PageImage { width: 900, height: 160, rgba }, 200.0).unwrap();
    assert!(res.words.is_empty(), "aucun mot sur une page blanche");
}
