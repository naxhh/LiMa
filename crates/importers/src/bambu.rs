//! Bambu Lab account login, used to get the access token MakerWorld downloads need.
//!
//! The flow mirrors the one used by the Home Assistant Bambu integration
//! (ha-bambulab): a password login either returns a token straight away or asks
//! for a code that Bambu emails, or for an authenticator (2FA) code.
//! Lima never stores the token; the server hands it to the browser as a cookie.

use base64::Engine;
use serde::Deserialize;
use serde_json::json;

use crate::{ImportError, Result};

const LOGIN: &str = "https://api.bambulab.com/v1/user-service/user/login";
const EMAIL_CODE: &str = "https://api.bambulab.com/v1/user-service/user/sendemail/code";
const CSRF: &str = "https://bambulab.com/api/csrf";
const TFA_LOGIN: &str = "https://bambulab.com/api/sign-in/tfa";
const MY_PROFILE: &str = "https://api.bambulab.com/v1/design-user-service/my/profile";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginOutcome {
    /// Logged in; the access token.
    Token(String),
    /// Bambu emailed a code; call [`login_with_code`].
    CodeSent,
    /// The account uses an authenticator app; call [`login_with_tfa`] with this key.
    TfaRequired { tfa_key: String },
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("wrong email or password")]
    BadCredentials,
    #[error("the code is incorrect")]
    CodeIncorrect,
    #[error("the code has expired; a new one was sent")]
    CodeExpired,
    #[error("{0}")]
    Other(#[from] ImportError),
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct LoginResponse {
    access_token: String,
    login_type: String,
    tfa_key: String,
    code: Option<i64>,
    error: Option<String>,
}

fn upstream(message: impl Into<String>) -> AuthError {
    AuthError::Other(ImportError::Upstream { site: "bambu", message: message.into() })
}

fn http(e: reqwest::Error) -> AuthError {
    AuthError::Other(ImportError::Http(e))
}

async fn post_login(client: &reqwest::Client, body: serde_json::Value) -> std::result::Result<(reqwest::StatusCode, LoginResponse), AuthError> {
    let res = client.post(LOGIN).json(&body).send().await.map_err(http)?;
    let status = res.status();
    let text = res.text().await.map_err(http)?;
    let parsed: LoginResponse = serde_json::from_str(&text).unwrap_or_default();
    tracing::debug!("Bambu login answered {status} (loginType={:?})", parsed.login_type);
    Ok((status, parsed))
}

/// Starts a password login.
pub async fn login(client: &reqwest::Client, account: &str, password: &str) -> std::result::Result<LoginOutcome, AuthError> {
    let (status, res) = post_login(client, json!({ "account": account, "password": password, "apiError": "" })).await?;
    if !res.access_token.is_empty() {
        return Ok(LoginOutcome::Token(res.access_token));
    }
    match res.login_type.as_str() {
        "verifyCode" => {
            send_email_code(client, account).await?;
            Ok(LoginOutcome::CodeSent)
        }
        "tfa" if !res.tfa_key.is_empty() => Ok(LoginOutcome::TfaRequired { tfa_key: res.tfa_key }),
        _ if status.is_client_error() => Err(AuthError::BadCredentials),
        _ => Err(upstream(format!("{status}: {}", res.error.unwrap_or_else(|| "unexpected login answer".into())))),
    }
}

/// Asks Bambu to email a login code.
pub async fn send_email_code(client: &reqwest::Client, email: &str) -> std::result::Result<(), AuthError> {
    let res = client
        .post(EMAIL_CODE)
        .json(&json!({ "email": email, "type": "codeLogin" }))
        .send()
        .await
        .map_err(http)?;
    if !res.status().is_success() {
        let status = res.status();
        return Err(upstream(format!("could not send the login code ({status}): {}", res.text().await.unwrap_or_default())));
    }
    Ok(())
}

/// Finishes a login with the emailed code.
pub async fn login_with_code(client: &reqwest::Client, account: &str, code: &str) -> std::result::Result<String, AuthError> {
    let (status, res) = post_login(client, json!({ "account": account, "code": code.trim() })).await?;
    if !res.access_token.is_empty() {
        return Ok(res.access_token);
    }
    match (status.as_u16(), res.code) {
        (400, Some(1)) => {
            send_email_code(client, account).await?;
            Err(AuthError::CodeExpired)
        }
        (400, Some(2)) => Err(AuthError::CodeIncorrect),
        _ => Err(upstream(format!("{status}: {}", res.error.unwrap_or_else(|| "login failed".into())))),
    }
}

/// Finishes a login with an authenticator-app code. This goes through the
/// bambulab.com website, which wants a CSRF cookie and returns the token as a cookie.
pub async fn login_with_tfa(client: &reqwest::Client, tfa_key: &str, code: &str) -> std::result::Result<String, AuthError> {
    let csrf_res = client.get(CSRF).send().await.map_err(http)?;
    let csrf = cookie_value(csrf_res.headers(), "bbl_csrf_token").ok_or_else(|| upstream("could not get a CSRF token from bambulab.com"))?;

    let res = client
        .post(TFA_LOGIN)
        .header("x-bbl-csrf-token", &csrf)
        .header("cookie", format!("bbl_csrf_token={csrf}"))
        .json(&json!({ "tfaKey": tfa_key, "tfaCode": code.trim() }))
        .send()
        .await
        .map_err(http)?;
    let status = res.status();
    let token = cookie_value(res.headers(), "token");
    let body = res.text().await.unwrap_or_default();
    if status.as_u16() == 400 && body.contains("Incorrect code") {
        return Err(AuthError::CodeIncorrect);
    }
    match token {
        Some(t) if status.is_success() && !t.is_empty() => Ok(t),
        _ => Err(upstream(format!("2FA login failed ({status})"))),
    }
}

fn cookie_value(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter_map(|v| v.split(';').next())
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| k.trim() == name)
        .map(|(_, v)| v.trim().to_string())
}

/// Checks the token against an endpoint that needs a login.
/// `Ok(false)` when Bambu rejects it.
pub async fn verify_token(client: &reqwest::Client, token: &str) -> Result<bool> {
    let res = client.get(MY_PROFILE).bearer_auth(token).header("accept", "application/json").send().await?;
    match res.status().as_u16() {
        200..=299 => Ok(true),
        401 | 403 => Ok(false),
        s => Err(ImportError::Upstream { site: "bambu", message: format!("token check answered {s}") }),
    }
}

/// Expiry (unix seconds) when the token is a JWT with an `exp` claim.
pub fn token_expiry(token: &str) -> Option<i64> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    claims.get("exp")?.as_i64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_jwt_expiry() {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"username":"u_1","exp":1795000000}"#);
        assert_eq!(token_expiry(&format!("eyJhbGciOiJIUzI1NiJ9.{payload}.sig")), Some(1795000000));
        assert_eq!(token_expiry("opaque-token"), None);
        assert_eq!(token_expiry("a.!!!.c"), None);
    }

    #[test]
    fn reads_set_cookie() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.append(reqwest::header::SET_COOKIE, "other=1; Path=/".parse().unwrap());
        headers.append(reqwest::header::SET_COOKIE, "token=abc.def; Path=/; HttpOnly".parse().unwrap());
        assert_eq!(cookie_value(&headers, "token").as_deref(), Some("abc.def"));
        assert_eq!(cookie_value(&headers, "missing"), None);
    }
}
