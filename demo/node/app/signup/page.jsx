"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useState } from "react";
import { signUp } from "../lib/api";

export default function Signup() {
  const router = useRouter();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  async function submit(event) {
    event.preventDefault();
    setError("");
    setLoading(true);
    try {
      await signUp(email, password);
      router.replace("/signin?verifyEmail=1");
    } catch (cause) {
      setError(cause.message);
    } finally {
      setLoading(false);
    }
  }

  return <main className="page-center"><div className="card">
    <h1 className="title">Create account</h1>
    {error && <p className="alert-error">{error}</p>}
    <form onSubmit={submit} className="form-stack">
      <input className="input" type="email" placeholder="Email" required value={email} onChange={(event) => setEmail(event.target.value)} />
      <input className="input" type="password" placeholder="Password" required value={password} onChange={(event) => setPassword(event.target.value)} />
      <button className="btn-primary" disabled={loading}>{loading ? "Creating account..." : "Sign Up"}</button>
    </form>
    <p className="footer"><Link className="link" href="/signin">Already have an account?</Link></p>
  </div></main>;
}
