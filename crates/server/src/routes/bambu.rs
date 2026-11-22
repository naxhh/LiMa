//! Bambu account login for MakerWorld downloads.
//!
//! Lima has no users, so the Bambu token is never stored server-side: after a
//! login it is handed to the browser as an HttpOnly cookie scoped to `/api`, and
//! read back from that cookie when an import needs it.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use lima_importers::bambu::{self, AuthError, LoginOutcome};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::models::http_error::{ApiErrorBody, ApiErrorResponse};
use crate::state::AppState;

pub const COOKIE: &str = "lima_bambu_token";
/// Cookie lifetime when the token does not say when it expires.
const DEFAULT_MAX_AGE: i64 = 30 * 24 * 3600;

#[derive(Serialize, ToSchema)]
pub struct BambuStatusResponse {
    pub logged_in: bool,
    /// Token expiry (unix seconds), when the token carries one.
    pub expires_at: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
pub struct BambuLoginRequest {
    /// Bambu account email.
    pub account: String,
    pub password: String,
}

#[derive(Deserialize, ToSchema)]
pub struct BambuSendCodeRequest {
    /// Bambu account email. Works for accounts created with Google/Apple sign-in too,
    /// which have no Bambu password.
    pub account: String,
}

#[derive(Deserialize, ToSchema)]
pub struct BambuCodeRequest {
    pub account: String,
    /// Code Bambu emailed.
    pub code: String,
}

#[derive(Deserialize, ToSchema)]
pub struct BambuTfaRequest {
    pub tfa_key: String,
    /// Code from the authenticator app.
    pub code: String,
}

#[derive(Deserialize, ToSchema)]
pub struct BambuTokenRequest {
    /// Access token copied from the `token` cookie of a logged-in makerworld.com session.
    pub token: String,
}

#[derive(Serialize, ToSchema)]
pub struct BambuLoginResponse {
    /// `logged_in`, `code_sent` (finish with /login/code) or `tfa_required` (finish with /login/tfa).
    pub status: String,
    pub tfa_key: Option<String>,
    pub expires_at: Option<i64>,
}

/// Token from the request cookie, if any.
pub fn token_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE)
        .map(|(_, v)| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn secure(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("https"))
}

fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

fn set_cookie(token: &str, request: &HeaderMap) -> HeaderValue {
    let max_age = bambu::token_expiry(token)
        .map(|exp| exp - now())
        .filter(|s| *s > 0)
        .unwrap_or(DEFAULT_MAX_AGE);
    let secure = if secure(request) { "; Secure" } else { "" };
    HeaderValue::from_str(&format!("{COOKIE}={token}; Path=/api; HttpOnly; SameSite=Strict; Max-Age={max_age}{secure}"))
        .expect("token characters were validated")
}

pub fn clear_cookie() -> HeaderValue {
    HeaderValue::from_str(&format!("{COOKIE}=; Path=/api; HttpOnly; SameSite=Strict; Max-Age=0")).unwrap()
}

/// Tokens go into a cookie as-is, so they must be plain cookie-safe ASCII.
fn valid_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 8192
        && token.bytes().all(|b| b.is_ascii_graphic() && !matches!(b, b';' | b',' | b'"' | b'\\'))
}

/// 401 asking the UI to log in to Bambu again, dropping the stale cookie.
pub fn login_required() -> Response {
    let mut res = ApiErrorResponse::new(
        StatusCode::UNAUTHORIZED,
        "bambu_login_required",
        "Log in to your Bambu account to download from MakerWorld.",
    )
    .into_response();
    res.headers_mut().append(header::SET_COOKIE, clear_cookie());
    res
}

fn auth_error(e: AuthError) -> ApiErrorResponse {
    match e {
        AuthError::BadCredentials => {
            ApiErrorResponse::new(StatusCode::UNAUTHORIZED, "bambu_bad_credentials", "Wrong email or password.")
        }
        AuthError::CodeIncorrect => ApiErrorResponse::new(StatusCode::BAD_REQUEST, "bambu_code_incorrect", "The code is incorrect."),
        AuthError::CodeExpired => ApiErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "bambu_code_expired",
            "The code has expired. A new one was sent to your email.",
        ),
        AuthError::Other(e) => {
            tracing::warn!("Bambu login failed: {e}");
            ApiErrorResponse::new(StatusCode::BAD_GATEWAY, "bambu_upstream", "Bambu login failed.").with_cause(&e.to_string())
        }
    }
}

fn logged_in(token: String, request: &HeaderMap) -> Result<Response, ApiErrorResponse> {
    if !valid_token(&token) {
        return Err(ApiErrorResponse::new(StatusCode::BAD_GATEWAY, "bambu_upstream", "Bambu returned an unusable token."));
    }
    let body = BambuLoginResponse { status: "logged_in".into(), tfa_key: None, expires_at: bambu::token_expiry(&token) };
    let mut res = (StatusCode::OK, Json(body)).into_response();
    res.headers_mut().append(header::SET_COOKIE, set_cookie(&token, request));
    Ok(res)
}

#[utoipa::path(
    get,
    path = "/api/integrations/bambu/status",
    responses(
        (status = 200, description = "Whether this browser holds a working Bambu token", body = BambuStatusResponse),
    )
)]
pub async fn bambu_status(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(token) = token_from_headers(&headers) else {
        return Json(BambuStatusResponse { logged_in: false, expires_at: None }).into_response();
    };
    match bambu::verify_token(&state.http, &token).await {
        Ok(true) => Json(BambuStatusResponse { logged_in: true, expires_at: bambu::token_expiry(&token) }).into_response(),
        Ok(false) => {
            let mut res = Json(BambuStatusResponse { logged_in: false, expires_at: None }).into_response();
            res.headers_mut().append(header::SET_COOKIE, clear_cookie());
            res
        }
        // Bambu unreachable: keep the cookie and report what we know.
        Err(e) => {
            tracing::warn!("Could not check Bambu token: {e}");
            Json(BambuStatusResponse { logged_in: true, expires_at: bambu::token_expiry(&token) }).into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/integrations/bambu/login",
    request_body = BambuLoginRequest,
    responses(
        (status = 200, description = "Logged in, or a code is needed to finish", body = BambuLoginResponse),
        (status = 401, description = "Wrong email or password", body = ApiErrorBody),
        (status = 502, description = "Bambu answered unexpectedly", body = ApiErrorBody),
    )
)]
pub async fn bambu_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BambuLoginRequest>,
) -> Result<Response, ApiErrorResponse> {
    let account = body.account.trim();
    if account.is_empty() || body.password.is_empty() {
        return Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "missing_fields", "Email and password are required."));
    }
    match bambu::login(&state.http, account, &body.password).await.map_err(auth_error)? {
        LoginOutcome::Token(token) => logged_in(token, &headers),
        LoginOutcome::CodeSent => {
            Ok(Json(BambuLoginResponse { status: "code_sent".into(), tfa_key: None, expires_at: None }).into_response())
        }
        LoginOutcome::TfaRequired { tfa_key } => {
            Ok(Json(BambuLoginResponse { status: "tfa_required".into(), tfa_key: Some(tfa_key), expires_at: None }).into_response())
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/integrations/bambu/login/send-code",
    request_body = BambuSendCodeRequest,
    responses(
        (status = 200, description = "Code emailed; finish with /login/code", body = BambuLoginResponse),
        (status = 400, description = "Missing email", body = ApiErrorBody),
        (status = 502, description = "Bambu answered unexpectedly", body = ApiErrorBody),
    )
)]
pub async fn bambu_send_code(
    State(state): State<AppState>,
    Json(body): Json<BambuSendCodeRequest>,
) -> Result<Response, ApiErrorResponse> {
    let account = body.account.trim();
    if account.is_empty() {
        return Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "missing_fields", "Email is required."));
    }
    bambu::send_email_code(&state.http, account).await.map_err(auth_error)?;
    Ok(Json(BambuLoginResponse { status: "code_sent".into(), tfa_key: None, expires_at: None }).into_response())
}

#[utoipa::path(
    post,
    path = "/api/integrations/bambu/login/code",
    request_body = BambuCodeRequest,
    responses(
        (status = 200, description = "Logged in", body = BambuLoginResponse),
        (status = 400, description = "Code incorrect or expired", body = ApiErrorBody),
        (status = 502, description = "Bambu answered unexpectedly", body = ApiErrorBody),
    )
)]
pub async fn bambu_login_code(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BambuCodeRequest>,
) -> Result<Response, ApiErrorResponse> {
    let token = bambu::login_with_code(&state.http, body.account.trim(), &body.code).await.map_err(auth_error)?;
    logged_in(token, &headers)
}

#[utoipa::path(
    post,
    path = "/api/integrations/bambu/login/tfa",
    request_body = BambuTfaRequest,
    responses(
        (status = 200, description = "Logged in", body = BambuLoginResponse),
        (status = 400, description = "Code incorrect", body = ApiErrorBody),
        (status = 502, description = "Bambu answered unexpectedly", body = ApiErrorBody),
    )
)]
pub async fn bambu_login_tfa(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BambuTfaRequest>,
) -> Result<Response, ApiErrorResponse> {
    let token = bambu::login_with_tfa(&state.http, &body.tfa_key, &body.code).await.map_err(auth_error)?;
    logged_in(token, &headers)
}

#[utoipa::path(
    post,
    path = "/api/integrations/bambu/token",
    request_body = BambuTokenRequest,
    responses(
        (status = 200, description = "Token accepted", body = BambuLoginResponse),
        (status = 400, description = "Malformed token", body = ApiErrorBody),
        (status = 401, description = "Bambu rejected the token", body = ApiErrorBody),
    )
)]
pub async fn bambu_set_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BambuTokenRequest>,
) -> Result<Response, ApiErrorResponse> {
    let token = body.token.trim().trim_start_matches("Bearer ").trim().to_string();
    if !valid_token(&token) {
        return Err(ApiErrorResponse::new(StatusCode::BAD_REQUEST, "invalid_token", "That does not look like a token."));
    }
    match bambu::verify_token(&state.http, &token).await {
        Ok(true) => logged_in(token, &headers),
        Ok(false) => Err(ApiErrorResponse::new(StatusCode::UNAUTHORIZED, "bambu_token_rejected", "Bambu rejected this token.")),
        Err(e) => Err(ApiErrorResponse::new(StatusCode::BAD_GATEWAY, "bambu_upstream", "Could not check the token with Bambu.")
            .with_cause(&e.to_string())),
    }
}

#[utoipa::path(
    post,
    path = "/api/integrations/bambu/logout",
    responses((status = 204, description = "Cookie removed")),
)]
pub async fn bambu_logout() -> Response {
    let mut res = StatusCode::NO_CONTENT.into_response();
    res.headers_mut().append(header::SET_COOKIE, clear_cookie());
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_token_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_static("theme=dark; lima_bambu_token=abc.def.ghi; x=1"));
        assert_eq!(token_from_headers(&headers).as_deref(), Some("abc.def.ghi"));
        headers.insert(header::COOKIE, HeaderValue::from_static("lima_bambu_token="));
        assert_eq!(token_from_headers(&headers), None);
    }

    #[test]
    fn cookie_flags() {
        let mut headers = HeaderMap::new();
        let plain = set_cookie("tok", &headers);
        let plain = plain.to_str().unwrap();
        assert!(plain.starts_with("lima_bambu_token=tok; Path=/api; HttpOnly; SameSite=Strict; Max-Age=2592000"), "{plain}");
        assert!(!plain.contains("Secure"));
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));
        assert!(set_cookie("tok", &headers).to_str().unwrap().ends_with("; Secure"));
    }

    #[test]
    fn token_validation() {
        assert!(valid_token("eyJ.abc-_.x"));
        assert!(!valid_token("a b"));
        assert!(!valid_token("a;b"));
        assert!(!valid_token(""));
    }
}
