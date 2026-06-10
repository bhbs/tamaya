use rusqlite::Connection;
use std::fs;
use std::path::Path;

pub fn open(database_url: &str) -> Result<Connection, String> {
    let path = database_path_from_url(database_url)?;
    eprintln!(
        "DATABASE_URL={database_url:?} dbPath={path:?} dir={:?}",
        Path::new(&path).parent()
    );

    if let Some(parent) = Path::new(&path).parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create db directory: {e}"))?;
    }

    let conn = Connection::open(&path).map_err(|e| format!("open database: {e}"))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| format!("set busy timeout: {e}"))?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| format!("set journal_mode: {e}"))?;
    conn.pragma_update(None, "foreign_keys", "on")
        .map_err(|e| format!("set foreign_keys: {e}"))?;
    migrate(&conn)?;
    Ok(conn)
}

fn database_path_from_url(database_url: &str) -> Result<String, String> {
    let Some(db_path) = database_url.strip_prefix("file:") else {
        return Err("only file: DATABASE_URL values are supported for SQLite".into());
    };
    Ok(db_path.to_string())
}

fn migrate(conn: &Connection) -> Result<(), String> {
    let statements = [
        "CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            email TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            name TEXT NOT NULL,
            email_verified INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        "CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            token TEXT NOT NULL UNIQUE,
            expires_at TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        "CREATE TABLE IF NOT EXISTS verification_tokens (
            id TEXT PRIMARY KEY,
            identifier TEXT NOT NULL,
            token TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    ];

    for stmt in statements {
        conn.execute_batch(stmt)
            .map_err(|e| format!("migration statement failed: {e}\n{stmt}"))?;
    }

    Ok(())
}
