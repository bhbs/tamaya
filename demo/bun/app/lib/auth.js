import { getDb } from "./db";
import { sendPasswordResetEmail, sendVerificationEmail } from "./email";

const DAY = 24 * 60 * 60 * 1000;
const uuid = () => crypto.randomUUID();
const now = () => new Date().toISOString();
const json = (value, status = 200, headers) => Response.json(value, { status, headers });
const error = (message, status) => json({ error: message }, status);
const sessionCookie = (token, maxAge = 30 * 24 * 60 * 60) =>
  `session=${token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=${maxAge}`;

function redirectUrl(request, path) {
  const host = request.headers.get("x-forwarded-host") || request.headers.get("host");
  const protocol = request.headers.get("x-forwarded-proto") || new URL(request.url).protocol.slice(0, -1);
  return new URL(path, `${protocol}://${host}`);
}

function userByEmail(email) {
  return getDb().query("SELECT id, email, name, email_verified, password_hash, created_at FROM users WHERE email = ?").get(email);
}

function publicUser(user) {
  return {
    id: user.id, email: user.email, name: user.name,
    emailVerified: Boolean(user.email_verified), createdAt: user.created_at,
  };
}

function createToken(identifier) {
  const token = uuid();
  getDb().run("INSERT INTO verification_tokens VALUES (?, ?, ?, ?, ?)", [uuid(), identifier, token, new Date(Date.now() + DAY).toISOString(), now()]);
  return token;
}

function tokenRow(token) {
  const row = getDb().query("SELECT id, identifier, expires_at FROM verification_tokens WHERE token = ?").get(token);
  if (!row) return null;
  if (Date.parse(row.expires_at) < Date.now()) {
    getDb().run("DELETE FROM verification_tokens WHERE id = ?", [row.id]);
    return null;
  }
  return row;
}

async function body(request) {
  try { return await request.json(); } catch { return {}; }
}

export async function signUp(request) {
  const { email = "", password = "" } = await body(request);
  const normalizedEmail = email.trim();
  if (!normalizedEmail || !password) return error("email and password are required", 400);

  try {
    const createdAt = now();
    getDb().run("INSERT INTO users VALUES (?, ?, ?, ?, 0, ?, ?)", [uuid(), normalizedEmail, await Bun.password.hash(password), normalizedEmail, createdAt, createdAt]);
    sendVerificationEmail(normalizedEmail, normalizedEmail, createToken(normalizedEmail));
    return json({ message: "Check your email to verify your account before signing in." }, 201);
  } catch (cause) {
    if (String(cause).includes("UNIQUE")) return error("could not create that account", 409);
    console.error("signup error:", cause);
    return error("could not create that account", 500);
  }
}

export async function signIn(request) {
  const { email = "", password = "" } = await body(request);
  const normalizedEmail = email.trim();
  if (!normalizedEmail || !password) return error("email and password are required", 400);

  const user = userByEmail(normalizedEmail);
  if (!user || !(await Bun.password.verify(password, user.password_hash))) return error("invalid email or password", 401);
  if (!user.email_verified) {
    sendVerificationEmail(user.email, user.name, createToken(user.email));
    return error("please verify your email address before signing in. we sent a new verification link", 403);
  }

  const token = uuid();
  getDb().run("INSERT INTO sessions VALUES (?, ?, ?, ?, ?)", [uuid(), user.id, token, new Date(Date.now() + 30 * DAY).toISOString(), now()]);
  return json({ user: publicUser(user), redirectTo: "/" }, 200, { "set-cookie": sessionCookie(token) });
}

export function signOut(request) {
  const token = request.cookies.get("session")?.value;
  if (token) getDb().run("DELETE FROM sessions WHERE token = ?", [token]);
  return json({ message: "signed out" }, 200, { "set-cookie": sessionCookie("", -1) });
}

export function session(request) {
  const token = request.cookies.get("session")?.value;
  if (!token) return json({ user: null });
  const row = getDb().query("SELECT user_id, expires_at FROM sessions WHERE token = ?").get(token);
  if (!row || Date.parse(row.expires_at) < Date.now()) {
    if (row) getDb().run("DELETE FROM sessions WHERE token = ?", [token]);
    return json({ user: null });
  }
  const user = getDb().query("SELECT id, email, name, email_verified, created_at FROM users WHERE id = ?").get(row.user_id);
  return json({ user: user ? publicUser(user) : null });
}

export async function verifyEmail(request) {
  const url = new URL(request.url);
  const payload = request.method === "POST" ? await body(request) : {};
  const token = url.searchParams.get("token") || payload.token;
  if (!token) return error("token is required", 400);
  const row = tokenRow(token);
  const redirect = (message) => Response.redirect(redirectUrl(request, `/signin?${message}`), 303);
  if (!row) return redirect("error=This+verification+link+is+invalid+or+expired");
  getDb().run("UPDATE users SET email_verified = 1 WHERE email = ?", [row.identifier]);
  getDb().run("DELETE FROM verification_tokens WHERE id = ?", [row.id]);
  return redirect("verified=1");
}

export async function forgotPassword(request) {
  const { email = "" } = await body(request);
  const normalizedEmail = email.trim();
  if (!normalizedEmail) return error("email is required", 400);
  if (userByEmail(normalizedEmail)) sendPasswordResetEmail(normalizedEmail, normalizedEmail, createToken(normalizedEmail));
  return json({ message: "if that email exists, a password reset link has been sent" });
}

export async function resetPassword(request) {
  const { token = "", password = "" } = await body(request);
  if (!token) return error("reset token is missing", 400);
  if (password.length < 8) return error("password must be at least 8 characters", 400);
  const row = tokenRow(token);
  if (!row) return error("that reset link is invalid or expired", 400);
  getDb().run("UPDATE users SET password_hash = ?, updated_at = ? WHERE email = ?", [await Bun.password.hash(password), now(), row.identifier]);
  getDb().run("DELETE FROM verification_tokens WHERE id = ?", [row.id]);
  getDb().run("DELETE FROM sessions WHERE user_id IN (SELECT id FROM users WHERE email = ?)", [row.identifier]);
  return json({ message: "your password has been reset" });
}
