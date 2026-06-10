pub const AuthError = error{
    EmailTaken,
    InvalidCredentials,
    EmailNotVerified,
    InvalidToken,
    TokenExpired,
};
