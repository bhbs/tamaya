use std::env;
use std::sync::OnceLock;

static BASE_URL: OnceLock<String> = OnceLock::new();

pub fn init_email(base_url: &str) {
    let _ = BASE_URL.set(base_url.to_string());
}

pub fn send_verification_email(email: &str, name: &str, token: &str) {
    let app_name = env::var("APP_NAME").unwrap_or_else(|_| "Demo".into());
    let base_url = BASE_URL
        .get()
        .map(|c| c.trim_end_matches('/').to_string())
        .unwrap_or_default();
    let url = format!("{base_url}/api/auth/verify-email?token={token}");
    let text = format!(
        "Use the link below to verify your {app_name} email address.\n\n{url}\n\nIf you did not create a {app_name} account, you can ignore this email."
    );
    send_email(email, name, &format!("Verify your {app_name} email"), &text);
}

pub fn send_password_reset_email(email: &str, name: &str, token: &str) {
    let app_name = env::var("APP_NAME").unwrap_or_else(|_| "Demo".into());
    let base_url = BASE_URL
        .get()
        .map(|c| c.trim_end_matches('/').to_string())
        .unwrap_or_default();
    let url = format!("{base_url}/reset-password?token={token}");
    let text = format!(
        "Use the link below to reset your {app_name} password.\n\n{url}\n\nIf you did not request this, you can ignore this email."
    );
    send_email(email, name, &format!("Reset your {app_name} password"), &text);
}

fn send_email(to: &str, name: &str, subject: &str, text: &str) {
    eprintln!("[EMAIL] To: {to} | Subject: {subject} | Body:\n{text}");
    let _ = name;
}
