import { useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { resetPassword } from "../lib/api";

export default function ResetPassword() {
  const [searchParams] = useSearchParams();
  const token = searchParams.get("token") || "";
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [loading, setLoading] = useState(false);

  async function handleSubmit(e) {
    e.preventDefault();
    setError("");

    if (!token) {
      setError("Reset token is missing.");
      return;
    }

    if (!password || password.length < 8) {
      setError("Password must be at least 8 characters.");
      return;
    }

    if (password !== confirmPassword) {
      setError("Passwords do not match.");
      return;
    }

    setLoading(true);
    try {
      await resetPassword(token, password);
      setMessage("Your password has been reset.");
    } catch (err) {
      setError(
        err instanceof Error
          ? err.message
          : "That reset link is invalid or expired.",
      );
    } finally {
      setLoading(false);
    }
  }

  if (!token && !message) {
    return (
      <main className="page-center">
        <div className="card card-center">
          <h1 style={{ marginBottom: "1rem", fontSize: "1.5rem", fontWeight: 700, color: "#111827" }}>
            Invalid link
          </h1>
          <p className="subtitle">
            This reset link is invalid or expired.{" "}
            <Link to="/forgot-password" className="link" viewTransition>
              Request a new link
            </Link>
          </p>
        </div>
      </main>
    );
  }

  if (message) {
    return (
      <main className="page-center">
        <div className="card card-center">
          <h1 style={{ marginBottom: "1rem", fontSize: "1.5rem", fontWeight: 700, color: "#111827" }}>
            Password reset
          </h1>
          <p className="subtitle">
            Your password has been reset.{" "}
            <Link to="/signin" className="link" viewTransition>
              Sign in
            </Link>
          </p>
        </div>
      </main>
    );
  }

  return (
    <main className="page-center">
      <div className="card">
        <h1 className="title">Set new password</h1>

        {error && (
          <p className="alert-error">{error}</p>
        )}

        <form onSubmit={handleSubmit} className="form-stack">
          <input name="token" type="hidden" value={token} />
          <div>
            <input
              name="password"
              type="password"
              placeholder="New password"
              required
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="input"
            />
          </div>
          <div>
            <input
              name="confirmPassword"
              type="password"
              placeholder="Confirm password"
              required
              value={confirmPassword}
              onChange={(e) => setConfirmPassword(e.target.value)}
              className="input"
            />
          </div>
          <button
            type="submit"
            disabled={loading}
            className="btn-primary"
          >
            {loading ? "Resetting..." : "Reset password"}
          </button>
        </form>
      </div>
    </main>
  );
}
