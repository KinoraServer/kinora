//! The web app, compiled into the binary. Everything that isn't /api is
//! served from here; unknown paths get index.html (the app uses hash routing,
//! so this only matters for bookmarks of the bare URL).

use axum::body::Body;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
struct Assets;

pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let path = if path.is_empty() { "index.html" } else { path };
    match Assets::get(path) {
        Some(file) => {
            let cache = if path.starts_with("assets/") {
                "public, max-age=31536000, immutable" // file names contain a content hash
            } else {
                "no-cache"
            };
            (
                [(header::CONTENT_TYPE, file.metadata.mimetype().to_string()), (header::CACHE_CONTROL, cache.into())],
                Body::from(file.data),
            )
                .into_response()
        }
        None => match Assets::get("index.html") {
            Some(index) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], Body::from(index.data)).into_response(),
            None => (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                "<h1>Kinora</h1><p>The web app wasn't built into this binary. Run <code>pnpm build</code> in web/ and rebuild.</p>",
            )
                .into_response(),
        },
    }
}
