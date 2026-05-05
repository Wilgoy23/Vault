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

  // Determine on first render whether a vault exists
  useState(() => {
    vaultExists().then((exists) => setIsNew(!exists));
  });

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");

    if (isNew && password !== confirm) {
      setError("Passwords don't match.");
      return;
    }
    if (password.length < 8) {
      setError("Password must be at least 8 characters.");
      return;
    }

    setLoading(true);
    try {
      if (isNew) {
        await createVault(password);
      } else {
        await unlock(password);
      }
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
      <div style={{
        background: "var(--bg2)", border: "1px solid var(--border)",
        borderRadius: "var(--radius-lg)", padding: "40px", width: "360px",
      }}>
        <div style={{ marginBottom: "28px" }}>
          <h1 style={{ fontSize: "20px", fontWeight: 600, marginBottom: "6px" }}>
            {isNew ? "Create your vault" : "Unlock your vault"}
          </h1>
          <p style={{ color: "var(--muted)", fontSize: "13px" }}>
            {isNew
              ? "Choose a strong master password. It cannot be recovered if lost."
              : "Enter your master password to continue."}
          </p>
        </div>

        <form onSubmit={handleSubmit} style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
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
            style={{ marginTop: "4px" }}
          >
            {loading ? "Please wait…" : isNew ? "Create vault" : "Unlock"}
          </button>
        </form>
      </div>
    </div>
  );
}