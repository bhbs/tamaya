import { useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { signUp } from "../lib/api";

export default function Signup() {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const callbackUrl = searchParams.get("callbackUrl");
  const queryError = searchParams.get("error");

  async function handleSubmit(e) {
    e.preventDefault();
    setError("");

    if (!email || !password) {
      setError("Email and password are required.");
      return;
    }

    setLoading(true);
    try {
      await signUp(email, password);
      const params = new URLSearchParams({ verifyEmail: "1" });
      if (callbackUrl) params.set("callbackUrl", callbackUrl);
      navigate(`/signin?${params}`, { replace: true, viewTransition: true });
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Could not create that account.",
      );
    } finally {
      setLoading(false);
    }
  }

  return (
    <main className="page-center">
      <div className="card">
        <h1 className="title">Create account</h1>

        {(queryError || error) && (
          <p className="alert-error">{queryError || error}</p>
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
            {loading ? "Creating account..." : "Sign Up"}
          </button>
        </form>

        <p className="footer">
          <Link
            to={
              callbackUrl
                ? `/signin?callbackUrl=${encodeURIComponent(callbackUrl)}`
                : "/signin"
            }
            className="link"
            viewTransition
          >
            Already have an account?
          </Link>
        </p>
      </div>
    </main>
  );
}
