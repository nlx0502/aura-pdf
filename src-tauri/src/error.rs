//! Erreurs applicatives sérialisées vers l'interface.
//!
//! Chaque erreur porte un `code` stable (utilisé par l'interface pour choisir
//! un comportement, ex. demander un mot de passe) et un `message` en français,
//! prêt à être affiché tel quel.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
}

impl AppError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    pub fn password_required() -> Self {
        Self::new("password_required", "Ce document est protégé par un mot de passe.")
    }

    pub fn wrong_password() -> Self {
        Self::new("wrong_password", "Mot de passe incorrect. Vérifiez la saisie et réessayez.")
    }

    pub fn corrupted() -> Self {
        Self::new(
            "corrupted",
            "Ce fichier est endommagé ou n’est pas un PDF valide.",
        )
    }

    pub fn file(detail: impl std::fmt::Display) -> Self {
        Self::new(
            "file",
            format!("Impossible de lire le fichier. Il a peut-être été déplacé ou supprimé. ({detail})"),
        )
    }

    pub fn not_found() -> Self {
        Self::new("not_found", "Ce document n’est plus ouvert.")
    }

    pub fn internal(detail: impl std::fmt::Display) -> Self {
        Self::new("internal", format!("Une erreur inattendue s’est produite. ({detail})"))
    }

    pub fn cancelled() -> Self {
        Self::new("cancelled", "Opération annulée.")
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::file(e)
    }
}

pub type AppResult<T> = Result<T, AppError>;
