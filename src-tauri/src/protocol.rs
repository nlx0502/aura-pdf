//! Protocole interne `aura://` (servi sous `http://aura.localhost/` sous Windows).
//!
//! Les bitmaps de pages transitent par ce protocole plutôt que par l'IPC JSON :
//! pas d'encodage base64, pas de copie inutile, et le navigateur peut annuler
//! les requêtes. Routes :
//!
//! - `/render?doc=&page=&w=&rot=&annots=&forms=&prio=&gen=` : bitmap RGBA brut
//!   précédé de 8 octets (largeur et hauteur en u32 petit-boutiste) ;
//! - `/file-thumb?path=&w=` : miniature JPEG de la première page d'un fichier
//!   (accueil, documents récents), mise en cache sur disque.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use tauri::http::{Request, Response};
use tauri::{Manager, UriSchemeContext, UriSchemeResponder, Wry};

use crate::engine::{prio, RenderJob, RenderOutcome};
use crate::AppState;

fn respond(responder: UriSchemeResponder, status: u16, content_type: &str, body: Vec<u8>) {
    let resp = Response::builder()
        .status(status)
        .header("Content-Type", content_type)
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", "no-store")
        .body(body)
        .unwrap_or_else(|_| Response::new(Vec::new()));
    responder.respond(resp);
}

/// Décodage des séquences `%XX` (UTF-8) et des `+`.
pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(b) => {
                        out.push(b);
                        i += 3;
                    }
                    None => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn parse_query(q: &str) -> HashMap<String, String> {
    q.split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| {
            let mut it = kv.splitn(2, '=');
            let k = percent_decode(it.next().unwrap_or(""));
            let v = percent_decode(it.next().unwrap_or(""));
            (k, v)
        })
        .collect()
}

pub fn handle(ctx: UriSchemeContext<'_, Wry>, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app = ctx.app_handle().clone();
    let uri = request.uri();
    let path = uri.path().to_string();
    let q = parse_query(uri.query().unwrap_or(""));
    let num = |k: &str, d: u64| q.get(k).and_then(|v| v.parse::<u64>().ok()).unwrap_or(d);

    match path.as_str() {
        "/render" => {
            let state = app.state::<AppState>();
            let job = RenderJob {
                doc: num("doc", 0) as u32,
                page: num("page", 0) as u32,
                width_px: num("w", 800) as u32,
                rotation: (num("rot", 0) % 4) as u8,
                annotations: num("annots", 1) == 1,
                forms: num("forms", 1) == 1,
                tile: q.get("tw").map(|_| {
                    [num("tx", 0) as u32, num("ty", 0) as u32, num("tw", 512) as u32, num("th", 512) as u32]
                }),
                prio: (num("prio", prio::VISIBLE as u64) as u8).min(prio::THUMB),
                gen: num("gen", 0),
                responder: Box::new(move |outcome| match outcome {
                    RenderOutcome::Ok(bmp) => {
                        let mut body = Vec::with_capacity(8 + bmp.rgba.len());
                        body.extend_from_slice(&bmp.width.to_le_bytes());
                        body.extend_from_slice(&bmp.height.to_le_bytes());
                        body.extend_from_slice(&bmp.rgba);
                        respond(responder, 200, "application/octet-stream", body);
                    }
                    RenderOutcome::Stale => respond(responder, 204, "text/plain", Vec::new()),
                    RenderOutcome::Err(e) => respond(responder, 500, "text/plain", e.into_bytes()),
                }),
            };
            state.engine.render(job);
        }
        "/file-thumb" => {
            let file = q.get("path").cloned().unwrap_or_default();
            let width = num("w", 240).clamp(32, 1024) as u32;
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AppState>();
                let meta = std::fs::metadata(&file);
                let Ok(meta) = meta else {
                    respond(responder, 404, "text/plain", b"introuvable".to_vec());
                    return;
                };
                // Clé de cache : chemin + date de modification + largeur.
                let mut h = std::collections::hash_map::DefaultHasher::new();
                file.to_lowercase().hash(&mut h);
                meta.modified().ok().hash(&mut h);
                width.hash(&mut h);
                let out = state.library.cache_dir().join("thumbs").join(format!("{:016x}.jpg", h.finish()));
                if !out.exists() {
                    if let Err(e) = state.engine.thumbnail(file.into(), out.clone(), width).await {
                        respond(responder, 500, "text/plain", e.message.into_bytes());
                        return;
                    }
                }
                match std::fs::read(&out) {
                    Ok(bytes) => respond(responder, 200, "image/jpeg", bytes),
                    Err(e) => respond(responder, 500, "text/plain", e.to_string().into_bytes()),
                }
            });
        }
        "/image" => {
            // Aperçu d'images locales (création de PDF) : extensions d'image uniquement.
            let file = q.get("path").cloned().unwrap_or_default();
            let ext = std::path::Path::new(&file)
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let mime = match ext.as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "bmp" => "image/bmp",
                "webp" => "image/webp",
                "tif" | "tiff" => "image/tiff",
                _ => {
                    respond(responder, 403, "text/plain", b"type refuse".to_vec());
                    return;
                }
            };
            tauri::async_runtime::spawn(async move {
                match std::fs::read(&file) {
                    Ok(bytes) => respond(responder, 200, mime, bytes),
                    Err(e) => respond(responder, 404, "text/plain", e.to_string().into_bytes()),
                }
            });
        }
        _ => respond(responder, 404, "text/plain", b"route inconnue".to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodage_url() {
        assert_eq!(percent_decode("C%3A%5CDocuments%5Crapport%20%C3%A9t%C3%A9.pdf"), "C:\\Documents\\rapport été.pdf");
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn analyse_requete() {
        let q = parse_query("doc=3&page=12&w=900");
        assert_eq!(q.get("page").map(String::as_str), Some("12"));
    }
}
