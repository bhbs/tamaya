package auth

import "errors"

var (
	ErrEmailTaken        = errors.New("email already taken")
	ErrInvalidCredentials = errors.New("invalid email or password")
	ErrEmailNotVerified  = errors.New("email not verified")
	ErrInvalidToken      = errors.New("invalid token")
	ErrTokenExpired      = errors.New("token expired")
)
