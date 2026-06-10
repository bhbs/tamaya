import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { getSession, signOut } from "../lib/api";

export default function Home() {
  const navigate = useNavigate();
  const [user, setUser] = useState(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    getSession()
      .then((data) => {
        if (!data.user) {
          navigate("/signin", { replace: true, viewTransition: true });
        } else {
          setUser(data.user);
        }
      })
      .catch(() => {
        navigate("/signin", { replace: true, viewTransition: true });
      })
      .finally(() => setLoading(false));
  }, [navigate]);

  async function handleSignOut() {
    try {
      await signOut();
      navigate("/signin", { replace: true, viewTransition: true });
    } catch {
      // ignore
    }
  }

  if (loading) {
    return (
      <main className="page-center">
        <p style={{ color: "#6b7280" }}>Loading...</p>
      </main>
    );
  }

  if (!user) return null;

  return (
    <main className="page-center">
      <div className="card">
        <p style={{ marginBottom: "1rem", fontSize: "1.125rem", color: "#111827" }}>
          Hello {user.name}
        </p>

        <button
          type="button"
          onClick={handleSignOut}
          className="btn-secondary"
        >
          Sign Out
        </button>
      </div>
    </main>
  );
}
