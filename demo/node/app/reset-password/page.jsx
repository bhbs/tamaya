"use client";

import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { Suspense, useState } from "react";
import { resetPassword } from "../lib/api";

export default function ResetPassword() {
  return <Suspense><ResetPasswordForm /></Suspense>;
}

function ResetPasswordForm() {
  const token = useSearchParams().get("token") || "";
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [done, setDone] = useState(false);

  async function submit(event) {
    event.preventDefault();
    setError("");
    if (password.length < 8) return setError("Password must be at least 8 characters.");
    if (password !== confirm) return setError("Passwords do not match.");
    try {
      await resetPassword(token, password);
      setDone(true);
    } catch (cause) {
      setError(cause.message);
    }
  }

  if (!token) return <main className="page-center"><div className="card"><p className="subtitle">This reset link is invalid or expired. <Link className="link" href="/forgot-password">Request a new link</Link></p></div></main>;
  if (done) return <main className="page-center"><div className="card"><p className="subtitle">Your password has been reset. <Link className="link" href="/signin">Sign in</Link></p></div></main>;
  return <main className="page-center"><div className="card">
    <h1 className="title">Set new password</h1>
    {error && <p className="alert-error">{error}</p>}
    <form onSubmit={submit} className="form-stack">
      <input className="input" type="password" placeholder="New password" required value={password} onChange={(event) => setPassword(event.target.value)} />
      <input className="input" type="password" placeholder="Confirm password" required value={confirm} onChange={(event) => setConfirm(event.target.value)} />
      <button className="btn-primary">Reset password</button>
    </form>
  </div></main>;
}
