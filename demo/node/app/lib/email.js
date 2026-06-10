function sendEmail(to, name, subject, text) {
  console.log(`[EMAIL] To: ${to} | Subject: ${subject} | Body:\n${text}`);
}

export function sendVerificationEmail(email, name, token) {
  const appName = process.env.APP_NAME || "Demo";
  const baseUrl = (process.env.BASE_URL || "http://localhost:3000").replace(/\/$/, "");
  const url = `${baseUrl}/api/auth/verify-email?token=${token}`;
  sendEmail(email, name, `Verify your ${appName} email`, `Use the link below to verify your ${appName} email address.\n\n${url}\n\nIf you did not create a ${appName} account, you can ignore this email.`);
}

export function sendPasswordResetEmail(email, name, token) {
  const appName = process.env.APP_NAME || "Demo";
  const baseUrl = (process.env.BASE_URL || "http://localhost:3000").replace(/\/$/, "");
  const url = `${baseUrl}/reset-password?token=${token}`;
  sendEmail(email, name, `Reset your ${appName} password`, `Use the link below to reset your ${appName} password.\n\n${url}\n\nIf you did not request this, you can ignore this email.`);
}
