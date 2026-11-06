//! Accounts, sessions and the role check every route goes through.
//!
//! Sessions are random tokens sent as an HttpOnly cookie (web) or an
//! `Authorization: Bearer` header (future TV apps). Only a SHA-256 of the
//! token is stored, so a leaked database doesn't leak sessions.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::HeaderMap;
use axum::routing::{delete, get, post};
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
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT u.id, u.username, u.role FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = ? AND s.expires_at > ?",
    )
    .bind(hash_token(token))
    .bind(now_ms())
    .fetch_optional(&state.db)
    .await?;
    Ok(row.map(|(id, username, role)| AuthUser { id, username, role: Role::parse(&role).unwrap_or(Role::Viewer) }))
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

fn validate_credentials(username: &str, password: &str) -> AppResult<()> {
    let u = username.trim();
    if u.is_empty() || u.len() > 64 {
        return Err(AppError::BadRequest("username must be 1-64 characters".into()));
    }
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
        .route("/api/users", get(list_users).post(create_user))
        .route("/api/users/{id}", delete(delete_user))
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
    let user = AuthUser { id, username: body.username.trim().into(), role: Role::Owner };
    Ok((jar.add(session_cookie(token.clone())), Json(LoginResponse { user, token })))
}

async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<Credentials>,
) -> AppResult<(CookieJar, Json<LoginResponse>)> {
    let row: Option<(String, String, String, String)> =
        sqlx::query_as("SELECT id, username, password_hash, role FROM users WHERE username = ?")
            .bind(body.username.trim())
            .fetch_optional(&state.db)
            .await?;
    let Some((id, username, hash, role)) = row else {
        // Same work either way so response time doesn't reveal valid usernames.
        let _ = hash_password(&body.password);
        return Err(AppError::BadRequest("wrong username or password".into()));
    };
    if !verify_password(&body.password, &hash) {
        return Err(AppError::BadRequest("wrong username or password".into()));
    }
    let token = create_session(&state, &id, body.device.as_deref()).await?;
    let user = AuthUser { id, username, role: Role::parse(&role).unwrap_or(Role::Viewer) };
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
    fn token_hash_is_stable_and_not_the_token() {
        assert_eq!(hash_token("abc"), hash_token("abc"));
        assert_ne!(hash_token("abc"), "abc");
    }
}
