"use client";

import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { Suspense, useState } from "react";
import { signIn } from "../lib/api";

export default function Signin() {
  return <Suspense><SigninForm /></Suspense>;
}

function SigninForm() {
  const router = useRouter();
  const params = useSearchParams();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const message = params.get("error") || (params.get("verifyEmail") ? "Check your email to verify your account before signing in." : params.get("verified") ? "Your email has been verified. You can sign in now." : "");

  async function submit(event) {
    event.preventDefault();
    setError("");
    setLoading(true);
    try {
      await signIn(email, password);
      const callbackUrl = params.get("callbackUrl");
      router.replace(callbackUrl?.startsWith("/") ? callbackUrl : "/");
    } catch (cause) {
      setError(cause.message);
    } finally {
      setLoading(false);
    }
  }

  return <main className="page-center"><div className="card">
    <h1 className="title">Sign in</h1>
    {message && <p className="alert-info">{message}</p>}
    {error && <p className="alert-error">{error}</p>}
    <form onSubmit={submit} className="form-stack">
      <input className="input" type="email" placeholder="Email" required value={email} onChange={(event) => setEmail(event.target.value)} />
      <input className="input" type="password" placeholder="Password" required value={password} onChange={(event) => setPassword(event.target.value)} />
      <button className="btn-primary" disabled={loading}>{loading ? "Signing in..." : "Sign In"}</button>
    </form>
    <div className="footer-stack"><Link className="link" href="/forgot-password">Forgot your password?</Link><Link className="link" href="/signup">Create an account</Link></div>
  </div></main>;
}
