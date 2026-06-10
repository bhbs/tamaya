"use client";

import Link from "next/link";
import { useState } from "react";
import { forgotPassword } from "../lib/api";

export default function ForgotPassword() {
  const [email, setEmail] = useState("");
  const [message, setMessage] = useState("");

  async function submit(event) {
    event.preventDefault();
    await forgotPassword(email);
    setMessage("If that email exists, a password reset link has been sent.");
  }

  return <main className="page-center"><div className="card">
    <h1 className="title">Reset password</h1>
    {message && <p className="alert-success">{message}</p>}
    <form onSubmit={submit} className="form-stack">
      <input className="input" type="email" placeholder="Email" required value={email} onChange={(event) => setEmail(event.target.value)} />
      <button className="btn-primary">Send reset link</button>
    </form>
    <p className="footer"><Link className="link" href="/signin">Back to sign in</Link></p>
  </div></main>;
}
