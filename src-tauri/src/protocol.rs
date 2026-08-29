//! Serving vault attachments to the webview.
//!
//! The webview cannot open a file by path, and it must not be handed one
//! either — no filesystem path crosses the boundary in this app, in either
//! direction. Tauri's built-in asset protocol would break that rule: the URLs
//! it produces embed the absolute path of the file being served.
//!
//! So we register our own scheme instead. The frontend asks for
//! `sutra://localhost/attachments/<name>` — exactly the vault-relative
//! reference already stored in the note's markdown — and Rust resolves it
//! against the open vault. The path exists only on this side.

use crate::state::AppState;
use percent_encoding::percent_decode_str;
use tauri::http::{Request, Response, StatusCode};
use tauri::{AppHandle, Manager, UriSchemeContext};

/// Guess a content type from the extension.
///
/// The webview needs this to decide whether it has an image. Only the formats
/// a notes vault actually holds are listed; anything else is served as binary,
/// which the browser will decline to render rather than guess at.
fn content_type(reference: &str) -> &'static str {
    let extension = reference
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase());
    match extension.as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    }
}

fn not_found() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Vec::new())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

/// Handle one `sutra://` request.
pub fn serve(
    ctx: UriSchemeContext<'_, tauri::Wry>,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let app: &AppHandle = ctx.app_handle();

    // The webview percent-encodes anything non-ASCII in the path, so decode
    // before handing it to the vault. A reference that is not valid UTF-8 is
    // not one we wrote.
    let raw = request.uri().path().trim_start_matches('/');
    let Ok(reference) = percent_decode_str(raw).decode_utf8() else {
        return not_found();
    };

    let state = app.state::<AppState>();
    let Ok(bytes) = state.with_vault(|vault| vault.read_attachment(&reference)) else {
        // Missing, refused, or no vault open. All indistinguishable to the
        // webview on purpose: a note should not be able to probe the
        // filesystem by watching which references 404 and which error.
        return not_found();
    };

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", content_type(&reference))
        // Attachments are immutable — the filename carries a fresh ULID every
        // time one is imported — so the webview never needs to re-fetch.
        .header("Cache-Control", "public, max-age=31536000, immutable")
        .body(bytes)
        .unwrap_or_else(|_| not_found())
}

#[cfg(test)]
mod tests {
    use super::content_type;

    #[test]
    fn content_types_come_from_the_extension() {
        assert_eq!(content_type("attachments/01H_x.png"), "image/png");
        assert_eq!(content_type("attachments/01H_x.JPEG"), "image/jpeg");
        assert_eq!(content_type("attachments/01H_x.pdf"), "application/pdf");
    }

    #[test]
    fn an_unknown_extension_is_not_guessed_at() {
        // Serving an unknown file as text/html would let a note's attachment
        // execute script in the webview's origin.
        assert_eq!(
            content_type("attachments/01H_x.html"),
            "application/octet-stream"
        );
        assert_eq!(
            content_type("attachments/01H_x"),
            "application/octet-stream"
        );
    }
}
