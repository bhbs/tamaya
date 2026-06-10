use super::email::{send_password_reset_email, send_verification_email};
use super::errors::AuthError;
use super::password;
use super::service::Service;
use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use rusqlite::Connection;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
}

pub fn routes(state: AppState) -> Router {
    Router::new()
        .route("/api/auth/signup", post(sign_up))
        .route("/api/auth/signin", post(sign_in))
        .route("/api/auth/signout", post(sign_out))
        .route("/api/auth/session", get(session))
        .route("/api/auth/verify-email", get(verify_email_get).post(verify_email_post))
        .route("/api/auth/forgot-password", post(forgot_password))
        .route("/api/auth/reset-password", post(reset_password))
        .route("/health", get(health))
        .with_state(state)
}

fn json_response(status: StatusCode, value: Value) -> Response {
    (status, Json(value)).into_response()
}

fn json_error(status: StatusCode, message: &str) -> Response {
    json_response(status, json!({ "error": message }))
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie_header
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("session=").map(str::trim).map(str::to_string))
}

fn set_session_cookie(headers: &mut HeaderMap, token: &str) {
    let value = format!(
        "session={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        30 * 24 * 60 * 60
    );
    if let Ok(header_value) = HeaderValue::from_str(&value) {
        headers.append(header::SET_COOKIE, header_value);
    }
}

fn clear_session_cookie(headers: &mut HeaderMap) {
    let value = "session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=-1";
    if let Ok(header_value) = HeaderValue::from_str(value) {
        headers.append(header::SET_COOKIE, header_value);
    }
}

#[derive(Deserialize)]
struct SignUpRequest {
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct SignInRequest {
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct EmailRequest {
    email: String,
}

#[derive(Deserialize)]
struct TokenRequest {
    token: String,
}

#[derive(Deserialize)]
struct ResetPasswordRequest {
    token: String,
    password: String,
}

#[derive(Deserialize)]
struct VerifyEmailQuery {
    token: Option<String>,
}

async fn sign_up(
    State(state): State<AppState>,
    Json(mut req): Json<SignUpRequest>,
) -> Response {
    req.email = req.email.trim().to_string();
    if req.email.is_empty() || req.password.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "email and password are required");
    }

    let conn = state.db.lock().unwrap();
    let user = match Service::create_user(&conn, &req.email, &req.password) {
        Ok(user) => user,
        Err(AuthError::EmailTaken) => {
            return json_error(StatusCode::CONFLICT, "could not create that account");
        }
        Err(_) => {
            eprintln!("signup error");
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not create that account",
            );
        }
    };

    if let Ok(vt_token) = Service::create_verification_token(&conn, &user.email) {
        let email = user.email.clone();
        let name = user.name.clone();
        send_verification_email(&email, &name, &vt_token);
    } else {
        eprintln!("create verification token error");
    }

    json_response(
        StatusCode::CREATED,
        json!({ "message": "Check your email to verify your account before signing in." }),
    )
}

async fn sign_in(
    State(state): State<AppState>,
    Json(mut req): Json<SignInRequest>,
) -> Response {
    req.email = req.email.trim().to_string();
    if req.email.is_empty() || req.password.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "email and password are required");
    }

    let conn = state.db.lock().unwrap();
    let (user, hash) = match Service::get_user_by_email(&conn, &req.email) {
        Ok(pair) => pair,
        Err(_) => {
            return json_error(StatusCode::UNAUTHORIZED, "invalid email or password");
        }
    };

    if !password::check_password(&req.password, &hash) {
        return json_error(StatusCode::UNAUTHORIZED, "invalid email or password");
    }

    if !user.email_verified {
        if let Ok(vt_token) = Service::create_verification_token(&conn, &user.email) {
            let email = user.email.clone();
            let name = user.name.clone();
            send_verification_email(&email, &name, &vt_token);
        } else {
            eprintln!("create verification token error");
        }
        return json_error(
            StatusCode::FORBIDDEN,
            "please verify your email address before signing in. we sent a new verification link",
        );
    }

    let session = match Service::create_session(&conn, &user.id) {
        Ok(session) => session,
        Err(err) => {
            eprintln!("create session error: {err}");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "could not sign in");
        }
    };

    let mut headers = HeaderMap::new();
    set_session_cookie(&mut headers, &session.token);
    let body = json!({ "user": user, "redirectTo": "/" });
    let mut response = json_response(StatusCode::OK, body);
    response.headers_mut().extend(headers);
    response
}

async fn sign_out(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = session_token(&headers) {
        let conn = state.db.lock().unwrap();
        let _ = Service::delete_session(&conn, &token);
    }

    let mut response_headers = HeaderMap::new();
    clear_session_cookie(&mut response_headers);
    let mut response = json_response(StatusCode::OK, json!({ "message": "signed out" }));
    response.headers_mut().extend(response_headers);
    response
}

async fn session(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(token) = session_token(&headers) else {
        return json_response(StatusCode::OK, json!({ "user": null }));
    };

    let conn = state.db.lock().unwrap();
    match Service::get_session(&conn, &token) {
        Ok(Some((_, user))) => json_response(StatusCode::OK, json!({ "user": user })),
        Ok(None) | Err(_) => json_response(StatusCode::OK, json!({ "user": null })),
    }
}

async fn verify_email_get(
    State(state): State<AppState>,
    Query(query): Query<VerifyEmailQuery>,
) -> Response {
    let Some(token) = query.token.filter(|t| !t.is_empty()) else {
        return json_error(StatusCode::BAD_REQUEST, "token is required");
    };
    verify_email_redirect(&state, &token).await
}

async fn verify_email_post(
    State(state): State<AppState>,
    Query(query): Query<VerifyEmailQuery>,
    body: Option<Json<TokenRequest>>,
) -> Response {
    let token = query.token.clone().filter(|t| !t.is_empty()).or_else(|| {
        body.as_ref()
            .map(|Json(req)| req.token.clone())
            .filter(|t| !t.is_empty())
    });

    let Some(token) = token else {
        return json_error(StatusCode::BAD_REQUEST, "token is required");
    };
    verify_email_redirect(&state, token.as_str()).await
}

async fn verify_email_redirect(state: &AppState, token: &str) -> Response {
    let conn = state.db.lock().unwrap();
    match Service::verify_email(&conn, token) {
        Ok(()) => Redirect::temporary("/signin?verified=1").into_response(),
        Err(AuthError::InvalidToken) | Err(AuthError::TokenExpired) => {
            Redirect::temporary("/signin?error=This+verification+link+is+invalid+or+expired")
                .into_response()
        }
        Err(_) => {
            eprintln!("verify email error");
            Redirect::temporary("/signin?error=Could+not+verify+email").into_response()
        }
    }
}

async fn forgot_password(
    State(state): State<AppState>,
    Json(mut req): Json<EmailRequest>,
) -> Response {
    req.email = req.email.trim().to_string();
    if req.email.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "email is required");
    }

    let message = "if that email exists, a password reset link has been sent";
    let conn = state.db.lock().unwrap();

    let exists = match Service::user_exists(&conn, &req.email) {
        Ok(exists) => exists,
        Err(err) => {
            eprintln!("user exists check error: {err}");
            return json_response(StatusCode::OK, json!({ "message": message }));
        }
    };

    if !exists {
        return json_response(StatusCode::OK, json!({ "message": message }));
    }

    match Service::create_verification_token(&conn, &req.email) {
        Ok(token) => {
            let email = req.email.clone();
            send_password_reset_email(&email, &email, &token);
        }
        Err(err) => eprintln!("create verification token error: {err}"),
    }

    json_response(StatusCode::OK, json!({ "message": message }))
}

async fn reset_password(
    State(state): State<AppState>,
    Json(req): Json<ResetPasswordRequest>,
) -> Response {
    if req.token.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "reset token is missing");
    }
    if req.password.is_empty() || req.password.len() < 8 {
        return json_error(
            StatusCode::BAD_REQUEST,
            "password must be at least 8 characters",
        );
    }

    let conn = state.db.lock().unwrap();
    match Service::reset_password(&conn, &req.token, &req.password) {
        Ok(()) => json_response(
            StatusCode::OK,
            json!({ "message": "your password has been reset" }),
        ),
        Err(AuthError::InvalidToken) | Err(AuthError::TokenExpired) => {
            json_error(StatusCode::BAD_REQUEST, "that reset link is invalid or expired")
        }
        Err(_) => {
            eprintln!("reset password error");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "could not reset password")
        }
    }
}

async fn health() -> Response {
    json_response(StatusCode::OK, json!({ "status": "ok" }))
}
