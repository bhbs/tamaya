package config

import (
	"os"
	"path/filepath"
)

type Config struct {
	DatabaseURL     string
	BaseURL         string
	Port            string
	SessionSecret   string
}

func Load() *Config {
	return &Config{
		DatabaseURL:     getEnv("DATABASE_URL", defaultDatabaseURL()),
		BaseURL:         getEnv("BASE_URL", "http://localhost:8080"),
		Port:            getEnv("PORT", "8080"),
		SessionSecret:   getEnv("SESSION_SECRET", "change-me-in-production"),
	}
}

func defaultDatabaseURL() string {
	if dataDir := os.Getenv("TAMAYA_DATA_DIR"); dataDir != "" {
		return "file:" + filepath.Join(dataDir, "demo.db")
	}
	return "file:./demo.db"
}

func getEnv(key, fallback string) string {
	if tamaya := os.Getenv(key); tamaya != "" {
		return tamaya
	}
	return fallback
}
