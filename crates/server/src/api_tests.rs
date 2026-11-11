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
    assert_eq!(list["starters"], json!(["marquee", "blank"]));

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
async fn watch_party_is_off_by_default_and_resolves_titles_when_on() {
    let t = app().await;
    let owner = t.setup_owner().await;
    seed_show(&t.db).await;
    sqlx::query("UPDATE items SET title = 'Kantai Show', provider_ids = '{\"tmdb\":\"1396\"}' WHERE id = 'show'")
        .execute(&t.db)
        .await
        .unwrap();
    sqlx::query("UPDATE items SET title = 'Pilot', provider_ids = '{\"tmdb\":62085,\"custom\":\"x\"}' WHERE id = 'e1'")
        .execute(&t.db)
        .await
        .unwrap();

    // Off: the web app hides parties and the resolver routes answer 404.
    let (s, cfg) = t.call("GET", "/api/watch-party", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(cfg["enabled"], false);
    assert!(cfg["client"].as_str().unwrap().starts_with("Kinora/"));
    let (s, _) = t.call("GET", "/api/watch-party/items/e1", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // On, with no other settings: the public relay (no relay URL).
    let (s, _) = t.call("PUT", "/api/modules/watch-party", Some(json!({"enabled": true})), Some(&owner)).await;
    assert_eq!(s, StatusCode::OK);
    let (_, cfg) = t.call("GET", "/api/watch-party", None, Some(&owner)).await;
    assert_eq!(cfg["enabled"], true);
    assert_eq!(cfg["relay_url"], Value::Null);

    // An own relay: checked, trimmed and handed to the web app.
    let bad = json!({"settings": {"relay_url": "relay.example.com"}});
    let (s, _) = t.call("PUT", "/api/modules/watch-party", Some(bad), Some(&owner)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let good = json!({"settings": {"relay_url": " https://relay.example.com "}});
    let (s, _) = t.call("PUT", "/api/modules/watch-party", Some(good), Some(&owner)).await;
    assert_eq!(s, StatusCode::OK);
    let (_, cfg) = t.call("GET", "/api/watch-party", None, Some(&owner)).await;
    assert_eq!(cfg["relay_url"], "https://relay.example.com");

    // The host's own item, with only the namespaces Open Watch Party knows.
    let (s, item) = t.call("GET", "/api/watch-party/items/e1", None, Some(&owner)).await;
    assert_eq!(s, StatusCode::OK, "{item}");
    assert_eq!(item["kind"], "episode");
    assert_eq!(item["ids"], json!({"tmdb": "62085"}));
    assert_eq!(item["show"]["title"], "Kantai Show");
    assert_eq!(item["show"]["ids"], json!({"tmdb": "1396"}));
    assert_eq!(item["duration_ms"], 1_420_000);

    // Step 1: by the episode's own ID, or by the series ID narrowed to the same numbers.
    let (_, found) =
        t.call("POST", "/api/watch-party/find", Some(json!({"ids": {"tmdb": "62085"}})), Some(&owner)).await;
    assert_eq!(found.as_array().unwrap().len(), 1);
    assert_eq!(found[0]["id"], "e1");
    let body = json!({"series_ids": {"tmdb": "1396"}, "season": 1, "episode": 2});
    let (_, found) = t.call("POST", "/api/watch-party/find", Some(body), Some(&owner)).await;
    assert_eq!(found.as_array().unwrap().len(), 1);
    assert_eq!(found[0]["id"], "e2");
    // Unknown namespaces are ignored, never used as a JSON path.
    let body = json!({"ids": {"x') OR 1=1 --": "1"}});
    let (_, found) = t.call("POST", "/api/watch-party/find", Some(body), Some(&owner)).await;
    assert_eq!(found, json!([]));

    // Step 2: episodes by their show's title, the room's numbers first; LIKE wildcards are literal.
    let (_, found) =
        t.call("GET", "/api/watch-party/search?q=kantai&kind=episode&season=1&episode=2", None, Some(&owner)).await;
    assert_eq!(found.as_array().unwrap().len(), 2);
    assert_eq!(found[0]["id"], "e2");
    let (_, found) = t.call("GET", "/api/watch-party/search?q=%25&kind=episode", None, Some(&owner)).await;
    assert_eq!(found, json!([]));
}
