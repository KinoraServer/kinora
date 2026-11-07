//! Accounts, sessions and the role check every route goes through.
//!
//! Sessions are random tokens sent as an HttpOnly cookie (web) or an
//! `Authorization: Bearer` header (future TV apps). Only a SHA-256 of the
//! token is stored, so a leaked database doesn't leak sessions.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::body::Bytes;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use kinora_core::roles::Role;
use kinora_plugin_api::ServerEvent;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::db::{new_id, now_ms};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "kinora_session";
const SESSION_DAYS: i64 = 90;

#[derive(Debug, Clone, Serialize)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
    pub role: Role,
    /// When the profile picture last changed (Unix ms), or `None` without one.
    /// Clients add it to the picture URL so a new picture skips the cache.
    pub avatar_version: Option<i64>,
}

impl AuthUser {
    /// The one permission check. Call it at the top of every handler that
    /// needs more than "signed in".
    pub fn require(&self, role: Role) -> AppResult<()> {
        if self.role.at_least(role) {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = token_from_headers(&parts.headers).ok_or(AppError::Unauthorized)?;
        user_for_token(state, &token).await?.ok_or(AppError::Unauthorized)
    }
}

/// Like `AuthUser` but already checked for admin.
pub struct AdminUser(pub AuthUser);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        user.require(Role::Admin)?;
        Ok(AdminUser(user))
    }
}

fn token_from_headers(headers: &HeaderMap) -> Option<String> {
    if let Some(v) = headers.get(axum::http::header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        if let Some(t) = v.strip_prefix("Bearer ") {
            return Some(t.trim().to_string());
        }
    }
    CookieJar::from_headers(headers).get(SESSION_COOKIE).map(|c| c.value().to_string())
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

async fn user_for_token(state: &AppState, token: &str) -> AppResult<Option<AuthUser>> {
    let row: Option<(String, String, String, Option<i64>)> = sqlx::query_as(
        "SELECT u.id, u.username, u.role, a.updated_at FROM sessions s JOIN users u ON u.id = s.user_id
         LEFT JOIN user_avatars a ON a.user_id = u.id
         WHERE s.token_hash = ? AND s.expires_at > ?",
    )
    .bind(hash_token(token))
    .bind(now_ms())
    .fetch_optional(&state.db)
    .await?;
    Ok(row.map(|(id, username, role, avatar_version)| AuthUser {
        id,
        username,
        role: Role::parse(&role).unwrap_or(Role::Viewer),
        avatar_version,
    }))
}

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("hashing failed: {e}"))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).map(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok()).unwrap_or(false)
}

fn validate_username(username: &str) -> AppResult<()> {
    let u = username.trim();
    if u.is_empty() || u.len() > 64 {
        return Err(AppError::BadRequest("username must be 1-64 characters".into()));
    }
    Ok(())
}

fn validate_credentials(username: &str, password: &str) -> AppResult<()> {
    validate_username(username)?;
    if password.chars().count() < 8 {
        return Err(AppError::BadRequest("password must be at least 8 characters".into()));
    }
    Ok(())
}

async fn create_session(state: &AppState, user_id: &str, device: Option<&str>) -> AppResult<String> {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let token = hex::encode(bytes);
    let now = now_ms();
    sqlx::query("INSERT INTO sessions (token_hash, user_id, device, created_at, expires_at) VALUES (?, ?, ?, ?, ?)")
        .bind(hash_token(&token))
        .bind(user_id)
        .bind(device)
        .bind(now)
        .bind(now + SESSION_DAYS * 24 * 3600 * 1000)
        .execute(&state.db)
        .await?;
    sqlx::query("UPDATE users SET last_login_at = ? WHERE id = ?").bind(now).bind(user_id).execute(&state.db).await?;
    Ok(token)
}

fn session_cookie(token: String) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time_days(SESSION_DAYS))
        .build()
}

fn time_days(days: i64) -> time::Duration {
    time::Duration::days(days)
}

pub async fn user_count(state: &AppState) -> AppResult<i64> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&state.db).await?)
}

// --- routes -----------------------------------------------------------------

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/setup", post(setup))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
        .route("/api/auth/password", post(change_password))
        .route("/api/auth/profile", patch(update_profile))
        .route("/api/auth/avatar", put(set_avatar).delete(remove_avatar))
        .route("/api/users", get(list_users).post(create_user))
        .route("/api/users/{id}", delete(delete_user))
        .route("/api/users/{id}/avatar", get(avatar))
}

#[derive(Deserialize)]
struct Credentials {
    username: String,
    password: String,
    #[serde(default)]
    device: Option<String>,
}

#[derive(Serialize)]
struct LoginResponse {
    user: AuthUser,
    /// Returned for non-browser clients that use the Authorization header.
    token: String,
}

/// First-run: create the owner account. Only works while there are no users.
async fn setup(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<Credentials>,
) -> AppResult<(CookieJar, Json<LoginResponse>)> {
    validate_credentials(&body.username, &body.password)?;
    if user_count(&state).await? > 0 {
        return Err(AppError::Conflict("setup is already complete".into()));
    }
    let id = new_id();
    sqlx::query("INSERT INTO users (id, username, password_hash, role, created_at) VALUES (?, ?, ?, 'owner', ?)")
        .bind(&id)
        .bind(body.username.trim())
        .bind(hash_password(&body.password)?)
        .bind(now_ms())
        .execute(&state.db)
        .await?;
    state.emit(ServerEvent::UserCreated { user_id: id.clone(), username: body.username.trim().into() });
    let token = create_session(&state, &id, body.device.as_deref()).await?;
    let user = AuthUser { id, username: body.username.trim().into(), role: Role::Owner, avatar_version: None };
    Ok((jar.add(session_cookie(token.clone())), Json(LoginResponse { user, token })))
}

async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<Credentials>,
) -> AppResult<(CookieJar, Json<LoginResponse>)> {
    let row: Option<(String, String, String, String, Option<i64>)> = sqlx::query_as(
        "SELECT u.id, u.username, u.password_hash, u.role, a.updated_at FROM users u
         LEFT JOIN user_avatars a ON a.user_id = u.id WHERE u.username = ?",
    )
    .bind(body.username.trim())
    .fetch_optional(&state.db)
    .await?;
    let Some((id, username, hash, role, avatar_version)) = row else {
        // Same work either way so response time doesn't reveal valid usernames.
        let _ = hash_password(&body.password);
        return Err(AppError::BadRequest("wrong username or password".into()));
    };
    if !verify_password(&body.password, &hash) {
        return Err(AppError::BadRequest("wrong username or password".into()));
    }
    let token = create_session(&state, &id, body.device.as_deref()).await?;
    let user = AuthUser { id, username, role: Role::parse(&role).unwrap_or(Role::Viewer), avatar_version };
    Ok((jar.add(session_cookie(token.clone())), Json(LoginResponse { user, token })))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap, jar: CookieJar) -> AppResult<CookieJar> {
    if let Some(token) = token_from_headers(&headers) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?").bind(hash_token(&token)).execute(&state.db).await?;
    }
    Ok(jar.remove(Cookie::build(SESSION_COOKIE).path("/")))
}

async fn me(user: AuthUser) -> Json<AuthUser> {
    Json(user)
}

#[derive(Deserialize)]
struct ChangePassword {
    current_password: String,
    new_password: String,
}

async fn change_password(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<ChangePassword>,
) -> AppResult<Json<serde_json::Value>> {
    validate_credentials(&user.username, &body.new_password)?;
    let hash: String =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?").bind(&user.id).fetch_one(&state.db).await?;
    if !verify_password(&body.current_password, &hash) {
        return Err(AppError::BadRequest("current password is wrong".into()));
    }
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(hash_password(&body.new_password)?)
        .bind(&user.id)
        .execute(&state.db)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
struct ProfileUpdate {
    username: String,
}

/// Rename yourself. The new name is also what you sign in with.
async fn update_profile(
    State(state): State<AppState>,
    mut user: AuthUser,
    Json(body): Json<ProfileUpdate>,
) -> AppResult<Json<AuthUser>> {
    validate_username(&body.username)?;
    let username = body.username.trim().to_string();
    let res = sqlx::query("UPDATE users SET username = ? WHERE id = ?")
        .bind(&username)
        .bind(&user.id)
        .execute(&state.db)
        .await;
    if let Err(sqlx::Error::Database(e)) = &res {
        if e.is_unique_violation() {
            return Err(AppError::Conflict("that username is taken".into()));
        }
    }
    res?;
    user.username = username;
    Ok(Json(user))
}

/// Largest profile picture we keep. The web app uploads ~256px images that are
/// far smaller; this only stops someone storing huge files.
const MAX_AVATAR_BYTES: usize = 1024 * 1024;

/// The image type from the file's first bytes. The client's Content-Type is
/// ignored so nothing but these four formats is ever served back.
fn image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Upload your profile picture: the raw image as the request body.
async fn set_avatar(State(state): State<AppState>, mut user: AuthUser, body: Bytes) -> AppResult<Json<AuthUser>> {
    if body.len() > MAX_AVATAR_BYTES {
        return Err(AppError::BadRequest("the picture must be 1 MB or smaller".into()));
    }
    let content_type =
        image_type(&body).ok_or_else(|| AppError::BadRequest("use a PNG, JPEG, WebP or GIF picture".into()))?;
    let now = now_ms();
    sqlx::query(
        "INSERT INTO user_avatars (user_id, content_type, data, updated_at) VALUES (?, ?, ?, ?)
         ON CONFLICT(user_id) DO UPDATE SET content_type = excluded.content_type, data = excluded.data,
         updated_at = excluded.updated_at",
    )
    .bind(&user.id)
    .bind(content_type)
    .bind(body.as_ref())
    .bind(now)
    .execute(&state.db)
    .await?;
    user.avatar_version = Some(now);
    Ok(Json(user))
}

async fn remove_avatar(State(state): State<AppState>, mut user: AuthUser) -> AppResult<Json<AuthUser>> {
    sqlx::query("DELETE FROM user_avatars WHERE user_id = ?").bind(&user.id).execute(&state.db).await?;
    user.avatar_version = None;
    Ok(Json(user))
}

/// Anyone signed in can see anyone's picture (for profile pickers and
/// "who's watching" later). The URL carries `?v=<avatar_version>`, so it can
/// be cached for a long time.
async fn avatar(State(state): State<AppState>, _user: AuthUser, Path(id): Path<String>) -> AppResult<Response> {
    let (content_type, data): (String, Vec<u8>) =
        sqlx::query_as("SELECT content_type, data FROM user_avatars WHERE user_id = ?")
            .bind(&id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::NotFound)?;
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "private, max-age=31536000, immutable".into()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
        ],
        data,
    )
        .into_response())
}

#[derive(Serialize, sqlx::FromRow)]
struct UserRow {
    id: String,
    username: String,
    role: String,
    created_at: i64,
    last_login_at: Option<i64>,
}

async fn list_users(State(state): State<AppState>, AdminUser(_): AdminUser) -> AppResult<Json<Vec<UserRow>>> {
    let rows = sqlx::query_as("SELECT id, username, role, created_at, last_login_at FROM users ORDER BY created_at")
        .fetch_all(&state.db)
        .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct NewUser {
    username: String,
    password: String,
    role: Role,
}

async fn create_user(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Json(body): Json<NewUser>,
) -> AppResult<Json<UserRow>> {
    validate_credentials(&body.username, &body.password)?;
    if !admin.role.can_manage(body.role) {
        return Err(AppError::Forbidden);
    }
    let id = new_id();
    let now = now_ms();
    let res = sqlx::query("INSERT INTO users (id, username, password_hash, role, created_at) VALUES (?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(body.username.trim())
        .bind(hash_password(&body.password)?)
        .bind(body.role.as_str())
        .bind(now)
        .execute(&state.db)
        .await;
    if let Err(sqlx::Error::Database(e)) = &res {
        if e.is_unique_violation() {
            return Err(AppError::Conflict("that username is taken".into()));
        }
    }
    res?;
    state.emit(ServerEvent::UserCreated { user_id: id.clone(), username: body.username.trim().into() });
    Ok(Json(UserRow {
        id,
        username: body.username.trim().into(),
        role: body.role.as_str().into(),
        created_at: now,
        last_login_at: None,
    }))
}

async fn delete_user(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id = ?").bind(&id).fetch_one(&state.db).await?;
    let target = Role::parse(&role).unwrap_or(Role::Viewer);
    if id == admin.id || !admin.role.can_manage(target) {
        return Err(AppError::Forbidden);
    }
    sqlx::query("DELETE FROM users WHERE id = ?").bind(&id).execute(&state.db).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_round_trip() {
        let h = hash_password("correct horse").unwrap();
        assert!(verify_password("correct horse", &h));
        assert!(!verify_password("wrong horse", &h));
        assert!(!verify_password("x", "not a hash"));
    }

    #[test]
    fn avatar_type_comes_from_the_bytes() {
        assert_eq!(image_type(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
        assert_eq!(image_type(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(image_type(b"GIF89a..."), Some("image/gif"));
        assert_eq!(image_type(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(image_type(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"), None);
        assert_eq!(image_type(b""), None);
    }

    #[test]
    fn token_hash_is_stable_and_not_the_token() {
        assert_eq!(hash_token("abc"), hash_token("abc"));
        assert_ne!(hash_token("abc"), "abc");
    }
}
