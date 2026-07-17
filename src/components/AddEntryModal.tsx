import { useMemo, useState } from "react";
import { X, ShieldCheck } from "lucide-react";
import { addEntry } from "../api";
import { Entry, Folder } from "../types";
import PasswordInput from "./PasswordInput";
import TotpSecretInput from "./TotpSecretInput";

interface Props {
  folders: Folder[];
  entries?: Entry[];
  defaultFolderId?: string;
  onAdded: (entry: Entry) => void;
  onClose: () => void;
}

export default function AddEntryModal({ folders, entries = [], defaultFolderId, onAdded, onClose }: Props) {
  const emailSuggestions = useMemo(
    () => [...new Set(entries.map((e) => e.email).filter(Boolean))].sort(),
    [entries]
  );
  const usernameSuggestions = useMemo(
    () => [...new Set(entries.map((e) => e.username).filter((u): u is string => !!u))].sort(),
    [entries]
  );
  const [form, setForm] = useState({
    name: "", username: "", email: "", password: "", url: "", notes: "",
    folder_id: defaultFolderId ?? "", totp_secret: "",
  });
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const set = (key: keyof typeof form) =>
    (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>) =>
      setForm({ ...form, [key]: e.target.value });

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    if (!form.name.trim() || !form.email.trim() || !form.password.trim()) {
      setError("Name, email and password are required.");
      return;
    }
    setLoading(true);
    try {
      const entry = await addEntry({
        name: form.name,
        username: form.username || undefined,
        email: form.email,
        password: form.password,
        url: form.url || undefined,
        notes: form.notes || undefined,
        folderId: form.folder_id || undefined,
        totpSecret: form.totp_secret || undefined,
      });
      onAdded(entry);
    } catch (err: any) {
      setError(err?.toString() ?? "Failed to add entry.");
    } finally {
      setLoading(false);
    }
  };

  const labelStyle: React.CSSProperties = {
    color: "var(--muted)", fontSize: "11px", fontWeight: 600,
    textTransform: "uppercase", letterSpacing: "0.06em",
    marginBottom: "4px", display: "block",
  };

  return (
    <div
      style={{
        position: "fixed", inset: 0, background: "rgba(3,8,20,0.65)",
        backdropFilter: "blur(6px)", WebkitBackdropFilter: "blur(6px)",
        display: "flex", alignItems: "center", justifyContent: "center", zIndex: 100,
      }}
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div className="glass" style={{
        borderRadius: "var(--radius-lg)", padding: "0", width: "420px",
        boxShadow: "0 8px 48px rgba(0,0,0,0.5), 0 0 60px rgba(30,80,200,0.10)",
        display: "flex", flexDirection: "column", maxHeight: "90vh",
      }}>
        {/* Header */}
        <div style={{
          display: "flex", alignItems: "center", justifyContent: "space-between",
          padding: "18px 20px 14px",
          borderBottom: "1px solid var(--border)",
          flexShrink: 0,
        }}>
          <span style={{ fontWeight: 600, fontSize: "15px" }}>Add entry</span>
          <button className="btn-icon" onClick={onClose} style={{ width: "28px", height: "28px" }}>
            <X size={14} strokeWidth={2} />
          </button>
        </div>

        {/* Form body */}
        <form
          onSubmit={handleSubmit}
          style={{ display: "flex", flexDirection: "column", gap: "14px", padding: "18px 20px 20px", overflowY: "auto" }}
        >
          <div>
            <label style={labelStyle}>Name</label>
            <input placeholder="e.g. GitHub" value={form.name} onChange={set("name")} autoFocus />
          </div>
          <div>
            <label style={labelStyle}>Username <span style={{ opacity: 0.5, fontWeight: 400, textTransform: "none", letterSpacing: 0 }}>(optional)</span></label>
            <input placeholder="Username" value={form.username} onChange={set("username")} list="username-suggestions" autoComplete="off" />
            {usernameSuggestions.length > 0 && (
              <datalist id="username-suggestions">
                {usernameSuggestions.map((u) => <option key={u} value={u} />)}
              </datalist>
            )}
          </div>
          <div>
            <label style={labelStyle}>Email</label>
            <input placeholder="email@example.com" type="email" value={form.email} onChange={set("email")} list="email-suggestions" autoComplete="off" />
            {emailSuggestions.length > 0 && (
              <datalist id="email-suggestions">
                {emailSuggestions.map((em) => <option key={em} value={em} />)}
              </datalist>
            )}
          </div>
          <div>
            <label style={labelStyle}>Password</label>
            <PasswordInput value={form.password} onChange={(v) => setForm({ ...form, password: v })} />
          </div>
          <div>
            <label style={labelStyle}>URL <span style={{ opacity: 0.5, fontWeight: 400, textTransform: "none", letterSpacing: 0 }}>(optional)</span></label>
            <input placeholder="https://…" value={form.url} onChange={set("url")} />
          </div>
          <div>
            <label style={labelStyle}>Notes <span style={{ opacity: 0.5, fontWeight: 400, textTransform: "none", letterSpacing: 0 }}>(optional)</span></label>
            <textarea placeholder="Notes…" value={form.notes} onChange={set("notes")} rows={2} style={{ resize: "vertical" }} />
          </div>
          <div>
            <label style={labelStyle}>
              <ShieldCheck size={11} strokeWidth={2.5} style={{ display: "inline", verticalAlign: "middle", marginRight: "4px" }} />
              2FA secret <span style={{ opacity: 0.5, fontWeight: 400, textTransform: "none", letterSpacing: 0 }}>(optional)</span>
            </label>
            <TotpSecretInput
              value={form.totp_secret}
              onChange={(v) => setForm((f) => ({ ...f, totp_secret: v }))}
              onMeta={({ issuer, account }) =>
                // An otpauth URI names the service — use it if the form is still blank
                setForm((f) => ({ ...f, name: f.name || issuer || account || f.name }))
              }
            />
          </div>
          {folders.length > 0 && (
            <div>
              <label style={labelStyle}>Folder</label>
              <select
                value={form.folder_id}
                onChange={set("folder_id")}
              >
                <option value="">No folder</option>
                {folders.map((f) => <option key={f.id} value={f.id}>{f.name}</option>)}
              </select>
            </div>
          )}
          {error && <p className="error">{error}</p>}
          <div style={{ display: "flex", gap: "8px", marginTop: "2px" }}>
            <button type="button" className="btn-ghost" onClick={onClose} style={{ flex: 1 }}>Cancel</button>
            <button type="submit" className="btn-primary" disabled={loading} style={{ flex: 1 }}>
              {loading ? "Saving…" : "Add entry"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
