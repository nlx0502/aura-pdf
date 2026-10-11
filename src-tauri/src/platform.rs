//! Fonctions dépendantes du système d'exploitation.
//!
//! Tout le code propre à Windows est regroupé ici (et, plus tard, dans
//! `platform/windows/*` pour l'impression et le scanner) afin de faciliter un
//! portage macOS : il suffira de fournir les équivalents `#[cfg(target_os = "macos")]`.

/// Prénom ou nom d'affichage de l'utilisateur, pour l'accueil (« Bonjour, Olivier »).
pub fn display_name() -> String {
    let real = whoami::realname();
    let first = real.split_whitespace().next().unwrap_or("").to_string();
    if !first.is_empty() && first != "Unknown" {
        return first;
    }
    let user = whoami::username();
    let mut c = user.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
