import { useState } from "react";
import { ShieldCheck, Lock, Eye, EyeOff, Check } from "lucide-react";
import { createVault, unlock, vaultExists } from "../api";
import MasterPasswordMeter from "./MasterPasswordMeter";

interface Props {
  onUnlocked: () => void;
}

export default function LockScreen({ onUnlocked }: Props) {
  const [isNew, setIsNew] = useState<boolean | null>(null);
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [showPw, setShowPw] = useState(false);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  useState(() => {
    vaultExists().then((exists) => setIsNew(!exists));
  });

  const confirmMismatch = isNew === true && confirm.length > 0 && confirm !== password;
  const confirmMatches = isNew === true && confirm.length > 0 && confirm === password;

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
          <div style={{ position: "relative" }}>
            <input
              type={showPw ? "text" : "password"}
              placeholder="Master password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoFocus
              style={{ width: "100%", paddingRight: "38px" }}
            />
            <button
              type="button"
              className="btn-icon"
              onClick={() => setShowPw((s) => !s)}
              title={showPw ? "Hide password" : "Show password"}
              tabIndex={-1}
              style={{
                position: "absolute", right: "6px", top: "50%",
                transform: "translateY(-50%)", padding: "5px",
              }}
            >
              {showPw ? <EyeOff size={14} strokeWidth={2} /> : <Eye size={14} strokeWidth={2} />}
            </button>
          </div>

          {/* Strength meter — creation only, where the choice is being made */}
          {isNew && <MasterPasswordMeter password={password} />}

          {isNew && (
            <div style={{ position: "relative" }}>
              <input
                type={showPw ? "text" : "password"}
                placeholder="Confirm password"
                value={confirm}
                onChange={(e) => setConfirm(e.target.value)}
                style={{
                  width: "100%", paddingRight: "38px",
                  borderColor: confirmMismatch ? "var(--danger)" : undefined,
                }}
              />
              {confirmMatches && (
                <Check
                  size={14} strokeWidth={2.5}
                  style={{
                    position: "absolute", right: "12px", top: "50%",
                    transform: "translateY(-50%)", color: "var(--success)",
                    pointerEvents: "none",
                  }}
                />
              )}
            </div>
          )}
          {confirmMismatch && (
            <p className="error" style={{ margin: 0, fontSize: "12px" }}>Passwords don't match.</p>
          )}
          {error && <p className="error">{error}</p>}
          <button
            type="submit"
            className="btn-primary"
            disabled={loading || (isNew === true && (password.length < 8 || confirm !== password))}
            style={{ marginTop: "4px" }}
          >
            {loading ? "Please wait…" : isNew ? "Create vault" : "Unlock"}
          </button>
        </form>
      </div>
    </div>
  );
}
