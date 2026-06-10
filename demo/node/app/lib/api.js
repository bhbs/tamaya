const API_BASE = "/api/auth";

async function request(path, body) {
  const response = await fetch(`${API_BASE}${path}`, {
    method: body ? "POST" : "GET",
    headers: body ? { "content-type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
  const data = await response.json();
  if (!response.ok) throw new Error(data.error || "Something went wrong");
  return data;
}

export const getSession = () => request("/session");
export const signUp = (email, password) => request("/signup", { email, password });
export const signIn = (email, password) => request("/signin", { email, password });
export const signOut = () => request("/signout", {});
export const forgotPassword = (email) => request("/forgot-password", { email });
export const resetPassword = (token, password) => request("/reset-password", { token, password });
