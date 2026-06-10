import { useState } from "react";
import { Link } from "react-router-dom";
import { forgotPassword } from "../lib/api";

export default function ForgotPassword() {
  const [email, setEmail] = useState("");
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [loading, setLoading] = useState(false);

  async function handleSubmit(e) {
    e.preventDefault();
    setError("");
    setMessage("");

    if (!email) {
      setError("Email is required.");
      return;
    }

    setLoading(true);
    try {
      await forgotPassword(email);
      setMessage("If that email exists, a password reset link has been sent.");
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Could not send a reset link.",
      );
    } finally {
      setLoading(false);
    }
  }

  return (
    <main className="page-center">
      <div className="card">
        <h1 className="title">Reset password</h1>

        {error && (
          <p className="alert-error">{error}</p>
        )}

        {message && (
          <p className="alert-success">{message}</p>
        )}

        <form onSubmit={handleSubmit} className="form-stack">
          <div>
            <input
              name="email"
              type="email"
              placeholder="Email"
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              className="input"
            />
          </div>
          <button
            type="submit"
            disabled={loading}
            className="btn-primary"
          >
            {loading ? "Sending..." : "Send reset link"}
          </button>
        </form>

        <p className="footer">
          <Link to="/signin" className="link" viewTransition>
            Back to sign in
          </Link>
        </p>
      </div>
    </main>
  );
}
