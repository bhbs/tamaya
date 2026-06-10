use std::{env, path::Path};

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub base_url: String,
    pub port: String,
    #[allow(dead_code)]
    pub session_secret: String,
}

impl Config {
    pub fn load() -> Self {
        Self {
            database_url: env_or("DATABASE_URL", &default_database_url()),
            base_url: env_or("BASE_URL", "http://localhost:8080"),
            port: env_or("PORT", "8080"),
            session_secret: env_or("SESSION_SECRET", "change-me-in-production"),
        }
    }
}

fn default_database_url() -> String {
    env::var("TAMAYA_DATA_DIR")
        .map(|dir| format!("file:{}", Path::new(&dir).join("demo.db").display()))
        .unwrap_or_else(|_| "file:./demo.db".to_string())
}

fn env_or(key: &str, fallback: &str) -> String {
    env::var(key).unwrap_or_else(|_| fallback.to_string())
}
