package main

import (
	"embed"
	"io/fs"
	"log"
	"net/http"
	"os"
	"strings"

	"github.com/demo/internal/auth"
	"github.com/demo/internal/config"
	"github.com/demo/internal/db"
)

//go:embed all:static
var staticEmbed embed.FS

func main() {
	cfg := config.Load()

	auth.InitEmail(cfg.BaseURL)

	database, err := db.Open(cfg.DatabaseURL)
	if err != nil {
		log.Fatalf("open database: %v", err)
	}
	defer database.Close()

	authSvc := auth.NewService(database)
	authHandler := auth.NewHandler(authSvc)

	staticFS, err := fs.Sub(staticEmbed, "static")
	if err != nil {
		log.Fatalf("static embed: %v", err)
	}

	mux := http.NewServeMux()

	authHandler.RegisterRoutes(mux)

	mux.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		path := strings.TrimPrefix(r.URL.Path, "/")
		f, err := staticFS.Open(path)
		if err != nil {
			r.URL.Path = "/"
			http.FileServer(http.FS(staticFS)).ServeHTTP(w, r)
			return
		}
		f.Close()
		http.FileServer(http.FS(staticFS)).ServeHTTP(w, r)
	})

	port := cfg.Port
	if envPort := os.Getenv("PORT"); envPort != "" {
		port = envPort
	}

	log.Printf("Demo server starting on :%s (embedded static)", port)
	if err := http.ListenAndServe(":"+port, mux); err != nil {
		log.Fatalf("server error: %v", err)
	}
}
