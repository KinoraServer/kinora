//! API tests: the whole router in-process with a temporary database.
//! No network, no ffmpeg, no open ports.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::config::Config;

struct TestApp {
    app: axum::Router,
    _dir: tempfile::TempDir,
}

async fn app() -> TestApp {
    let dir = tempfile::tempdir().unwrap();
    let config = Config {
        bind: "127.0.0.1".into(),
        port: 0,
        config_dir: dir.path().join("config"),
        cache_dir: dir.path().join("cache"),
        transcode_dir: dir.path().join("transcodes"),
        ffmpeg: "ffmpeg".into(),
        ffprobe: "ffprobe".into(),
        tmdb_api_key: None,
        scan_concurrency: 2,
    };
    let state = crate::build_state(config).await.unwrap();
    TestApp { app: crate::router(state), _dir: dir }
}

impl TestApp {
    async fn call(&self, method: &str, uri: &str, body: Option<Value>, token: Option<&str>) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let req = match body {
            Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    async fn setup_owner(&self) -> String {
        let (s, body) =
            self.call("POST", "/api/setup", Some(json!({"username": "owner", "password": "password123"})), None).await;
        assert_eq!(s, StatusCode::OK, "{body}");
        body["token"].as_str().unwrap().to_string()
    }
}

#[tokio::test]
async fn first_run_setup_flow() {
    let t = app().await;
    let (_, info) = t.call("GET", "/api/system/info", None, None).await;
    assert_eq!(info["setup_required"], true);

    let token = t.setup_owner().await;

    let (_, info) = t.call("GET", "/api/system/info", None, None).await;
    assert_eq!(info["setup_required"], false);

    // Setup can't be run twice.
    let (s, _) = t.call("POST", "/api/setup", Some(json!({"username": "x", "password": "password123"})), None).await;
    assert_eq!(s, StatusCode::CONFLICT);

    let (s, me) = t.call("GET", "/api/auth/me", None, Some(&token)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(me["role"], "owner");
}

#[tokio::test]
async fn routes_require_sign_in() {
    let t = app().await;
    for uri in ["/api/home", "/api/items", "/api/libraries", "/api/auth/me"] {
        let (s, _) = t.call("GET", uri, None, None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{uri}");
    }
    let (s, _) = t.call("GET", "/api/home", None, Some("not-a-real-token")).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_and_wrong_password() {
    let t = app().await;
    t.setup_owner().await;
    let (s, _) =
        t.call("POST", "/api/auth/login", Some(json!({"username": "owner", "password": "nope-nope"})), None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, body) =
        t.call("POST", "/api/auth/login", Some(json!({"username": "OWNER", "password": "password123"})), None).await;
    assert_eq!(s, StatusCode::OK, "usernames are case-insensitive");
    assert!(body["token"].as_str().unwrap().len() >= 32);
}

#[tokio::test]
async fn viewers_cannot_do_admin_things() {
    let t = app().await;
    let owner = t.setup_owner().await;
    let (s, _) = t
        .call(
            "POST",
            "/api/users",
            Some(json!({"username": "kid", "password": "password123", "role": "viewer"})),
            Some(&owner),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (_, login) =
        t.call("POST", "/api/auth/login", Some(json!({"username": "kid", "password": "password123"})), None).await;
    let viewer = login["token"].as_str().unwrap().to_string();

    for (method, uri) in [("GET", "/api/users"), ("GET", "/api/modules"), ("GET", "/api/system/settings")] {
        let (s, _) = t.call(method, uri, None, Some(&viewer)).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{uri}");
    }
    let (s, _) = t
        .call("POST", "/api/libraries", Some(json!({"name": "M", "kind": "movies", "paths": ["/"]})), Some(&viewer))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // But they can browse.
    let (s, _) = t.call("GET", "/api/home", None, Some(&viewer)).await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn player_style_is_server_wide_and_admin_only() {
    let t = app().await;
    let owner = t.setup_owner().await;
    let (_, body) = t.call("GET", "/api/player/style", None, Some(&owner)).await;
    assert_eq!(body["style"], "theme");

    let (s, _) = t.call("PUT", "/api/player/style", Some(json!({"style": "Bad Id!"})), Some(&owner)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = t.call("PUT", "/api/player/style", Some(json!({"style": "cinematic"})), Some(&owner)).await;
    assert_eq!(s, StatusCode::OK);

    t.call(
        "POST",
        "/api/users",
        Some(json!({"username": "kid", "password": "password123", "role": "viewer"})),
        Some(&owner),
    )
    .await;
    let (_, login) =
        t.call("POST", "/api/auth/login", Some(json!({"username": "kid", "password": "password123"})), None).await;
    let viewer = login["token"].as_str().unwrap().to_string();

    // Every signed-in client reads the same style, but only admins can change it.
    let (s, body) = t.call("GET", "/api/player/style", None, Some(&viewer)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["style"], "cinematic");
    let (s, _) = t.call("PUT", "/api/player/style", Some(json!({"style": "pro"})), Some(&viewer)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = t.call("GET", "/api/player/style", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn keepalive_for_a_missing_stream_is_404() {
    let t = app().await;
    let owner = t.setup_owner().await;
    let (s, _) = t.call("POST", "/api/hls/nope/keepalive", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = t.call("POST", "/api/hls/nope/keepalive", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn themes_are_public_and_include_builtins() {
    let t = app().await;
    let (s, body) = t.call("GET", "/api/themes", None, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["default_theme"], "kinora");
    let ids: Vec<&str> = body["themes"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"kinora") && ids.contains(&"midnight") && ids.contains(&"daylight"));
}

#[tokio::test]
async fn module_secrets_are_never_returned() {
    let t = app().await;
    let owner = t.setup_owner().await;
    let (s, _) =
        t.call("PUT", "/api/modules/tmdb", Some(json!({"settings": {"api_key": "super-secret"}})), Some(&owner)).await;
    assert_eq!(s, StatusCode::OK);
    let (_, modules) = t.call("GET", "/api/modules", None, Some(&owner)).await;
    let text = modules.to_string();
    assert!(!text.contains("super-secret"));
    let tmdb = modules.as_array().unwrap().iter().find(|m| m["id"] == "tmdb").unwrap();
    assert_eq!(tmdb["secrets_set"], json!(["api_key"]));
}

#[tokio::test]
async fn library_needs_existing_folder() {
    let t = app().await;
    let owner = t.setup_owner().await;
    let (s, body) = t
        .call(
            "POST",
            "/api/libraries",
            Some(json!({"name": "Movies", "kind": "movies", "paths": ["/definitely/not/here"]})),
            Some(&owner),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("folder not found"));
}
