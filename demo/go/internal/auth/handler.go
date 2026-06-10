package auth

import (
	"encoding/json"
	"log"
	"net/http"
	"strings"
)

type Handler struct {
	svc *Service
}

func NewHandler(svc *Service) *Handler {
	return &Handler{svc: svc}
}

func (h *Handler) RegisterRoutes(mux *http.ServeMux) {
	mux.HandleFunc("/api/auth/signup", h.handleSignUp)
	mux.HandleFunc("/api/auth/signin", h.handleSignIn)
	mux.HandleFunc("/api/auth/signout", h.handleSignOut)
	mux.HandleFunc("/api/auth/session", h.handleSession)
	mux.HandleFunc("/api/auth/verify-email", h.handleVerifyEmail)
	mux.HandleFunc("/api/auth/forgot-password", h.handleForgotPassword)
	mux.HandleFunc("/api/auth/reset-password", h.handleResetPassword)
	mux.HandleFunc("/health", h.handleHealth)
}

func writeJSON(w http.ResponseWriter, status int, tamaya interface{}) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(tamaya)
}

func writeError(w http.ResponseWriter, status int, message string) {
	writeJSON(w, status, map[string]string{"error": message})
}

func getSessionToken(r *http.Request) string {
	cookie, err := r.Cookie("session")
	if err != nil {
		return ""
	}
	return cookie.Value
}

func setSessionCookie(w http.ResponseWriter, token string) {
	http.SetCookie(w, &http.Cookie{
		Name:     "session",
		Value:    token,
		Path:     "/",
		HttpOnly: true,
		Secure:   false,
		SameSite: http.SameSiteLaxMode,
		MaxAge:   30 * 24 * 60 * 60,
	})
}

func clearSessionCookie(w http.ResponseWriter) {
	http.SetCookie(w, &http.Cookie{
		Name:     "session",
		Value:    "",
		Path:     "/",
		HttpOnly: true,
		Secure:   false,
		SameSite: http.SameSiteLaxMode,
		MaxAge:   -1,
	})
}

type signUpRequest struct {
	Email    string `json:"email"`
	Password string `json:"password"`
}

type signInRequest struct {
	Email    string `json:"email"`
	Password string `json:"password"`
}

type emailRequest struct {
	Email string `json:"email"`
}

type tokenRequest struct {
	Token string `json:"token"`
}

type resetPasswordRequest struct {
	Token    string `json:"token"`
	Password string `json:"password"`
}

func (h *Handler) handleSignUp(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		writeError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}

	var req signUpRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, "invalid request body")
		return
	}

	req.Email = strings.TrimSpace(req.Email)
	if req.Email == "" || req.Password == "" {
		writeError(w, http.StatusBadRequest, "email and password are required")
		return
	}

	user, err := h.svc.CreateUser(req.Email, req.Password)
	if err != nil {
		if err == ErrEmailTaken {
			writeError(w, http.StatusConflict, "could not create that account")
			return
		}
		log.Printf("signup error: %v", err)
		writeError(w, http.StatusInternalServerError, "could not create that account")
		return
	}

	vtToken, err := h.svc.CreateVerificationToken(user.Email)
	if err != nil {
		log.Printf("create verification token error: %v", err)
	} else {
		go sendVerificationEmail(user.Email, user.Name, vtToken)
	}

	writeJSON(w, http.StatusCreated, map[string]string{
		"message": "Check your email to verify your account before signing in.",
	})
}

func (h *Handler) handleSignIn(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		writeError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}

	var req signInRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, "invalid request body")
		return
	}

	req.Email = strings.TrimSpace(req.Email)
	if req.Email == "" || req.Password == "" {
		writeError(w, http.StatusBadRequest, "email and password are required")
		return
	}

	user, hash, err := h.svc.GetUserByEmail(req.Email)
	if err != nil || !CheckPassword(req.Password, hash) {
		writeError(w, http.StatusUnauthorized, "invalid email or password")
		return
	}

	if !user.EmailVerified {
		vtToken, err := h.svc.CreateVerificationToken(user.Email)
		if err != nil {
			log.Printf("create verification token error: %v", err)
		} else {
			go sendVerificationEmail(user.Email, user.Name, vtToken)
		}
		writeError(w, http.StatusForbidden, "please verify your email address before signing in. we sent a new verification link")
		return
	}

	session, err := h.svc.CreateSession(user.ID)
	if err != nil {
		log.Printf("create session error: %v", err)
		writeError(w, http.StatusInternalServerError, "could not sign in")
		return
	}

	setSessionCookie(w, session.Token)

	writeJSON(w, http.StatusOK, map[string]interface{}{
		"user":       user,
		"redirectTo": "/",
	})
}

func (h *Handler) handleSignOut(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		writeError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}

	token := getSessionToken(r)
	if token != "" {
		h.svc.DeleteSession(token)
	}

	clearSessionCookie(w)
	writeJSON(w, http.StatusOK, map[string]string{"message": "signed out"})
}

func (h *Handler) handleSession(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		writeError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}

	token := getSessionToken(r)
	if token == "" {
		writeJSON(w, http.StatusOK, map[string]interface{}{"user": nil})
		return
	}

	_, user, err := h.svc.GetSession(token)
	if err != nil || user == nil {
		writeJSON(w, http.StatusOK, map[string]interface{}{"user": nil})
		return
	}

	writeJSON(w, http.StatusOK, map[string]interface{}{"user": user})
}

func (h *Handler) handleVerifyEmail(w http.ResponseWriter, r *http.Request) {
	token := r.URL.Query().Get("token")

	if r.Method == http.MethodPost && token == "" {
		var req tokenRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			writeError(w, http.StatusBadRequest, "invalid request body")
			return
		}
		token = req.Token
	}

	if token == "" {
		writeError(w, http.StatusBadRequest, "token is required")
		return
	}

	if err := h.svc.VerifyEmail(token); err != nil {
		if err == ErrInvalidToken || err == ErrTokenExpired {
			http.Redirect(w, r, "/signin?error=This+verification+link+is+invalid+or+expired", http.StatusSeeOther)
			return
		}
		log.Printf("verify email error: %v", err)
		http.Redirect(w, r, "/signin?error=Could+not+verify+email", http.StatusSeeOther)
		return
	}

	http.Redirect(w, r, "/signin?verified=1", http.StatusSeeOther)
}

func (h *Handler) handleForgotPassword(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		writeError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}

	var req emailRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, "invalid request body")
		return
	}

	req.Email = strings.TrimSpace(req.Email)
	if req.Email == "" {
		writeError(w, http.StatusBadRequest, "email is required")
		return
	}

	exists, err := h.svc.UserExists(req.Email)
	if err != nil {
		log.Printf("user exists check error: %v", err)
		writeJSON(w, http.StatusOK, map[string]string{
			"message": "if that email exists, a password reset link has been sent",
		})
		return
	}

	if !exists {
		writeJSON(w, http.StatusOK, map[string]string{
			"message": "if that email exists, a password reset link has been sent",
		})
		return
	}

	token, err := h.svc.CreateVerificationToken(req.Email)
	if err != nil {
		log.Printf("create verification token error: %v", err)
		writeJSON(w, http.StatusOK, map[string]string{
			"message": "if that email exists, a password reset link has been sent",
		})
		return
	}

	go sendPasswordResetEmail(req.Email, req.Email, token)

	writeJSON(w, http.StatusOK, map[string]string{
		"message": "if that email exists, a password reset link has been sent",
	})
}

func (h *Handler) handleResetPassword(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		writeError(w, http.StatusMethodNotAllowed, "method not allowed")
		return
	}

	var req resetPasswordRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, "invalid request body")
		return
	}

	if req.Token == "" {
		writeError(w, http.StatusBadRequest, "reset token is missing")
		return
	}

	if req.Password == "" || len(req.Password) < 8 {
		writeError(w, http.StatusBadRequest, "password must be at least 8 characters")
		return
	}

	if err := h.svc.ResetPassword(req.Token, req.Password); err != nil {
		if err == ErrInvalidToken || err == ErrTokenExpired {
			writeError(w, http.StatusBadRequest, "that reset link is invalid or expired")
			return
		}
		log.Printf("reset password error: %v", err)
		writeError(w, http.StatusInternalServerError, "could not reset password")
		return
	}

	writeJSON(w, http.StatusOK, map[string]string{
		"message": "your password has been reset",
	})
}

func (h *Handler) handleHealth(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
}
