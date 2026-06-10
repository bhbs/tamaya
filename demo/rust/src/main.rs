mod auth;
mod config;
mod db;

use auth::{AppState, init_email, routes};
use axum::{
    Router,
    body::Body,
    http::{StatusCode, Uri, header},
    response::Response,
};
use rust_embed::Embed;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Embed)]
#[folder = "static/"]
struct StaticAssets;

#[tokio::main]
async fn main() {
    let cfg = config::Config::load();
    init_email(&cfg.base_url);

    let conn = db::open(&cfg.database_url).unwrap_or_else(|e| {
        eprintln!("open database: {e}");
        std::process::exit(1);
    });

    let auth_state = AppState {
        db: Arc::new(Mutex::new(conn)),
    };

    let app = Router::new()
        .merge(routes(auth_state))
        .fallback(|uri: Uri| async move { serve_static(uri).await });

    let port: u16 = std::env::var("PORT")
        .ok()
        .or(Some(cfg.port))
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("cannot bind to {addr}: {e}"));

    eprintln!("Demo server starting on :{port} (embedded static)");
    axum::serve(listener, app)
        .await
        .unwrap_or_else(|e| panic!("server error: {e}"));
}

async fn serve_static(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        return serve_file("index.html").await;
    }

    let has_extension = Path::new(path)
        .extension()
        .is_some_and(|ext| !ext.is_empty());
    if !has_extension {
        return serve_file("index.html").await;
    }

    serve_file(path).await
}

async fn serve_file(name: &str) -> Response {
    let asset = StaticAssets::get(name).or_else(|| {
        if name == "index.html" {
            None
        } else {
            StaticAssets::get("index.html")
        }
    });

    let Some(content) = asset else {
        return not_found();
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content.metadata.mimetype())
        .body(Body::from(content.data.into_owned()))
        .unwrap()
}

fn not_found() -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap()
}
