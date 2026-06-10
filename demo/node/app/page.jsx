"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { getSession, signOut } from "./lib/api";

export default function Home() {
  const router = useRouter();
  const [user, setUser] = useState(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    getSession()
      .then((data) => {
        if (!data.user) router.replace("/signin");
        else setUser(data.user);
      })
      .catch(() => router.replace("/signin"))
      .finally(() => setLoading(false));
  }, [router]);

  async function handleSignOut() {
    await signOut().catch(() => {});
    router.replace("/signin");
  }

  if (loading) return <main className="page-center">Loading...</main>;
  if (!user) return null;

  return (
    <main className="page-center">
      <div className="card">
        <p className="welcome">Hello {user.name}</p>
        <button type="button" onClick={handleSignOut} className="btn-secondary">
          Sign Out
        </button>
      </div>
    </main>
  );
}
