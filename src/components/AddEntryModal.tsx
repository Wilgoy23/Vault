import { useState } from "react";
import { addEntry } from "../api";
import { Entry } from "../types";

interface Props {
  onAdded: (entry: Entry) => void;
  onClose: () => void;
}

export default function AddEntryModal({ onAdded, onClose }: Props) {
  const [form, setForm] = useState({ name: "", email: "", password: "", url: "", notes: "" });
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const set = (key: keyof typeof form) => (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) =>
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
        email: form.email,
        password: form.password,
        url: form.url || undefined,
        notes: form.notes || undefined,
      });
      onAdded(entry);
    } catch (err: any) {
      setError(err?.toString() ?? "Failed to add entry.");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{
      position: "fixed", inset: 0, background: "rgba(0,0,0,0.6)",
      display: "flex", alignItems: "center", justifyContent: "center", zIndex: 100,
    }}>
      <div style={{
        background: "var(--bg2)", border: "1px solid var(--border)",
        borderRadius: "var(--radius-lg)", padding: "32px", width: "400px",
      }}>
        <h2 style={{ fontSize: "17px", fontWeight: 600, marginBottom: "20px" }}>Add entry</h2>
        <form onSubmit={handleSubmit} style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
          <input placeholder="Name  (e.g. GitHub)" value={form.name} onChange={set("name")} autoFocus />
          <input placeholder="Email" type="email" value={form.email} onChange={set("email")} />
          <input placeholder="Password" value={form.password} onChange={set("password")} />
          <input placeholder="URL (optional)" value={form.url} onChange={set("url")} />
          <textarea
            placeholder="Notes (optional)"
            value={form.notes}
            onChange={set("notes")}
            rows={3}
            style={{ resize: "vertical" }}
          />
          {error && <p className="error">{error}</p>}
          <div style={{ display: "flex", gap: "8px", marginTop: "4px" }}>
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