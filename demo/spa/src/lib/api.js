const API_BASE = "/api/auth";

async function request(path, body) {
  const res = await fetch(`${API_BASE}${path}`, {
    method: body ? "POST" : "GET",
    headers: body ? { "Content-Type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });

  const data = await res.json();

  if (!res.ok) {
    throw new Error(data.error || "Something went wrong");
  }

  return data;
}

export function getSession() {
  return request("/session");
}

export function signUp(email, password) {
  return request("/signup", { email, password });
}

export function signIn(email, password) {
  return request("/signin", { email, password });
}

export function signOut() {
  return request("/signout", {});
}

export function verifyEmail(token) {
  return request("/verify-email", { token });
}

export function forgotPassword(email) {
  return request("/forgot-password", { email });
}

export function resetPassword(token, password) {
  return request("/reset-password", { token, password });
}
