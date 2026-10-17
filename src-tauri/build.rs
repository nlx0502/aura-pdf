use std::path::PathBuf;

fn main() {
    link_tesseract();
    tauri_build::build()
}

/// Tesseract et ses dépendances (Leptonica, images, compression), compilés en
/// bibliothèques statiques par vcpkg :
///   Windows : vcpkg install tesseract:x64-windows-static-md
///   macOS   : vcpkg install tesseract:arm64-osx   (Intel : x64-osx)
/// Dossier : variable AURA_VCPKG_ROOT, sinon VCPKG_ROOT, sinon D:\DevTools\vcpkg.
fn link_tesseract() {
    println!("cargo:rerun-if-env-changed=AURA_VCPKG_ROOT");
    println!("cargo:rerun-if-env-changed=VCPKG_ROOT");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let (triplet, ext) = match (os.as_str(), arch.as_str()) {
        ("windows", _) => ("x64-windows-static-md", "lib"),
        ("macos", "aarch64") => ("arm64-osx", "a"),
        ("macos", _) => ("x64-osx", "a"),
        _ => return,
    };
    let root = std::env::var("AURA_VCPKG_ROOT")
        .or_else(|_| std::env::var("VCPKG_ROOT"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"D:\DevTools\vcpkg"));
    let lib = root.join("installed").join(triplet).join("lib");
    if !lib.exists() {
        println!("cargo:warning=Bibliothèques Tesseract introuvables dans {} : lancez « vcpkg install tesseract:{triplet} ».", lib.display());
        return;
    }
    println!("cargo:rerun-if-changed={}", lib.display());
    println!("cargo:rustc-link-search=native={}", lib.display());
    let mut names: Vec<String> = std::fs::read_dir(&lib)
        .expect("dossier des bibliothèques")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == ext).unwrap_or(false))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
        // macOS : « libtesseract55.a » se lie sous le nom « tesseract55 ».
        .map(|n| if ext == "a" { n.strip_prefix("lib").map(str::to_string).unwrap_or(n) } else { n })
        .collect();
    names.sort();
    for n in names {
        println!("cargo:rustc-link-lib=static={n}");
    }
    if os == "windows" {
        for sys in ["ws2_32", "crypt32", "bcrypt", "advapi32", "user32", "secur32", "normaliz", "wldap32", "iphlpapi", "gdi32", "ole32", "shell32"] {
            println!("cargo:rustc-link-lib=dylib={sys}");
        }
    } else {
        // Tesseract est en C++ ; libarchive et curl s'appuient sur iconv et les
        // services réseau et de sécurité du système.
        for sys in ["c++", "iconv"] {
            println!("cargo:rustc-link-lib=dylib={sys}");
        }
        for fw in ["CoreFoundation", "Security", "SystemConfiguration"] {
            println!("cargo:rustc-link-lib=framework={fw}");
        }
    }
}
