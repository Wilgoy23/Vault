import { useState } from "react";
import { ShieldCheck, Lock } from "lucide-react";
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
      if (isNew) { await createVault(password); } else { await unlock(password); }
      onUnlocked();
    } catch (err: any) {
      setError(err?.toString() ?? "Something went wrong.");
    } finally {
      setLoading(false);
    }
  };

  if (isNew === null) return null;

  return (
    <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100vh" }}>
      <div className="glass" style={{
        borderRadius: "var(--radius-lg)", padding: "40px", width: "360px",
        boxShadow: "0 8px 48px rgba(0,0,0,0.5), 0 0 80px rgba(30,80,200,0.12)",
      }}>
        {/* Icon + title */}
        <div style={{ display: "flex", flexDirection: "column", alignItems: "center", marginBottom: "28px", gap: "12px" }}>
          <div style={{
            width: "52px", height: "52px", borderRadius: "14px",
            background: "rgba(77,157,224,0.12)",
            border: "1px solid rgba(77,157,224,0.25)",
            display: "flex", alignItems: "center", justifyContent: "center",
            boxShadow: "0 0 24px rgba(77,157,224,0.15)",
          }}>
            {isNew
              ? <ShieldCheck size={26} color="var(--accent)" strokeWidth={1.75} />
              : <Lock size={24} color="var(--accent)" strokeWidth={1.75} />
            }
          </div>
          <div style={{ textAlign: "center" }}>
            <h1 style={{ fontSize: "19px", fontWeight: 600, marginBottom: "5px" }}>
              {isNew ? "Create your vault" : "Unlock your vault"}
            </h1>
            <p style={{ color: "var(--muted)", fontSize: "13px", lineHeight: 1.5 }}>
              {isNew
                ? "Choose a strong master password. It cannot be recovered if lost."
                : "Enter your master password to continue."}
            </p>
          </div>
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
          <button type="submit" className="btn-primary" disabled={loading} style={{ marginTop: "4px" }}>
            {loading ? "Please wait…" : isNew ? "Create vault" : "Unlock"}
          </button>
        </form>
      </div>
    </div>
  );
}
