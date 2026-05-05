import { useState } from "react";
import { createVault, unlock, vaultExists } from "../api";

interface Props {
  onUnlocked: () => void;
}

export default function LockScreen({ onUnlocked }: Props) {
  const [isNew, setIsNew] = useState<boolean | null>(null);
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  useState(() => {
    vaultExists().then((exists) => setIsNew(!exists));
  });

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    if (isNew && password !== confirm) { setError("Passwords don't match."); return; }
    if (password.length < 8) { setError("Password must be at least 8 characters."); return; }
    setLoading(true);
    try {
      if (isNew) await createVault(password);
      else await unlock(password);
      onUnlocked();
    } catch (err: any) {
      setError(err?.toString() ?? "Something went wrong.");
    } finally {
      setLoading(false);
    }
  };

  if (isNew === null) return null;

  return (
    <div style={{
      display: "flex", alignItems: "center", justifyContent: "center",
      height: "100vh", background: "var(--bg)",
    }}>
      {/* Subtle radial glow */}
      <div style={{
        position: "fixed", inset: 0, pointerEvents: "none",
        background: "radial-gradient(ellipse 60% 50% at 50% 40%, rgba(124,106,247,0.06) 0%, transparent 70%)",
      }} />

      <div style={{
        background: "var(--bg2)", border: "1px solid var(--border-hi)",
        borderRadius: "var(--radius-xl)", padding: "40px 36px",
        width: "360px", boxShadow: "var(--shadow)", position: "relative",
      }}>
        {/* Icon */}
        <div style={{
          width: "48px", height: "48px", borderRadius: "12px",
          background: "var(--accent-dim)", border: "1px solid rgba(124,106,247,0.25)",
          display: "flex", alignItems: "center", justifyContent: "center",
          marginBottom: "24px",
        }}>
          <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="var(--accent)"
            strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
            <rect x="3" y="11" width="18" height="11" rx="2"/>
            <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
          </svg>
        </div>

        <h1 style={{ fontSize: "18px", fontWeight: 600, marginBottom: "6px" }}>
          {isNew ? "Create your vault" : "Unlock your vault"}
        </h1>
        <p style={{ color: "var(--muted)", fontSize: "13px", marginBottom: "24px", lineHeight: 1.6 }}>
          {isNew
            ? "Choose a strong master password. It cannot be recovered if lost."
            : "Enter your master password to continue."}
        </p>

        <form onSubmit={handleSubmit} style={{ display: "flex", flexDirection: "column", gap: "10px" }}>
          <input
            type="password"
            placeholder="Master password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoFocus
          />
          {isNew && (
            <input
              type="password"
              placeholder="Confirm password"
              value={confirm}
              onChange={(e) => setConfirm(e.target.value)}
            />
          )}
          {error && <p className="error">{error}</p>}
          <button
            type="submit"
            className="btn-primary"
            disabled={loading}
            style={{ marginTop: "6px", padding: "10px" }}
          >
            {loading ? "Please wait…" : isNew ? "Create vault" : "Unlock"}
          </button>
        </form>
      </div>
    </div>
  );
}
