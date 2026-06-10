package auth

import (
	"fmt"
	"log"
	"os"
	"strings"
)

var (
	baseURL string
)

func InitEmail(url string) {
	baseURL = url
}

func sendEmail(to, name, subject, text string) {
	log.Printf("[EMAIL] To: %s | Subject: %s | Body:\n%s", to, subject, text)
}

func sendVerificationEmail(email, name, token string) {
	appName := "Demo"
	if tamaya := os.Getenv("APP_NAME"); tamaya != "" {
		appName = tamaya
	}

	url := fmt.Sprintf("%s/api/auth/verify-email?token=%s", strings.TrimRight(baseURL, "/"), token)
	text := strings.Join([]string{
		"Use the link below to verify your " + appName + " email address.",
		"",
		url,
		"",
		"If you did not create a " + appName + " account, you can ignore this email.",
	}, "\n")

	sendEmail(email, name, "Verify your "+appName+" email", text)
}

func sendPasswordResetEmail(email, name, token string) {
	appName := "Demo"
	if tamaya := os.Getenv("APP_NAME"); tamaya != "" {
		appName = tamaya
	}

	url := fmt.Sprintf("%s/reset-password?token=%s", strings.TrimRight(baseURL, "/"), token)
	text := strings.Join([]string{
		"Use the link below to reset your " + appName + " password.",
		"",
		url,
		"",
		"If you did not request this, you can ignore this email.",
	}, "\n")

	sendEmail(email, name, "Reset your "+appName+" password", text)
}
