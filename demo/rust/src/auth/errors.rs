#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    EmailTaken,
    InvalidCredentials,
    InvalidToken,
    TokenExpired,
}
