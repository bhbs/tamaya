package auth

import (
	"database/sql"
	"fmt"
	"strings"
	"time"

	"github.com/google/uuid"
)

type User struct {
	ID            string `json:"id"`
	Email         string `json:"email"`
	Name          string `json:"name"`
	EmailVerified bool   `json:"emailVerified"`
	CreatedAt     string `json:"createdAt"`
}

type Service struct {
	db *sql.DB
}

func NewService(db *sql.DB) *Service {
	return &Service{db: db}
}

func (s *Service) CreateUser(email, password string) (*User, error) {
	hash, err := HashPassword(password)
	if err != nil {
		return nil, fmt.Errorf("hash password: %w", err)
	}

	id := uuid.New().String()
	now := time.Now().UTC().Format(time.RFC3339)

	_, err = s.db.Exec(
		`INSERT INTO users (id, email, password_hash, name, email_verified, created_at, updated_at)
		 VALUES (?, ?, ?, ?, 0, ?, ?)`,
		id, email, hash, email, now, now,
	)
	if err != nil {
		if strings.Contains(err.Error(), "UNIQUE") {
			return nil, ErrEmailTaken
		}
		return nil, fmt.Errorf("insert user: %w", err)
	}

	return &User{
		ID:            id,
		Email:         email,
		Name:          email,
		EmailVerified: false,
		CreatedAt:     now,
	}, nil
}

func (s *Service) GetUserByEmail(email string) (*User, string, error) {
	var u User
	var hash string
	var verified int

	err := s.db.QueryRow(
		`SELECT id, email, name, email_verified, password_hash, created_at
		 FROM users WHERE email = ?`, email,
	).Scan(&u.ID, &u.Email, &u.Name, &verified, &hash, &u.CreatedAt)

	if err == sql.ErrNoRows {
		return nil, "", ErrInvalidCredentials
	}
	if err != nil {
		return nil, "", fmt.Errorf("query user: %w", err)
	}

	u.EmailVerified = verified != 0
	return &u, hash, nil
}

func (s *Service) GetUserByID(id string) (*User, error) {
	var u User
	var verified int

	err := s.db.QueryRow(
		`SELECT id, email, name, email_verified, created_at
		 FROM users WHERE id = ?`, id,
	).Scan(&u.ID, &u.Email, &u.Name, &verified, &u.CreatedAt)

	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("query user: %w", err)
	}

	u.EmailVerified = verified != 0
	return &u, nil
}

func (s *Service) CreateSession(userID string) (*Session, error) {
	id := uuid.New().String()
	token := uuid.New().String()
	expiresAt := time.Now().UTC().Add(30 * 24 * time.Hour).Format(time.RFC3339)
	now := time.Now().UTC().Format(time.RFC3339)

	_, err := s.db.Exec(
		`INSERT INTO sessions (id, user_id, token, expires_at, created_at)
		 VALUES (?, ?, ?, ?, ?)`,
		id, userID, token, expiresAt, now,
	)
	if err != nil {
		return nil, fmt.Errorf("insert session: %w", err)
	}

	return &Session{
		ID:        id,
		UserID:    userID,
		Token:     token,
		ExpiresAt: expiresAt,
		CreatedAt: now,
	}, nil
}

func (s *Service) GetSession(token string) (*Session, *User, error) {
	var sess Session
	var userID string

	err := s.db.QueryRow(
		`SELECT id, user_id, token, expires_at, created_at
		 FROM sessions WHERE token = ?`, token,
	).Scan(&sess.ID, &userID, &sess.Token, &sess.ExpiresAt, &sess.CreatedAt)

	if err == sql.ErrNoRows {
		return nil, nil, nil
	}
	if err != nil {
		return nil, nil, fmt.Errorf("query session: %w", err)
	}

	expiresAt, err := time.Parse(time.RFC3339, sess.ExpiresAt)
	if err != nil || time.Now().UTC().After(expiresAt) {
		s.db.Exec(`DELETE FROM sessions WHERE token = ?`, token)
		return nil, nil, nil
	}

	sess.UserID = userID

	user, err := s.GetUserByID(userID)
	if err != nil {
		return nil, nil, err
	}

	return &sess, user, nil
}

func (s *Service) DeleteSession(token string) error {
	_, err := s.db.Exec(`DELETE FROM sessions WHERE token = ?`, token)
	return err
}

func (s *Service) RevokeUserSessions(userID string) error {
	_, err := s.db.Exec(`DELETE FROM sessions WHERE user_id = ?`, userID)
	return err
}

func (s *Service) CreateVerificationToken(identifier string) (string, error) {
	id := uuid.New().String()
	token := uuid.New().String()
	expiresAt := time.Now().UTC().Add(24 * time.Hour).Format(time.RFC3339)
	now := time.Now().UTC().Format(time.RFC3339)

	_, err := s.db.Exec(
		`INSERT INTO verification_tokens (id, identifier, token, expires_at, created_at)
		 VALUES (?, ?, ?, ?, ?)`,
		id, identifier, token, expiresAt, now,
	)
	if err != nil {
		return "", fmt.Errorf("insert verification token: %w", err)
	}

	return token, nil
}

func (s *Service) VerifyEmail(token string) error {
	var vtID, identifier string
	var expiresAtStr string

	err := s.db.QueryRow(
		`SELECT id, identifier, expires_at FROM verification_tokens WHERE token = ?`,
		token,
	).Scan(&vtID, &identifier, &expiresAtStr)

	if err == sql.ErrNoRows {
		return ErrInvalidToken
	}
	if err != nil {
		return fmt.Errorf("query verification token: %w", err)
	}

	expiresAt, err := time.Parse(time.RFC3339, expiresAtStr)
	if err != nil || time.Now().UTC().After(expiresAt) {
		s.db.Exec(`DELETE FROM verification_tokens WHERE id = ?`, vtID)
		return ErrTokenExpired
	}

	if _, err := s.db.Exec(`UPDATE users SET email_verified = 1 WHERE email = ?`, identifier); err != nil {
		return fmt.Errorf("update user verified: %w", err)
	}

	if _, err := s.db.Exec(`DELETE FROM verification_tokens WHERE id = ?`, vtID); err != nil {
		return fmt.Errorf("delete verification token: %w", err)
	}

	return nil
}

func (s *Service) ResetPassword(token, newPassword string) error {
	var vtID, identifier string
	var expiresAtStr string

	err := s.db.QueryRow(
		`SELECT id, identifier, expires_at FROM verification_tokens WHERE token = ?`,
		token,
	).Scan(&vtID, &identifier, &expiresAtStr)

	if err == sql.ErrNoRows {
		return ErrInvalidToken
	}
	if err != nil {
		return fmt.Errorf("query verification token: %w", err)
	}

	expiresAt, err := time.Parse(time.RFC3339, expiresAtStr)
	if err != nil || time.Now().UTC().After(expiresAt) {
		s.db.Exec(`DELETE FROM verification_tokens WHERE id = ?`, vtID)
		return ErrTokenExpired
	}

	hash, err := HashPassword(newPassword)
	if err != nil {
		return fmt.Errorf("hash password: %w", err)
	}

	if _, err := s.db.Exec(`UPDATE users SET password_hash = ?, updated_at = ? WHERE email = ?`,
		hash, time.Now().UTC().Format(time.RFC3339), identifier); err != nil {
		return fmt.Errorf("update password: %w", err)
	}

	if _, err := s.db.Exec(`DELETE FROM verification_tokens WHERE id = ?`, vtID); err != nil {
		return fmt.Errorf("delete verification token: %w", err)
	}

	if _, err := s.db.Exec(`DELETE FROM sessions WHERE user_id IN (SELECT id FROM users WHERE email = ?)`, identifier); err != nil {
		return fmt.Errorf("revoke sessions: %w", err)
	}

	return nil
}

func (s *Service) UserExists(email string) (bool, error) {
	var exists bool
	err := s.db.QueryRow(`SELECT EXISTS(SELECT 1 FROM users WHERE email = ?)`, email).Scan(&exists)
	return exists, err
}

type Session struct {
	ID        string `json:"id"`
	UserID    string `json:"userId"`
	Token     string `json:"token"`
	ExpiresAt string `json:"expiresAt"`
	CreatedAt string `json:"createdAt"`
}
