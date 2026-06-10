use super::errors::AuthError;
use super::password;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub email: String,
    pub name: String,
    pub email_verified: bool,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct Session {
    #[allow(dead_code)]
    pub id: String,
    pub user_id: String,
    pub token: String,
    pub expires_at: String,
    #[allow(dead_code)]
    pub created_at: String,
}

pub struct Service;

impl Service {
    pub fn create_user(
        conn: &Connection,
        email: &str,
        password: &str,
    ) -> Result<User, AuthError> {
        let hash = password::hash_password(password).map_err(|_| AuthError::InvalidCredentials)?;
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        let result = conn.execute(
            "INSERT INTO users (id, email, password_hash, name, email_verified, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
            (&id, email, &hash, email, &now, &now),
        );

        match result {
            Ok(_) => Ok(User {
                id,
                email: email.to_string(),
                name: email.to_string(),
                email_verified: false,
                created_at: now,
            }),
            Err(err) if err.to_string().contains("UNIQUE") => Err(AuthError::EmailTaken),
            Err(_) => Err(AuthError::InvalidCredentials),
        }
    }

    pub fn get_user_by_email(
        conn: &Connection,
        email: &str,
    ) -> Result<(User, String), AuthError> {
        let mut stmt = conn
            .prepare(
                "SELECT id, email, name, email_verified, password_hash, created_at
                 FROM users WHERE email = ?1",
            )
            .map_err(|_| AuthError::InvalidCredentials)?;

        let row = stmt
            .query_row([email], |row| {
                let verified: i64 = row.get(3)?;
                Ok((
                    User {
                        id: row.get(0)?,
                        email: row.get(1)?,
                        name: row.get(2)?,
                        email_verified: verified != 0,
                        created_at: row.get(5)?,
                    },
                    row.get::<_, String>(4)?,
                ))
            })
            .optional()
            .map_err(|_| AuthError::InvalidCredentials)?;

        row.ok_or(AuthError::InvalidCredentials)
    }

    pub fn get_user_by_id(conn: &Connection, id: &str) -> Result<Option<User>, String> {
        let mut stmt = conn
            .prepare(
                "SELECT id, email, name, email_verified, created_at
                 FROM users WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;

        stmt.query_row([id], |row| {
            let verified: i64 = row.get(3)?;
            Ok(User {
                id: row.get(0)?,
                email: row.get(1)?,
                name: row.get(2)?,
                email_verified: verified != 0,
                created_at: row.get(4)?,
            })
        })
        .optional()
        .map_err(|e| e.to_string())
    }

    pub fn create_session(conn: &Connection, user_id: &str) -> Result<Session, String> {
        let id = Uuid::new_v4().to_string();
        let token = Uuid::new_v4().to_string();
        let expires_at = (Utc::now() + chrono::Duration::days(30)).to_rfc3339();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO sessions (id, user_id, token, expires_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (&id, user_id, &token, &expires_at, &now),
        )
        .map_err(|e| e.to_string())?;

        Ok(Session {
            id,
            user_id: user_id.to_string(),
            token,
            expires_at,
            created_at: now,
        })
    }

    pub fn get_session(
        conn: &Connection,
        token: &str,
    ) -> Result<Option<(Session, User)>, String> {
        let mut stmt = conn
            .prepare(
                "SELECT id, user_id, token, expires_at, created_at
                 FROM sessions WHERE token = ?1",
            )
            .map_err(|e| e.to_string())?;

        let sess = stmt
            .query_row([token], |row| {
                Ok(Session {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    token: row.get(2)?,
                    expires_at: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .optional()
            .map_err(|e| e.to_string())?;

        let Some(sess) = sess else {
            return Ok(None);
        };

        let expires_at = DateTime::parse_from_rfc3339(&sess.expires_at)
            .map(|dt| dt.with_timezone(&Utc));
        if expires_at.is_err() || Utc::now() > expires_at.unwrap() {
            let _ = conn.execute("DELETE FROM sessions WHERE token = ?1", [token]);
            return Ok(None);
        }

        let user = Self::get_user_by_id(conn, &sess.user_id)?;
        let Some(user) = user else {
            return Ok(None);
        };

        Ok(Some((sess, user)))
    }

    pub fn delete_session(conn: &Connection, token: &str) -> Result<(), String> {
        conn.execute("DELETE FROM sessions WHERE token = ?1", [token])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn create_verification_token(conn: &Connection, identifier: &str) -> Result<String, String> {
        let id = Uuid::new_v4().to_string();
        let token = Uuid::new_v4().to_string();
        let expires_at = (Utc::now() + chrono::Duration::hours(24)).to_rfc3339();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO verification_tokens (id, identifier, token, expires_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (&id, identifier, &token, &expires_at, &now),
        )
        .map_err(|e| e.to_string())?;

        Ok(token)
    }

    pub fn verify_email(conn: &Connection, token: &str) -> Result<(), AuthError> {
        let row = conn
            .query_row(
                "SELECT id, identifier, expires_at FROM verification_tokens WHERE token = ?1",
                [token],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| AuthError::InvalidToken)?;

        let Some((vt_id, identifier, expires_at_str)) = row else {
            return Err(AuthError::InvalidToken);
        };

        let expires_at = DateTime::parse_from_rfc3339(&expires_at_str)
            .map(|dt| dt.with_timezone(&Utc));
        if expires_at.is_err() || Utc::now() > expires_at.unwrap() {
            let _ = conn.execute(
                "DELETE FROM verification_tokens WHERE id = ?1",
                [&vt_id],
            );
            return Err(AuthError::TokenExpired);
        }

        conn.execute(
            "UPDATE users SET email_verified = 1 WHERE email = ?1",
            [&identifier],
        )
        .map_err(|_| AuthError::InvalidToken)?;
        conn.execute(
            "DELETE FROM verification_tokens WHERE id = ?1",
            [&vt_id],
        )
        .map_err(|_| AuthError::InvalidToken)?;

        Ok(())
    }

    pub fn reset_password(
        conn: &Connection,
        token: &str,
        new_password: &str,
    ) -> Result<(), AuthError> {
        let row = conn
            .query_row(
                "SELECT id, identifier, expires_at FROM verification_tokens WHERE token = ?1",
                [token],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| AuthError::InvalidToken)?;

        let Some((vt_id, identifier, expires_at_str)) = row else {
            return Err(AuthError::InvalidToken);
        };

        let expires_at = DateTime::parse_from_rfc3339(&expires_at_str)
            .map(|dt| dt.with_timezone(&Utc));
        if expires_at.is_err() || Utc::now() > expires_at.unwrap() {
            let _ = conn.execute(
                "DELETE FROM verification_tokens WHERE id = ?1",
                [&vt_id],
            );
            return Err(AuthError::TokenExpired);
        }

        let hash = password::hash_password(new_password).map_err(|_| AuthError::InvalidToken)?;
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE users SET password_hash = ?1, updated_at = ?2 WHERE email = ?3",
            (&hash, &now, &identifier),
        )
        .map_err(|_| AuthError::InvalidToken)?;
        conn.execute(
            "DELETE FROM verification_tokens WHERE id = ?1",
            [&vt_id],
        )
        .map_err(|_| AuthError::InvalidToken)?;
        conn.execute(
            "DELETE FROM sessions WHERE user_id IN (SELECT id FROM users WHERE email = ?1)",
            [&identifier],
        )
        .map_err(|_| AuthError::InvalidToken)?;

        Ok(())
    }

    pub fn user_exists(conn: &Connection, email: &str) -> Result<bool, String> {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM users WHERE email = ?1)",
            [email],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())
    }
}
