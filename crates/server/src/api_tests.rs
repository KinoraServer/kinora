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
    db: sqlx::SqlitePool,
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
    TestApp { db: state.db.clone(), app: crate::router(state), _dir: dir }
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
async fn auto_tracks_remember_choices_per_show() {
    let t = app().await;
    let owner = t.setup_owner().await;
    let (f1, f2) = seed_show(&t.db).await;
    // A client that plays the mkv as is, so no transcode is started.
    let start = json!({"caps": {"containers": ["mkv"], "video_codecs": ["h264"], "audio_codecs": ["aac"]}});

    let (s, body) = t.call("POST", &format!("/api/playback/{f2}"), Some(start.clone()), Some(&owner)).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert!(body.get("auto_tracks").is_none(), "nothing picked yet: {body}");
    assert_eq!(body["plan"]["audio_index"], 1);

    // Episode 1: switch to Japanese audio and the full English subtitles.
    let (s, _) = t
        .call(
            "PUT",
            &format!("/api/playback/{f1}/tracks"),
            Some(json!({"audio_index": 2, "subtitle_index": 5})),
            Some(&owner),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = t
        .call(
            "PUT",
            &format!("/api/playback/{f1}/tracks"),
            Some(json!({"audio_index": 9, "subtitle_index": null})),
            Some(&owner),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Episode 2 starts with the same tracks, found by language and title.
    let (_, body) = t.call("POST", &format!("/api/playback/{f2}"), Some(start.clone()), Some(&owner)).await;
    assert_eq!(body["auto_tracks"]["from"], "series", "{body}");
    assert_eq!(body["auto_tracks"]["audio_index"], 1);
    assert_eq!(body["auto_tracks"]["subtitle_index"], 3);
    assert_eq!(body["plan"]["audio_index"], 1);
    assert_eq!(body["subtitles"][0]["is_hearing_impaired"], false);

    // Asking for a specific audio track wins over the remembered one.
    let mut explicit = start.clone();
    explicit["audio_index"] = json!(2);
    let (_, body) = t.call("POST", &format!("/api/playback/{f2}"), Some(explicit), Some(&owner)).await;
    assert_eq!(body["plan"]["audio_index"], 2);
    assert!(body.get("auto_tracks").is_none());

    // Subtitles off is remembered as off (null), not as "no opinion" (missing).
    t.call(
        "PUT",
        &format!("/api/playback/{f1}/tracks"),
        Some(json!({"audio_index": 2, "subtitle_index": null})),
        Some(&owner),
    )
    .await;
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
    let (s, _) = t.call("DELETE", "/api/track-choices", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, body) = t.call("POST", &format!("/api/playback/{f2}"), Some(start), Some(&owner)).await;
    assert!(body.get("auto_tracks").is_none());
}

#[tokio::test]
async fn auto_tracks_fall_back_to_the_library() {
    let t = app().await;
    let owner = t.setup_owner().await;
    let (f1, _) = seed_show(&t.db).await;
    // A second show in the same library, never watched.
    sqlx::query(
        "INSERT INTO items (id, library_id, kind, series_id, parent_id, match_key, title, sort_title, added_at, updated_at)
         VALUES ('other', 'lib', 'series', NULL, NULL, 'other', 'o', 'o', 0, 0),
                ('o1', 'lib', 'episode', 'other', 'other', 'o1', 'o1', 'o1', 0, 0)",
    )
    .execute(&t.db)
    .await
    .unwrap();
    let info: String =
        sqlx::query_scalar("SELECT info FROM media_files WHERE id = 'f1'").fetch_one(&t.db).await.unwrap();
    sqlx::query(
        "INSERT INTO media_files (id, item_id, library_id, path, size, mtime, info, scanned_at)
         VALUES ('fo', 'o1', 'lib', '/media/o1.mkv', 1, 0, ?, 0)",
    )
    .bind(info)
    .execute(&t.db)
    .await
    .unwrap();

    t.call(
        "PUT",
        &format!("/api/playback/{f1}/tracks"),
        Some(json!({"audio_index": 2, "subtitle_index": 5})),
        Some(&owner),
    )
    .await;
    let start = json!({"caps": {"containers": ["mkv"], "video_codecs": ["h264"], "audio_codecs": ["aac"]}});
    let (_, body) = t.call("POST", "/api/playback/fo", Some(start.clone()), Some(&owner)).await;
    assert_eq!(body["auto_tracks"]["from"], "library", "{body}");
    assert_eq!(body["plan"]["audio_index"], 2);
    assert_eq!(body["auto_tracks"]["subtitle_index"], 5);

    let (s, _) = t
        .call("PUT", "/api/modules/auto-tracks", Some(json!({"settings": {"library_fallback": "false"}})), Some(&owner))
        .await;
    assert_eq!(s, StatusCode::OK);
    let (_, body) = t.call("POST", "/api/playback/fo", Some(start), Some(&owner)).await;
    assert!(body.get("auto_tracks").is_none());
}
