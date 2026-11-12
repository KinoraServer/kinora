//! API tests: the whole router in-process with a temporary database.
//! No network, no ffmpeg, no open ports.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::config::Config;
use crate::modules::test_support::{install_fixture, TestRegistry};

struct TestApp {
    app: axum::Router,
    db: sqlx::SqlitePool,
    state: crate::state::AppState,
    _dir: tempfile::TempDir,
}

async fn app() -> TestApp {
    // Nothing listens here: tests that don't bring a registry have none.
    app_with_registry("http://127.0.0.1:9", vec![]).await
}

async fn app_with_registry(registry_url: &str, registry_keys: Vec<String>) -> TestApp {
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
        registry_url: registry_url.into(),
        registry_keys,
    };
    let state = crate::build_state(config).await.unwrap();
    TestApp { db: state.db.clone(), app: crate::router(state.clone()), state, _dir: dir }
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

/// Sends raw bytes (not JSON) and returns the status and the raw response.
async fn raw(
    t: &TestApp,
    method: &str,
    uri: &str,
    body: Vec<u8>,
    token: &str,
) -> (StatusCode, Vec<u8>, Option<String>) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from(body))
        .unwrap();
    let resp = t.app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let ct = resp.headers().get(header::CONTENT_TYPE).map(|v| v.to_str().unwrap().to_string());
    (status, resp.into_body().collect().await.unwrap().to_bytes().to_vec(), ct)
}

#[tokio::test]
async fn people_edit_their_own_profile() {
    let t = app().await;
    let owner = t.setup_owner().await;
    t.call(
        "POST",
        "/api/users",
        Some(json!({"username": "kid", "password": "password123", "role": "viewer"})),
        Some(&owner),
    )
    .await;
    let (_, login) =
        t.call("POST", "/api/auth/login", Some(json!({"username": "kid", "password": "password123"})), None).await;
    let kid = login["token"].as_str().unwrap().to_string();
    let kid_id = login["user"]["id"].as_str().unwrap().to_string();
    assert_eq!(login["user"]["avatar_version"], Value::Null);

    // Rename: blank and taken names are refused.
    let (s, _) = t.call("PATCH", "/api/auth/profile", Some(json!({"username": "  "})), Some(&kid)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = t.call("PATCH", "/api/auth/profile", Some(json!({"username": "Owner"})), Some(&kid)).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, me) = t.call("PATCH", "/api/auth/profile", Some(json!({"username": " Sam "})), Some(&kid)).await;
    assert_eq!(s, StatusCode::OK, "{me}");
    assert_eq!(me["username"], "Sam");
    let (s, _) =
        t.call("POST", "/api/auth/login", Some(json!({"username": "sam", "password": "password123"})), None).await;
    assert_eq!(s, StatusCode::OK, "signs in with the new name");

    // Password change needs the current one.
    let (s, _) = t
        .call(
            "POST",
            "/api/auth/password",
            Some(json!({"current_password": "nope", "new_password": "newpass123"})),
            Some(&kid),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = t
        .call(
            "POST",
            "/api/auth/password",
            Some(json!({"current_password": "password123", "new_password": "newpass123"})),
            Some(&kid),
        )
        .await;
    assert_eq!(s, StatusCode::OK);

    // Pictures: only real images, served back with the detected type.
    let (s, _, _) = raw(&t, "PUT", "/api/auth/avatar", b"<svg onload=alert(1)>".to_vec(), &kid).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _, _) = raw(&t, "PUT", "/api/auth/avatar", vec![0xFF; 1024 * 1024 + 1], &kid).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let png = b"\x89PNG\r\n\x1a\nnot really a png".to_vec();
    let (s, body, _) = raw(&t, "PUT", "/api/auth/avatar", png.clone(), &kid).await;
    assert_eq!(s, StatusCode::OK);
    let me: Value = serde_json::from_slice(&body).unwrap();
    assert!(me["avatar_version"].as_i64().is_some());
    let (_, me) = t.call("GET", "/api/auth/me", None, Some(&kid)).await;
    assert!(me["avatar_version"].as_i64().is_some());

    let uri = format!("/api/users/{kid_id}/avatar");
    let (s, body, ct) = raw(&t, "GET", &uri, vec![], &owner).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body, png);
    assert_eq!(ct.as_deref(), Some("image/png"));

    let (s, me) = t.call("DELETE", "/api/auth/avatar", None, Some(&kid)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(me["avatar_version"], Value::Null);
    let (s, _, _) = raw(&t, "GET", &uri, vec![], &owner).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
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
    let (s, _) = t.call("PUT", "/api/libraries/any", Some(json!({"name": "Mine"})), Some(&viewer)).await;
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
    // Kinora's own pair; the other themes are installed from the registry.
    assert_eq!(ids, ["kinora", "kinora-light"]);
}

#[tokio::test]
async fn module_secrets_are_never_returned() {
    let t = app().await;
    let owner = t.setup_owner().await;
    // Not installed yet: there is nothing to configure.
    let body = json!({"settings": {"api_key": "super-secret"}});
    let (s, _) = t.call("PUT", "/api/modules/tmdb", Some(body.clone()), Some(&owner)).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    install_fixture(&t.state, "tmdb", json!({})).await;
    let (s, _) = t.call("PUT", "/api/modules/tmdb", Some(body), Some(&owner)).await;
    assert_eq!(s, StatusCode::OK);
    let (_, modules) = t.call("GET", "/api/modules", None, Some(&owner)).await;
    let text = modules.to_string();
    assert!(!text.contains("super-secret"));
    let tmdb = modules.as_array().unwrap().iter().find(|m| m["id"] == "tmdb").unwrap();
    assert_eq!(tmdb["secrets_set"], json!(["api_key"]));
    assert_eq!(tmdb["source"], "registry");
    let engine = modules.as_array().unwrap().iter().find(|m| m["id"] == "custom-themes").unwrap();
    assert_eq!(engine["source"], "builtin");
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

#[tokio::test]
async fn editing_a_library() {
    let t = app().await;
    let owner = t.setup_owner().await;
    let media = tempfile::tempdir().unwrap();
    let dir = |name: &str| {
        let p = media.path().join(name);
        std::fs::create_dir_all(&p).unwrap();
        p.display().to_string()
    };
    let (movies, movies_4k, tv, anime) = (dir("movies"), dir("movies-4k"), dir("tv"), dir("anime"));

    let (s, body) = t
        .call(
            "POST",
            "/api/libraries",
            Some(json!({"name": "Movies", "kind": "movies", "paths": [movies]})),
            Some(&owner),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    let id = body["id"].as_str().unwrap().to_string();
    let (s, body) =
        t.call("POST", "/api/libraries", Some(json!({"name": "TV", "kind": "tv", "paths": [tv]})), Some(&owner)).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    let uri = format!("/api/libraries/{id}");

    // Rename only: same id, folders kept, no rescan.
    let (s, body) = t.call("PUT", &uri, Some(json!({"name": "Films"})), Some(&owner)).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["rescan"], false);
    let (_, libs) = t.call("GET", "/api/libraries", None, Some(&owner)).await;
    let lib = libs.as_array().unwrap().iter().find(|l| l["id"] == id.as_str()).unwrap();
    assert_eq!(lib["name"], "Films");
    assert_eq!(lib["paths"], json!([movies]));

    // Add a folder (with a trailing slash and a duplicate): cleaned up and rescanned.
    let (s, body) = t
        .call(
            "PUT",
            &uri,
            Some(json!({"name": "Films", "kind": "movies", "paths": [movies, format!("{movies_4k}/"), movies_4k]})),
            Some(&owner),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["rescan"], true);
    let (_, libs) = t.call("GET", "/api/libraries", None, Some(&owner)).await;
    let lib = libs.as_array().unwrap().iter().find(|l| l["id"] == id.as_str()).unwrap();
    assert_eq!(lib["paths"], json!([movies, movies_4k]));

    // Remove one again.
    let (s, body) = t.call("PUT", &uri, Some(json!({"paths": [movies_4k]})), Some(&owner)).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    let (_, libs) = t.call("GET", "/api/libraries", None, Some(&owner)).await;
    let lib = libs.as_array().unwrap().iter().find(|l| l["id"] == id.as_str()).unwrap();
    assert_eq!(lib["paths"], json!([movies_4k]));

    let rejected: [(Value, StatusCode, &str); 7] = [
        (json!({"name": "tv"}), StatusCode::CONFLICT, "already a library called"),
        (json!({"name": "  "}), StatusCode::BAD_REQUEST, "name"),
        (json!({"kind": "tv"}), StatusCode::BAD_REQUEST, "can't be changed"),
        (json!({"paths": []}), StatusCode::BAD_REQUEST, "at least one folder"),
        (json!({"paths": ["/definitely/not/here"]}), StatusCode::BAD_REQUEST, "folder not found"),
        (json!({"paths": [media.path(), movies]}), StatusCode::BAD_REQUEST, "overlap"),
        (json!({"paths": [movies, anime, tv]}), StatusCode::CONFLICT, "the library TV already uses"),
    ];
    for (edit, status, message) in rejected {
        let (s, body) = t.call("PUT", &uri, Some(edit.clone()), Some(&owner)).await;
        assert_eq!(s, status, "{edit} -> {body}");
        assert!(body["error"].as_str().unwrap().contains(message), "{edit} -> {body}");
    }
    // Nothing changed after the rejected edits.
    let (_, libs) = t.call("GET", "/api/libraries", None, Some(&owner)).await;
    let lib = libs.as_array().unwrap().iter().find(|l| l["id"] == id.as_str()).unwrap();
    assert_eq!((&lib["name"], &lib["paths"]), (&json!("Films"), &json!([movies_4k])));

    // A library can't be created on another library's folder either.
    let (s, _) = t
        .call("POST", "/api/libraries", Some(json!({"name": "Shows", "kind": "tv", "paths": [tv]})), Some(&owner))
        .await;
    assert_eq!(s, StatusCode::CONFLICT);

    let (s, _) = t.call("PUT", "/api/libraries/nope", Some(json!({"name": "X"})), Some(&owner)).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

/// A show with two episodes laid out like the 86 Eighty Six release: English
/// then Japanese audio, three English subtitle tracks told apart by title.
/// Episode 2 has its tracks in a different order. Returns the two file ids.
async fn seed_show(db: &sqlx::SqlitePool) -> (String, String) {
    let q = |sql: &'static str| sqlx::query(sql);
    q("INSERT INTO libraries (id, name, kind, paths, created_at) VALUES ('lib', 'Anime', 'tv', '[]', 0)")
        .execute(db)
        .await
        .unwrap();
    for (id, kind, parent, ep) in [
        ("show", "series", None, None),
        ("s1", "season", Some("show"), None),
        ("e1", "episode", Some("s1"), Some(1)),
        ("e2", "episode", Some("s1"), Some(2)),
    ] {
        sqlx::query(
            "INSERT INTO items (id, library_id, kind, parent_id, series_id, match_key, title, sort_title, season_number,
                                episode_number, added_at, updated_at)
             VALUES (?, 'lib', ?, ?, ?, ?, ?, ?, 1, ?, 0, 0)",
        )
        .bind(id)
        .bind(kind)
        .bind(parent)
        .bind(if kind == "series" { None } else { Some("show") })
        .bind(id)
        .bind(id)
        .bind(id)
        .bind(ep)
        .execute(db)
        .await
        .unwrap();
    }
    let audio = |i: u32, lang: &str, title: Option<&str>| json!({"index": i, "codec": "aac", "channels": 2, "language": lang, "title": title, "is_default": i == 1, "bitrate": null});
    let sub = |i: u32, title: &str| json!({"index": i, "codec": "ass", "language": "eng", "title": title, "is_default": false, "is_forced": false, "is_text": true});
    let video = json!({"index": 0, "codec": "h264", "profile": "High", "width": 1920, "height": 1080, "bit_depth": 8,
                       "pix_fmt": "yuv420p", "hdr": "sdr", "dv_profile": null, "dv_compat_id": null, "bitrate": null,
                       "frame_rate": 23.976});
    let e1 = json!({"container": "matroska", "duration_ms": 1_420_000, "bitrate": null, "video": video,
        "audio": [audio(1, "eng", Some("English (2.0 AAC)")), audio(2, "jpn", None)],
        "subtitles": [sub(3, "Signs & Songs"), sub(4, "Kantai"), sub(5, "English subs")]});
    let e2 = json!({"container": "matroska", "duration_ms": 1_420_000, "bitrate": null, "video": video,
        "audio": [audio(1, "jpn", None), audio(2, "eng", Some("English (2.0 AAC)"))],
        "subtitles": [sub(3, "English subs"), sub(4, "Signs & Songs")]});
    for (file, item, info) in [("f1", "e1", e1), ("f2", "e2", e2)] {
        sqlx::query(
            "INSERT INTO media_files (id, item_id, library_id, path, size, mtime, container, duration_ms, info, scanned_at)
             VALUES (?, ?, 'lib', ?, 1, 0, 'matroska', 1420000, ?, 0)",
        )
        .bind(file)
        .bind(item)
        .bind(format!("/media/{file}.mkv"))
        .bind(info.to_string())
        .execute(db)
        .await
        .unwrap();
    }
    ("f1".into(), "f2".into())
}

#[tokio::test]
async fn track_selection_goes_through_the_installed_module() {
    let t = app().await;
    let owner = t.setup_owner().await;
    let (f1, f2) = seed_show(&t.db).await;
    // A client that plays the mkv as is, so no transcode is started.
    let start = json!({"caps": {"containers": ["mkv"], "video_codecs": ["h264"], "audio_codecs": ["aac"]}});
    let remember = |audio: u32, subtitle: Value| json!({"audio_index": audio, "subtitle_index": subtitle});

    // Without a track selection module nothing is picked and nothing is saved.
    let (s, body) = t.call("POST", &format!("/api/playback/{f2}"), Some(start.clone()), Some(&owner)).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert!(body.get("auto_tracks").is_none(), "{body}");
    assert_eq!(body["plan"]["audio_index"], 1);
    let (s, _) = t.call("PUT", &format!("/api/playback/{f1}/tracks"), Some(remember(2, json!(5))), Some(&owner)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = t.call("DELETE", "/api/track-choices", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    // The test module remembers stream indexes as they are.
    install_fixture(&t.state, "auto-tracks", json!({"storage": true})).await;
    let (_, body) = t.call("POST", &format!("/api/playback/{f2}"), Some(start.clone()), Some(&owner)).await;
    assert!(body.get("auto_tracks").is_none(), "nothing picked yet: {body}");

    let (s, _) = t.call("PUT", &format!("/api/playback/{f1}/tracks"), Some(remember(2, json!(5))), Some(&owner)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) =
        t.call("PUT", &format!("/api/playback/{f1}/tracks"), Some(remember(9, Value::Null)), Some(&owner)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "a track the file doesn't have");

    // Episode 2 has no stream 5: the module's answer is held to what the file has.
    let (_, body) = t.call("POST", &format!("/api/playback/{f2}"), Some(start.clone()), Some(&owner)).await;
    assert_eq!(body["auto_tracks"]["from"], "series", "{body}");
    assert_eq!(body["auto_tracks"]["audio_index"], 2);
    assert!(!body["auto_tracks"].as_object().unwrap().contains_key("subtitle_index"), "{body}");
    assert_eq!(body["plan"]["audio_index"], 2);
    assert_eq!(body["subtitles"][0]["is_hearing_impaired"], false);

    t.call("PUT", &format!("/api/playback/{f1}/tracks"), Some(remember(1, json!(4))), Some(&owner)).await;
    let (_, body) = t.call("POST", &format!("/api/playback/{f2}"), Some(start.clone()), Some(&owner)).await;
    assert_eq!(body["auto_tracks"]["audio_index"], 1);
    assert_eq!(body["auto_tracks"]["subtitle_index"], 4);

    // Asking for a specific audio track wins over the remembered one.
    let mut explicit = start.clone();
    explicit["audio_index"] = json!(2);
    let (_, body) = t.call("POST", &format!("/api/playback/{f2}"), Some(explicit), Some(&owner)).await;
    assert_eq!(body["plan"]["audio_index"], 2);
    assert!(body.get("auto_tracks").is_none());

    // Subtitles off is passed on as off (null), not as "no opinion" (missing).
    t.call("PUT", &format!("/api/playback/{f1}/tracks"), Some(remember(2, Value::Null)), Some(&owner)).await;
    let (_, body) = t.call("POST", &format!("/api/playback/{f2}"), Some(start.clone()), Some(&owner)).await;
    assert!(body["auto_tracks"]["subtitle_index"].is_null());
    assert!(body["auto_tracks"].as_object().unwrap().contains_key("subtitle_index"));

    // Other viewers have their own choices.
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
    let (_, body) = t.call("POST", &format!("/api/playback/{f2}"), Some(start.clone()), Some(&viewer)).await;
    assert!(body.get("auto_tracks").is_none());

    // The module's settings reach it: with "per season" on, the season's choice is used.
    let per_season = json!({"settings": {"per_season": "true"}});
    let (s, _) = t.call("PUT", "/api/modules/auto-tracks", Some(per_season), Some(&owner)).await;
    assert_eq!(s, StatusCode::OK);
    let (_, body) = t.call("POST", &format!("/api/playback/{f2}"), Some(start.clone()), Some(&owner)).await;
    assert_eq!(body["auto_tracks"]["from"], "season", "{body}");

    // Turning the module off stops picking (choices are kept for later).
    let (s, _) = t.call("PUT", "/api/modules/auto-tracks", Some(json!({"enabled": false})), Some(&owner)).await;
    assert_eq!(s, StatusCode::OK);
    let (_, body) = t.call("POST", &format!("/api/playback/{f2}"), Some(start.clone()), Some(&owner)).await;
    assert!(body.get("auto_tracks").is_none());
    let (s, _) = t
        .call(
            "PUT",
            "/api/modules/auto-tracks",
            Some(json!({"enabled": true, "settings": {"per_season": "maybe"}})),
            Some(&owner),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Forgetting clears them.
    t.call("PUT", "/api/modules/auto-tracks", Some(json!({"enabled": true})), Some(&owner)).await;
    let (_, body) = t.call("POST", &format!("/api/playback/{f2}"), Some(start.clone()), Some(&owner)).await;
    assert!(body.get("auto_tracks").is_some());
    let (s, _) = t.call("DELETE", "/api/track-choices", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, body) = t.call("POST", &format!("/api/playback/{f2}"), Some(start), Some(&owner)).await;
    assert!(body.get("auto_tracks").is_none());
}

#[tokio::test]
async fn uninstalling_a_module_keeps_its_data_unless_asked() {
    let t = app().await;
    let owner = t.setup_owner().await;
    let (f1, f2) = seed_show(&t.db).await;
    let start = json!({"caps": {"containers": ["mkv"], "video_codecs": ["h264"], "audio_codecs": ["aac"]}});
    let picked = || async {
        let (_, body) = t.call("POST", &format!("/api/playback/{f2}"), Some(start.clone()), Some(&owner)).await;
        body.get("auto_tracks").is_some()
    };

    install_fixture(&t.state, "auto-tracks", json!({"storage": true})).await;
    let choice = json!({"audio_index": 2, "subtitle_index": 4});
    t.call("PUT", &format!("/api/playback/{f1}/tracks"), Some(choice), Some(&owner)).await;
    assert!(picked().await);

    let (s, _) = t.call("DELETE", "/api/registry/auto-tracks", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(!picked().await, "no module, no picks");
    assert!(!t.state.config.modules_dir().join("auto-tracks").exists());
    let (_, modules) = t.call("GET", "/api/modules", None, Some(&owner)).await;
    assert!(!modules.as_array().unwrap().iter().any(|m| m["id"] == "auto-tracks"));
    let (s, _) = t.call("DELETE", "/api/registry/auto-tracks", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    install_fixture(&t.state, "auto-tracks", json!({"storage": true})).await;
    assert!(picked().await, "installing it again finds the choices it left");

    let (s, _) = t.call("DELETE", "/api/registry/auto-tracks?purge=true", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    install_fixture(&t.state, "auto-tracks", json!({"storage": true})).await;
    assert!(!picked().await, "purged");

    // A viewer's stored data goes with the viewer.
    t.call("PUT", &format!("/api/playback/{f1}/tracks"), Some(json!({"audio_index": 2})), Some(&owner)).await;
    let rows = || sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM module_data").fetch_one(&t.db);
    assert!(rows().await.unwrap() > 0);
    sqlx::query("DELETE FROM users").execute(&t.db).await.unwrap();
    assert_eq!(rows().await.unwrap(), 0);
}

#[tokio::test]
async fn modules_run_in_a_sandbox() {
    let t = app().await;
    t.setup_owner().await;
    let user: String = sqlx::query_scalar("SELECT id FROM users").fetch_one(&t.db).await.unwrap();
    install_fixture(&t.state, "boxed", json!({"storage": true})).await;
    let module = t.state.modules.find(&t.state, "boxed").await.unwrap();
    let call = |op: &'static str, args: Value| {
        let (state, module) = (t.state.clone(), module.clone());
        async move { state.modules.call(&state, &module, op, args).await }
    };
    let host = |request: Value| {
        let call = call("test.host", request);
        async move { call.await.unwrap() }
    };

    // Runaway modules are stopped, and the next call starts clean.
    let e = call("test.spin", Value::Null).await.unwrap_err().to_string();
    assert!(e.contains("too long"), "{e}");
    assert!(call("test.grow", Value::Null).await.is_err(), "asked for more memory than a call may have");
    assert!(call("test.panic", Value::Null).await.is_err());
    assert!(call("nope", Value::Null).await.unwrap_err().to_string().contains("unknown operation"));
    assert_eq!(call("metadata.ready", Value::Null).await.unwrap()["ready"], false);

    // Storage is the module's own, per viewer or shared.
    assert!(host(json!({"fn": "kv_get", "key": "a"})).await["ok"].is_null());
    assert!(host(json!({"fn": "kv_set", "key": "a", "value": "shared"})).await.get("error").is_none());
    assert!(host(json!({"fn": "kv_set", "key": "a", "value": "mine", "user": user})).await.get("error").is_none());
    assert!(host(json!({"fn": "kv_set", "key": "a", "value": "again", "user": user})).await.get("error").is_none());
    assert_eq!(host(json!({"fn": "kv_get", "key": "a"})).await["ok"], "shared");
    assert_eq!(host(json!({"fn": "kv_get", "key": "a", "user": user})).await["ok"], "again");
    assert!(host(json!({"fn": "kv_set", "key": "a", "value": "x", "user": "nobody"})).await["error"].is_string());
    host(json!({"fn": "kv_set", "key": "a_b", "value": "1"})).await;
    host(json!({"fn": "kv_set", "key": "axb", "value": "2"})).await;
    host(json!({"fn": "kv_clear", "prefix": "a_"})).await;
    assert!(host(json!({"fn": "kv_get", "key": "a_b"})).await["ok"].is_null());
    assert_eq!(host(json!({"fn": "kv_get", "key": "axb"})).await["ok"], "2", "_ is not a wildcard");
    host(json!({"fn": "kv_delete", "key": "a"})).await;
    assert!(host(json!({"fn": "kv_get", "key": "a"})).await["ok"].is_null());
    assert_eq!(host(json!({"fn": "kv_get", "key": "a", "user": user})).await["ok"], "again");
    // Another module can't see it.
    install_fixture(&t.state, "other", json!({"storage": true})).await;
    let other = t.state.modules.find(&t.state, "other").await.unwrap();
    let args = json!({"fn": "kv_get", "key": "a", "user": user});
    assert!(t.state.modules.call(&t.state, &other, "test.host", args).await.unwrap()["ok"].is_null());

    // Everything else needs a permission this module doesn't have.
    let e = host(json!({"fn": "http", "method": "GET", "url": "https://example.com/"})).await;
    assert!(e["error"].as_str().unwrap().contains("no permission to reach"), "{e}");
    let e = host(json!({"fn": "activity_record", "event": "x", "payload": {}})).await;
    assert!(e["error"].as_str().unwrap().contains("no permission"), "{e}");
    assert!(host(json!({"fn": "format_disk"})).await["error"].is_string());
    assert!(host(json!({"fn": "log", "level": "info", "message": "hello"})).await.get("error").is_none());

    install_fixture(&t.state, "plain", json!({})).await;
    let plain = t.state.modules.find(&t.state, "plain").await.unwrap();
    let e = t.state.modules.call(&t.state, &plain, "test.host", json!({"fn": "kv_get", "key": "a"})).await.unwrap();
    assert!(e["error"].as_str().unwrap().contains("no permission for storage"), "{e}");
}

#[tokio::test]
async fn metadata_and_events_come_from_installed_modules() {
    let t = app().await;
    let owner = t.setup_owner().await;
    seed_show(&t.db).await;

    // No provider: the scan step is a no-op.
    crate::scanner::fetch_metadata(&t.state, "lib").await.unwrap();
    let status =
        || sqlx::query_scalar::<_, String>("SELECT metadata_status FROM items WHERE id = 'show'").fetch_one(&t.db);
    assert_eq!(status().await.unwrap(), "pending");

    // Installed but without a key it says it isn't ready.
    install_fixture(&t.state, "testdb", json!({"activity": true})).await;
    crate::scanner::fetch_metadata(&t.state, "lib").await.unwrap();
    assert_eq!(status().await.unwrap(), "pending");

    let settings = json!({"settings": {"api_key": "k", "language": "de-DE"}});
    t.call("PUT", "/api/modules/testdb", Some(settings), Some(&owner)).await;
    crate::scanner::fetch_metadata(&t.state, "lib").await.unwrap();
    let (title, overview, ids): (String, String, String) =
        sqlx::query_as("SELECT title, overview, provider_ids FROM items WHERE id = 'show'")
            .fetch_one(&t.db)
            .await
            .unwrap();
    assert_eq!(title, "Details for 42");
    assert_eq!(overview, "language de-DE", "settings reach the module");
    assert_eq!(serde_json::from_str::<Value>(&ids).unwrap()["testdb"], "42");
    // Episodes are looked up with their series' id from the same provider.
    let episode: String = sqlx::query_scalar("SELECT title FROM items WHERE id = 'e2'").fetch_one(&t.db).await.unwrap();
    assert_eq!(episode, "42 S1E2");

    // A provider that rejects its key stops the scan step with an error.
    sqlx::query("UPDATE items SET metadata_status = 'pending' WHERE id = 'show'").execute(&t.db).await.unwrap();
    t.call("PUT", "/api/modules/testdb", Some(json!({"settings": {"api_key": "rejected"}})), Some(&owner)).await;
    assert!(crate::scanner::fetch_metadata(&t.state, "lib").await.is_err());

    // The same module subscribes to events and writes the activity log.
    crate::modules::events::spawn(t.state.clone());
    tokio::task::yield_now().await;
    let user: String = sqlx::query_scalar("SELECT id FROM users").fetch_one(&t.db).await.unwrap();
    t.state.emit(kinora_plugin_api::ServerEvent::PlaybackStarted {
        user_id: user,
        item_id: "e1".into(),
        method: "direct_play".into(),
    });
    let mut log = Value::Null;
    for _ in 0..100 {
        (_, log) = t.call("GET", "/api/system/activity", None, Some(&owner)).await;
        if log.as_array().is_some_and(|a| !a.is_empty()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(log[0]["event"], "playback_started", "{log}");
    assert_eq!(log[0]["username"], "owner");
}

#[tokio::test]
async fn modules_reach_only_the_hosts_they_were_given() {
    // A stand-in for a metadata site.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = hits.clone();
    let site = axum::Router::new().route(
        "/search",
        axum::routing::get(move || {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async { axum::Json(json!({"provider_id": "7", "title": "From the web", "genres": []})) }
        }),
    );
    tokio::spawn(async move { axum::serve(listener, site).await.unwrap() });

    let t = app().await;
    let owner = t.setup_owner().await;
    install_fixture(&t.state, "web", json!({"http": ["127.0.0.1"]})).await;
    let settings = json!({"settings": {"api_key": "k", "base_url": format!("http://127.0.0.1:{port}")}});
    t.call("PUT", "/api/modules/web", Some(settings), Some(&owner)).await;
    let module = t.state.modules.find(&t.state, "web").await.unwrap();
    let search = json!({"kind": "movie", "title": "Anything", "year": null});
    let hit = t.state.modules.call(&t.state, &module, "metadata.search", search.clone()).await.unwrap();
    assert_eq!(hit["title"], "From the web");
    // The module asked for the answer to be cached, so the site is asked once.
    t.state.modules.call(&t.state, &module, "metadata.search", search).await.unwrap();
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);

    let elsewhere = json!({"fn": "http", "method": "GET", "url": "http://localhost:1/"});
    let e = t.state.modules.call(&t.state, &module, "test.host", elsewhere).await.unwrap();
    assert!(e["error"].as_str().unwrap().contains("no permission to reach"), "{e}");
    let delete = json!({"fn": "http", "method": "DELETE", "url": format!("http://127.0.0.1:{port}/search")});
    let e = t.state.modules.call(&t.state, &module, "test.host", delete).await.unwrap();
    assert!(e["error"].as_str().unwrap().contains("not allowed"), "{e}");
}

const SUNSET: &str = r##"{"id":"sunset","name":"Sunset","scheme":"dark","tokens":{"primary":"#ff7a59"}}"##;

/// A registry with one of each kind of package.
async fn registry() -> TestRegistry {
    let mut r = TestRegistry::start().await;
    r.add_module("tmdb", "1.0.0");
    let package =
        |id: &str, kind: &str| json!({"id": id, "kind": kind, "name": id, "description": "d", "version": "1.0.0"});
    r.add(package("sunset", "theme"), &[("sunset.json", SUNSET.as_bytes())]);
    let skin: Vec<(&str, &[u8])> = vec![
        ("skin.json", br#"{"id":"glow","name":"Glow"}"#),
        ("skin.html", br#"<button data-kn-action="play">Play</button>"#),
        ("skin.css", b".x { color: red }"),
    ];
    r.add(package("glow", "player-skin"), &skin);
    let blank = crate::modules::app_skins::samples().into_iter().find(|(n, _)| *n == "blank").unwrap().1;
    let files: Vec<(&str, Vec<u8>)> = blank
        .iter()
        .map(|(path, text)| (*path, text.replace("\"id\": \"blank\"", "\"id\": \"plain\"").into_bytes()))
        .collect();
    let files: Vec<(&str, &[u8])> = files.iter().map(|(p, d)| (*p, d.as_slice())).collect();
    r.add(package("plain", "app-skin"), &files);
    r.publish(None);
    r
}

#[tokio::test]
async fn packages_install_and_uninstall_from_a_registry() {
    let mut r = registry().await;
    let t = app_with_registry(&r.url, vec![]).await;
    let owner = t.setup_owner().await;
    let find =
        |list: &Value, id: &str| list["packages"].as_array().unwrap().iter().find(|p| p["id"] == id).cloned().unwrap();

    let (s, list) = t.call("GET", "/api/registry", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    assert!(list["error"].is_null(), "{list}");
    assert_eq!(list["signed"], false);
    assert_eq!(list["packages"].as_array().unwrap().len(), 4);
    assert_eq!(list["packages"][0]["id"], "tmdb", "recommended packages come first");
    let tmdb = find(&list, "tmdb");
    assert_eq!(tmdb["version"], "1.0.0");
    assert!(tmdb["installed_version"].is_null());
    assert_eq!(tmdb["permissions"], json!({"storage": true}));
    assert_eq!(tmdb["provides"][0], "metadata_provider");
    // Nothing is there until it is installed.
    let (_, modules) = t.call("GET", "/api/modules", None, Some(&owner)).await;
    assert_eq!(modules.as_array().unwrap().len(), 3, "only the built-in engines: {modules}");

    for id in ["tmdb", "sunset", "glow", "plain"] {
        let (s, body) = t.call("POST", &format!("/api/registry/{id}/install"), None, Some(&owner)).await;
        assert_eq!(s, StatusCode::OK, "{id}: {body}");
        assert_eq!(body["version"], "1.0.0");
    }
    let (s, _) = t.call("POST", "/api/registry/nope/install", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let config = &t.state.config;
    assert!(config.modules_dir().join("tmdb/module.wasm").is_file());
    assert!(config.themes_dir().join("sunset.json").is_file());
    assert!(config.skins_dir().join("glow/skin.html").is_file());
    assert!(config.app_skins_dir().join("plain/skin.json").is_file());
    assert!(std::fs::read_dir(config.staging_dir()).map(|d| d.count()).unwrap_or(0) == 0, "staging is cleaned up");

    // Each shows up where its kind lives.
    let (_, modules) = t.call("GET", "/api/modules", None, Some(&owner)).await;
    let module = modules.as_array().unwrap().iter().find(|m| m["id"] == "tmdb").unwrap();
    assert_eq!((&module["source"], &module["enabled"]), (&json!("registry"), &json!(true)));
    let (_, themes) = t.call("GET", "/api/themes", None, None).await;
    assert!(themes["themes"].as_array().unwrap().iter().any(|t| t["id"] == "sunset"));
    let (_, skins) = t.call("GET", "/api/player-skins", None, Some(&owner)).await;
    assert_eq!(skins[0]["id"], "glow");
    let (_, app_skins) = t.call("GET", "/api/app-skins", None, Some(&owner)).await;
    assert_eq!(app_skins["skins"][0]["id"], "plain");
    assert_eq!(app_skins["skins"][0]["usable"], true);
    let (_, list) = t.call("GET", "/api/registry", None, Some(&owner)).await;
    assert_eq!(find(&list, "tmdb")["installed_version"], "1.0.0");
    assert_eq!(find(&list, "tmdb")["update_available"], false);

    // A new version in the registry is offered as an update and installs over the old one.
    r.add_module("tmdb", "1.1.0");
    r.publish(None);
    let (_, list) = t.call("GET", "/api/registry", None, Some(&owner)).await;
    assert_eq!(find(&list, "tmdb")["update_available"], false, "the index is cached for a while");
    let (_, list) = t.call("GET", "/api/registry?refresh=true", None, Some(&owner)).await;
    assert_eq!(find(&list, "tmdb")["update_available"], true);
    t.call("PUT", "/api/modules/tmdb", Some(json!({"settings": {"api_key": "kept"}})), Some(&owner)).await;
    let (s, body) = t.call("POST", "/api/registry/tmdb/install", None, Some(&owner)).await;
    assert_eq!((s, &body["version"]), (StatusCode::OK, &json!("1.1.0")));
    let (_, modules) = t.call("GET", "/api/modules", None, Some(&owner)).await;
    let module = modules.as_array().unwrap().iter().find(|m| m["id"] == "tmdb").unwrap();
    assert_eq!(module["version"], "1.1.0");
    assert_eq!(module["secrets_set"], json!(["api_key"]), "settings survive an update");

    // Uninstalling removes the files; an app skin in use stops being used.
    t.call("PUT", "/api/modules/app-skins", Some(json!({"enabled": true})), Some(&owner)).await;
    let (s, _) = t.call("PUT", "/api/app-skin", Some(json!({"skin": "plain"})), Some(&owner)).await;
    assert_eq!(s, StatusCode::OK);
    for id in ["tmdb", "sunset", "glow", "plain"] {
        let (s, _) = t.call("DELETE", &format!("/api/registry/{id}"), None, Some(&owner)).await;
        assert_eq!(s, StatusCode::NO_CONTENT, "{id}");
    }
    assert!(!config.modules_dir().join("tmdb").exists());
    assert!(!config.themes_dir().join("sunset.json").exists());
    assert!(!config.skins_dir().join("glow").exists());
    assert!(!config.app_skins_dir().join("plain").exists());
    let (s, _) = t.call("GET", "/api/app-skin/active", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, list) = t.call("GET", "/api/registry", None, Some(&owner)).await;
    assert!(list["packages"].as_array().unwrap().iter().all(|p| p["installed_version"].is_null()));

    // Viewers can't browse or install.
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
    for (method, uri) in
        [("GET", "/api/registry"), ("POST", "/api/registry/sunset/install"), ("DELETE", "/api/registry/sunset")]
    {
        let (s, _) = t.call(method, uri, None, Some(&viewer)).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{method} {uri}");
    }
}

#[tokio::test]
async fn tampered_and_conflicting_packages_are_refused() {
    let r = registry().await;
    let t = app_with_registry(&r.url, vec![]).await;
    let owner = t.setup_owner().await;
    let config = &t.state.config;

    // A file changed after the registry was built: its hash no longer matches.
    let theme = r.dir.path().join("packages/sunset/1.0.0/files/sunset.json");
    std::fs::write(&theme, SUNSET.replace("#ff7a59", "#000000")).unwrap();
    let (s, body) = t.call("POST", "/api/registry/sunset/install", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("checksum"), "{body}");
    assert!(!config.themes_dir().join("sunset.json").exists());
    std::fs::write(&theme, SUNSET).unwrap();

    // So does a changed manifest.
    let manifest = r.dir.path().join("packages/glow/1.0.0/package.json");
    let original = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, original.replace("\"glow\"", "\"glow\" ")).unwrap();
    let (s, body) = t.call("POST", "/api/registry/glow/install", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("does not match the registry's index"), "{body}");
    std::fs::write(&manifest, original).unwrap();

    // The admin's own files are never overwritten.
    std::fs::write(config.themes_dir().join("sunset.json"), SUNSET.replace("Sunset", "Mine")).unwrap();
    std::fs::create_dir_all(config.skins_dir().join("glow")).unwrap();
    std::fs::create_dir_all(config.app_skins_dir().join("plain")).unwrap();
    for id in ["sunset", "glow", "plain"] {
        let (s, body) = t.call("POST", &format!("/api/registry/{id}/install"), None, Some(&owner)).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{id}");
        assert!(body["error"].as_str().unwrap().contains("not installed from the registry"), "{body}");
    }
    assert!(std::fs::read_to_string(config.themes_dir().join("sunset.json")).unwrap().contains("Mine"));

    // A module that isn't a module.
    let mut bad = TestRegistry::start().await;
    let mut manifest =
        serde_json::to_value(crate::modules::test_support::fixture_manifest("fake", "1.0.0", json!({}))).unwrap();
    manifest.as_object_mut().unwrap().remove("files");
    bad.add(manifest, &[("module.wasm", b"this is not WebAssembly")]);
    bad.publish(None);
    let t = app_with_registry(&bad.url, vec![]).await;
    let owner = t.setup_owner().await;
    let (s, body) = t.call("POST", "/api/registry/fake/install", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("WebAssembly"), "{body}");
    assert!(!t.state.config.modules_dir().join("fake").exists());

    // No registry at all: the page still loads and says why.
    let t = app().await;
    let owner = t.setup_owner().await;
    install_fixture(&t.state, "kept", json!({})).await;
    let (s, list) = t.call("GET", "/api/registry", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::OK);
    assert!(list["error"].as_str().unwrap().contains("could not reach the registry"), "{list}");
    assert_eq!(list["packages"][0]["id"], "kept", "what is installed can still be managed");
    assert!(list["packages"][0]["version"].is_null());
}

#[tokio::test]
async fn a_pinned_key_requires_a_signed_index() {
    let r = registry().await;
    let key = r.publish(Some([7; 32])).unwrap();
    let other_key = crate::modules::test_support::base64(
        &ed25519_dalek::SigningKey::from_bytes(&[8; 32]).verifying_key().to_bytes(),
    );

    let t = app_with_registry(&r.url, vec![other_key.clone(), key.clone()]).await;
    let owner = t.setup_owner().await;
    let (_, list) = t.call("GET", "/api/registry", None, Some(&owner)).await;
    assert_eq!(list["signed"], true, "{list}");
    assert_eq!(list["packages"].as_array().unwrap().len(), 4);
    let (s, _) = t.call("POST", "/api/registry/sunset/install", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::OK);

    // Signed by a key this server doesn't trust.
    let t = app_with_registry(&r.url, vec![other_key]).await;
    let owner = t.setup_owner().await;
    let (_, list) = t.call("GET", "/api/registry", None, Some(&owner)).await;
    assert!(list["error"].as_str().unwrap().contains("doesn't trust"), "{list}");
    let (s, _) = t.call("POST", "/api/registry/sunset/install", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // The index changed after it was signed.
    let index = r.dir.path().join("index.json");
    let text = std::fs::read_to_string(&index).unwrap();
    std::fs::write(&index, text.replace("\"sunset\"", "\"sunrise\"")).unwrap();
    let t = app_with_registry(&r.url, vec![key.clone()]).await;
    let owner = t.setup_owner().await;
    let (_, list) = t.call("GET", "/api/registry", None, Some(&owner)).await;
    assert!(list["error"].as_str().unwrap().contains("signature does not match"), "{list}");

    // Not signed at all.
    r.publish(None);
    let t = app_with_registry(&r.url, vec![key]).await;
    let owner = t.setup_owner().await;
    let (_, list) = t.call("GET", "/api/registry", None, Some(&owner)).await;
    assert!(list["error"].as_str().unwrap().contains("signature is missing"), "{list}");
}

#[tokio::test]
async fn an_upgraded_server_gets_back_what_used_to_be_built_in() {
    let mut r = TestRegistry::start().await;
    r.add_module("tmdb", "1.0.0");
    r.add_module("auto-tracks", "1.0.0");
    r.add_module("activity-log", "1.0.0");
    let theme = |id: &str| SUNSET.replace("sunset", id);
    for id in ["midnight", "oled"] {
        let package = json!({"id": id, "kind": "theme", "name": id, "description": "d", "version": "1.0.0"});
        r.add(package, &[(&format!("{id}.json"), theme(id).as_bytes())]);
    }
    r.publish(None);
    let installed = |t: &TestApp| {
        let db = t.db.clone();
        async move { sqlx::query_scalar::<_, String>("SELECT id FROM packages ORDER BY id").fetch_all(&db).await.unwrap() }
    };

    // A brand new server starts with nothing installed, and stays that way.
    let t = app_with_registry(&r.url, vec![]).await;
    crate::modules::registry::adopt(&t.state).await;
    t.setup_owner().await;
    crate::modules::registry::adopt(&t.state).await;
    assert!(installed(&t).await.is_empty());

    // A server that already has users had these built in. One was switched off; one isn't published.
    let t = app_with_registry(&r.url, vec![]).await;
    t.setup_owner().await;
    sqlx::query("INSERT INTO modules (id, enabled) VALUES ('auto-tracks', 0)").execute(&t.db).await.unwrap();
    crate::modules::registry::adopt(&t.state).await;
    assert_eq!(installed(&t).await, ["activity-log", "midnight", "oled", "tmdb"]);

    // "ember" was missing, so the next start tries again; once it's all there, it stops.
    r.add(
        json!({"id": "ember", "kind": "theme", "name": "e", "description": "d", "version": "1.0.0"}),
        &[("ember.json", theme("ember").as_bytes())],
    );
    r.publish(None);
    t.state.registry.index(&t.state, true).await.unwrap();
    crate::modules::registry::adopt(&t.state).await;
    assert_eq!(installed(&t).await, ["activity-log", "ember", "midnight", "oled", "tmdb"]);
    let owner_removed = sqlx::query("DELETE FROM packages WHERE id = 'oled'").execute(&t.db).await.unwrap();
    assert_eq!(owner_removed.rows_affected(), 1);
    crate::modules::registry::adopt(&t.state).await;
    assert_eq!(installed(&t).await.len(), 4, "what the admin uninstalls later stays uninstalled");
}

#[tokio::test]
async fn app_skins_starter_options_and_active_bundle() {
    let t = app().await;
    let owner = t.setup_owner().await;

    // Off by default: nobody gets a skin.
    let (s, _) = t.call("GET", "/api/app-skin/active", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    t.call("PUT", "/api/modules/app-skins", Some(json!({"enabled": true})), Some(&owner)).await;

    let (s, body) = t.call("POST", "/api/app-skins/starters/marquee", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["id"], "marquee");
    let (_, body) = t.call("POST", "/api/app-skins/starters/marquee", None, Some(&owner)).await;
    assert_eq!(body["id"], "marquee-2", "a second copy gets a new id");
    let (s, _) = t.call("POST", "/api/app-skins/starters/nope", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let (_, list) = t.call("GET", "/api/app-skins", None, Some(&owner)).await;
    let skins = list["skins"].as_array().unwrap();
    assert_eq!(skins.len(), 2);
    assert!(skins.iter().all(|s| s["usable"] == true), "{list}");
    assert_eq!(list["starters"], json!(["blank", "marquee"]));

    // Options are checked, and can be changed without switching skins.
    let (s, _) = t.call("PUT", "/api/app-skins/marquee/options", Some(json!({"card-size": "xl"})), Some(&owner)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = t.call("PUT", "/api/app-skins/marquee/options", Some(json!({"card-size": "l"})), Some(&owner)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    let (s, _) = t.call("PUT", "/api/app-skin", Some(json!({"skin": "marquee"})), Some(&owner)).await;
    assert_eq!(s, StatusCode::OK);
    let (s, bundle) = t.call("GET", "/api/app-skin/active", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(bundle["id"], "marquee");
    assert_eq!(bundle["optionValues"]["card-size"], "l");
    assert!(bundle["templates"]["shell"].as_str().unwrap().contains("data-kn-slot=\"outlet\""));
    assert!(bundle["css"].as_str().unwrap().contains(".mq-"));

    // Viewers can't pick their own until the admin allows it.
    let (_, mine) = t.call("GET", "/api/me/app-skin", None, Some(&owner)).await;
    assert_eq!(mine["allowed"], false);
    assert_eq!(mine["server"], "marquee");

    let (s, _) = t.call("DELETE", "/api/app-skins/marquee-2", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, list) = t.call("GET", "/api/app-skins", None, Some(&owner)).await;
    assert_eq!(list["skins"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn dashboard_lists_direct_play_and_follows_heartbeats() {
    let t = app().await;
    let owner = t.setup_owner().await;
    let (f1, _) = seed_show(&t.db).await;
    // A client that plays the mkv as is: direct play, no transcoder involved.
    let start = json!({
        "caps": {"containers": ["mkv"], "video_codecs": ["h264"], "audio_codecs": ["aac"]},
        "client": {"name": "Kinora web", "device": "Chrome on macOS"},
        "start_ms": 60_000
    });
    let (s, body) = t.call("POST", &format!("/api/playback/{f1}"), Some(start), Some(&owner)).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["method"], "direct_play");
    let play_id = body["play_id"].as_str().unwrap().to_string();

    let (s, list) = t.call("GET", "/api/sessions", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::OK);
    let row = &list[0];
    assert_eq!(list.as_array().unwrap().len(), 1, "{list}");
    assert_eq!(row["id"], play_id.as_str());
    assert_eq!(row["method"], "direct_play");
    assert_eq!(row["item"]["kind"], "episode");
    assert_eq!(row["item"]["series_title"], "show");
    assert_eq!(row["item"]["season"], 1);
    assert_eq!(row["item"]["episode"], 1);
    assert_eq!(row["client"]["device"], "Chrome on macOS");
    assert_eq!(row["duration_ms"], 1_420_000);
    assert_eq!(row["position_ms"], 60_000);
    assert!(row["transcoder"].is_null());

    // A paused heartbeat with stats.
    let hb = json!({"position_ms": 120_000, "state": "paused",
                    "stats": {"dropped_frames": 3, "total_frames": 900, "recent_errors": ["oops"]}});
    let (s, _) = t.call("POST", &format!("/api/now-playing/{play_id}/heartbeat"), Some(hb), Some(&owner)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, list) = t.call("GET", "/api/sessions", None, Some(&owner)).await;
    assert_eq!(list[0]["state"], "paused");
    assert_eq!(list[0]["position_ms"], 120_000);
    assert_eq!(list[0]["bitrate_bps"], 0);
    assert_eq!(list[0]["client_stats"]["dropped_frames"], 3);
    assert_eq!(list[0]["client_stats"]["recent_errors"][0], "oops");

    // Clients without heartbeats still move the marker with their progress saves.
    let (s, _) = t
        .call(
            "POST",
            "/api/items/e1/progress",
            Some(json!({"position_ms": 200_000, "duration_ms": 1_420_000})),
            Some(&owner),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, list) = t.call("GET", "/api/sessions", None, Some(&owner)).await;
    assert_eq!(list[0]["position_ms"], 200_000);

    // Viewers can't see the dashboard or touch someone else's playback.
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
    let (s, _) = t.call("GET", "/api/sessions", None, Some(&viewer)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = t.call("POST", &format!("/api/now-playing/{play_id}/heartbeat"), Some(json!({})), Some(&viewer)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = t.call("DELETE", &format!("/api/now-playing/{play_id}"), None, Some(&viewer)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Closing the player removes it; later heartbeats get a 404.
    let (s, _) = t.call("DELETE", &format!("/api/now-playing/{play_id}"), None, Some(&owner)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, list) = t.call("GET", "/api/sessions", None, Some(&owner)).await;
    assert_eq!(list, json!([]));
    let (s, _) = t.call("POST", &format!("/api/now-playing/{play_id}/heartbeat"), Some(json!({})), Some(&owner)).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn stream_buffer_is_an_admin_setting_players_receive() {
    let t = app().await;
    let owner = t.setup_owner().await;
    let (f1, _) = seed_show(&t.db).await;
    let start = json!({"caps": {"containers": ["mkv"], "video_codecs": ["h264"], "audio_codecs": ["aac"]}});

    let (_, settings) = t.call("GET", "/api/system/settings", None, Some(&owner)).await;
    assert_eq!(settings["stream_buffer_seconds"], 30);
    let (_, body) = t.call("POST", &format!("/api/playback/{f1}"), Some(start.clone()), Some(&owner)).await;
    assert_eq!(body["buffer_seconds"], 30);

    let mut changed = settings.clone();
    changed["stream_buffer_seconds"] = json!(5);
    let (s, _) = t.call("PUT", "/api/system/settings", Some(changed.clone()), Some(&owner)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "too small");

    changed["stream_buffer_seconds"] = json!(120);
    let (s, body) = t.call("PUT", "/api/system/settings", Some(changed), Some(&owner)).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    let (_, settings) = t.call("GET", "/api/system/settings", None, Some(&owner)).await;
    assert_eq!(settings["stream_buffer_seconds"], 120);
    let (_, body) = t.call("POST", &format!("/api/playback/{f1}"), Some(start), Some(&owner)).await;
    assert_eq!(body["buffer_seconds"], 120);

    // Older clients that don't send the field keep the default.
    let mut old = settings.clone();
    old.as_object_mut().unwrap().remove("stream_buffer_seconds");
    let (s, body) = t.call("PUT", "/api/system/settings", Some(old), Some(&owner)).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["stream_buffer_seconds"], 30);
}
