package db

import (
	"database/sql"
	"log"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	_ "modernc.org/sqlite"
)

func databasePathFromURL(databaseURL string) (string, error) {
	if !strings.HasPrefix(databaseURL, "file:") {
		return "", fmt.Errorf("only file: DATABASE_URL values are supported for SQLite")
	}

	dbPath := strings.TrimPrefix(databaseURL, "file:")
	log.Printf("DATABASE_URL=%q dbPath=%q dir=%q", databaseURL, dbPath, filepath.Dir(dbPath))

	return dbPath, nil
}

func Open(databaseURL string) (*sql.DB, error) {
	path, err := databasePathFromURL(databaseURL)
	if err != nil {
		return nil, err
	}

	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return nil, fmt.Errorf("create db directory: %w", err)
	}

	db, err := sql.Open("sqlite", path+"?_journal_mode=WAL&_foreign_keys=on")
	if err != nil {
		return nil, fmt.Errorf("open database: %w", err)
	}

	db.SetMaxOpenConns(1)

	if err := migrate(db); err != nil {
		db.Close()
		return nil, fmt.Errorf("migrate database: %w", err)
	}

	return db, nil
}

func migrate(db *sql.DB) error {
	statements := []string{
		`CREATE TABLE IF NOT EXISTS users (
			id TEXT PRIMARY KEY,
			email TEXT NOT NULL UNIQUE,
			password_hash TEXT NOT NULL,
			name TEXT NOT NULL,
			email_verified INTEGER NOT NULL DEFAULT 0,
			created_at TEXT NOT NULL DEFAULT (datetime('now')),
			updated_at TEXT NOT NULL DEFAULT (datetime('now'))
		)`,
		`CREATE TABLE IF NOT EXISTS sessions (
			id TEXT PRIMARY KEY,
			user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
			token TEXT NOT NULL UNIQUE,
			expires_at TEXT NOT NULL,
			created_at TEXT NOT NULL DEFAULT (datetime('now'))
		)`,
		`CREATE TABLE IF NOT EXISTS verification_tokens (
			id TEXT PRIMARY KEY,
			identifier TEXT NOT NULL,
			token TEXT NOT NULL,
			expires_at TEXT NOT NULL,
			created_at TEXT NOT NULL DEFAULT (datetime('now'))
		)`,
	}

	for _, stmt := range statements {
		if _, err := db.Exec(stmt); err != nil {
			return fmt.Errorf("migration statement failed: %w\n%s", err, stmt)
		}
	}

	return nil
}
