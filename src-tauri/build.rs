use std::path::PathBuf;

fn main() {
    link_tesseract();
    tauri_build::build()
}

/// Tesseract et ses dépendances (Leptonica, images, compression), compilés en
/// bibliothèques statiques par vcpkg (triplet x64-windows-static-md) :
///   vcpkg install tesseract:x64-windows-static-md
/// Dossier : variable AURA_VCPKG_ROOT, sinon D:\DevTools\vcpkg.
fn link_tesseract() {
    println!("cargo:rerun-if-env-changed=AURA_VCPKG_ROOT");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let root = std::env::var("AURA_VCPKG_ROOT").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(r"D:\DevTools\vcpkg"));
    let lib = root.join("installed").join("x64-windows-static-md").join("lib");
    if !lib.exists() {
        println!("cargo:warning=Bibliothèques Tesseract introuvables dans {} : lancez « vcpkg install tesseract:x64-windows-static-md ».", lib.display());
        return;
    }
    println!("cargo:rerun-if-changed={}", lib.display());
    println!("cargo:rustc-link-search=native={}", lib.display());
    let mut names: Vec<String> = std::fs::read_dir(&lib)
        .expect("dossier des bibliothèques")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "lib").unwrap_or(false))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
        .collect();
    names.sort();
    for n in names {
        println!("cargo:rustc-link-lib=static={n}");
    }
    for sys in ["ws2_32", "crypt32", "bcrypt", "advapi32", "user32", "secur32", "normaliz", "wldap32", "iphlpapi", "gdi32", "ole32", "shell32"] {
        println!("cargo:rustc-link-lib=dylib={sys}");
    }
}
