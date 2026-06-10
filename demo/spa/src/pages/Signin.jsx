import { useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { signIn } from "../lib/api";

export default function Signin() {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const verifyEmail = searchParams.get("verifyEmail");
  const verified = searchParams.get("verified");
  const callbackUrl = searchParams.get("callbackUrl");

  const message = searchParams.get("error")
    ? searchParams.get("error")
    : verifyEmail
      ? "Check your email to verify your account before signing in."
      : verified
        ? "Your email has been verified. You can sign in now."
        : undefined;

  async function handleSubmit(e) {
    e.preventDefault();
    setError("");

    if (!email || !password) {
      setError("Email and password are required.");
      return;
    }

    setLoading(true);
    try {
      await signIn(email, password);
      const redirectTo =
        callbackUrl && callbackUrl.startsWith("/") ? callbackUrl : "/";
      navigate(redirectTo, { replace: true, viewTransition: true });
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Invalid email or password.",
      );
    } finally {
      setLoading(false);
    }
  }

  return (
    <main className="page-center">
      <div className="card">
        <h1 className="title">Sign in</h1>

        {message && (
          <p className="alert-info">{message}</p>
        )}

        {error && (
          <p className="alert-error">{error}</p>
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
          <div>
            <input
              name="password"
              type="password"
              placeholder="Password"
              required
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="input"
            />
          </div>
          <button
            type="submit"
            disabled={loading}
            className="btn-primary"
          >
            {loading ? "Signing in..." : "Sign In"}
          </button>
        </form>

        <div className="footer-stack">
          <p>
            <Link to="/forgot-password" className="link" viewTransition>
              Forgot your password?
            </Link>
          </p>
          <p>
            <Link
              to={
                callbackUrl
                  ? `/signup?callbackUrl=${encodeURIComponent(callbackUrl)}`
                  : "/signup"
              }
              className="link"
              viewTransition
            >
              Create an account
            </Link>
          </p>
        </div>
      </div>
    </main>
  );
}
