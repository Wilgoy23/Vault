import { useState } from "react";
import { Entry, Folder } from "../types";
import { updateEntry, deleteEntry } from "../api";
import PasswordInput from "./PasswordInput";

interface Props {
  entry: Entry;
  folders: Folder[];
  onUpdated: (entry: Entry) => void;
  onDeleted: (id: string) => void;
}

export default function EntryDetail({ entry, folders, onUpdated, onDeleted }: Props) {
  const [editing, setEditing] = useState(false);
  const [showPassword, setShowPassword] = useState(false);
  const [copied, setCopied] = useState<"password" | "email" | "username" | null>(null);
  const [form, setForm] = useState({ ...entry });
  const [error, setError] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);

  const copy = async (type: "password" | "email" | "username") => {
    const value = type === "password" ? entry.password : type === "email" ? entry.email : (entry.username ?? "");
    await navigator.clipboard.writeText(value);
    setCopied(type);
    setTimeout(() => setCopied(null), 2000);
    setTimeout(() => navigator.clipboard.writeText(""), 30000);
  };

  const handleSave = async () => {
    setError("");
    if (!form.name.trim() || !form.email.trim() || !form.password.trim()) {
      setError("Name, email and password are required.");
      return;
    }
    try {
      await updateEntry({
        id: entry.id,
        name: form.name,
        username: form.username || undefined,
        email: form.email,
        password: form.password,
        url: form.url || undefined,
        notes: form.notes || undefined,
        folder_id: form.folder_id || undefined,
      });
      onUpdated({ ...form, updated_at: Date.now() / 1000 });
      setEditing(false);
    } catch (err: any) {
      setError(err?.toString() ?? "Failed to save.");
    }
  };

  const handleDelete = async () => {
    if (!confirmDelete) { setConfirmDelete(true); return; }
    try {
      await deleteEntry(entry.id);
      onDeleted(entry.id);
    } catch (err: any) {
      setError(err?.toString() ?? "Failed to delete.");
    }
  };

  const field = (label: string, key: keyof typeof form, type = "text") => (
    <div style={{ marginBottom: "16px" }}>
      <div style={{ color: "var(--muted)", fontSize: "12px", marginBottom: "4px" }}>{label}</div>
      {editing
        ? <input type={type} value={(form[key] as string) ?? ""} onChange={(e) => setForm({ ...form, [key]: e.target.value })} />
        : <div style={{ fontFamily: key === "password" ? "monospace" : "inherit", fontSize: "14px" }}>
            {key === "password"
              ? showPassword ? entry.password : "•".repeat(Math.min(entry.password.length, 20))
              : (entry[key] as string) || <span style={{ color: "var(--muted)" }}>—</span>}
          </div>
      }
    </div>
  );

  const currentFolder = folders.find((f) => f.id === entry.folder_id);

  return (
    <div style={{ flex: 1, minHeight: 0, padding: "28px", overflowY: "auto" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "24px" }}>
        <h2 style={{ fontSize: "18px", fontWeight: 600 }}>{entry.name}</h2>
        <div style={{ display: "flex", gap: "8px" }}>
          {!editing && (
            <button className="btn-ghost" onClick={() => { setEditing(true); setConfirmDelete(false); setForm({ ...entry }); }}>
              Edit
            </button>
          )}
          {editing && (
            <>
              <button className="btn-ghost" onClick={() => { setEditing(false); setError(""); }}>Cancel</button>
              <button className="btn-primary" onClick={handleSave}>Save</button>
            </>
          )}
          <button className="btn-danger" onClick={handleDelete}>
            {confirmDelete ? "Confirm delete" : "Delete"}
          </button>
        </div>
      </div>

      {field("Name", "name")}
      {field("Username", "username")}
      {field("Email", "email")}

      <div style={{ marginBottom: "16px" }}>
        <div style={{ color: "var(--muted)", fontSize: "12px", marginBottom: "4px" }}>Password</div>
        {editing
          ? <PasswordInput value={form.password} onChange={(v) => setForm({ ...form, password: v })} />
          : <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
              <span style={{ fontFamily: "monospace" }}>
                {showPassword ? entry.password : "•".repeat(Math.min(entry.password.length, 20))}
              </span>
              <button className="btn-ghost" style={{ padding: "4px 10px", fontSize: "12px" }} onClick={() => setShowPassword(!showPassword)}>
                {showPassword ? "Hide" : "Show"}
              </button>
              <button className="btn-ghost" style={{ padding: "4px 10px", fontSize: "12px" }} onClick={() => copy("password")}>
                {copied === "password" ? "Copied!" : "Copy"}
              </button>
            </div>
        }
      </div>

      {field("URL", "url")}
      {field("Notes", "notes")}

      {/* Folder field */}
      {folders.length > 0 && (
        <div style={{ marginBottom: "16px" }}>
          <div style={{ color: "var(--muted)", fontSize: "12px", marginBottom: "4px" }}>Folder</div>
          {editing ? (
            <select
              value={form.folder_id ?? ""}
              onChange={(e) => setForm({ ...form, folder_id: e.target.value || undefined })}
              style={{
                background: "rgba(255,255,255,0.06)", color: "var(--text)",
                border: "1px solid var(--border)",
                borderRadius: "var(--radius)", padding: "8px 10px", fontSize: "13px", width: "100%",
              }}
            >
              <option value="">No folder</option>
              {folders.map((f) => (
                <option key={f.id} value={f.id}>{f.name}</option>
              ))}
            </select>
          ) : (
            <div style={{ fontSize: "14px" }}>
              {currentFolder
                ? currentFolder.name
                : <span style={{ color: "var(--muted)" }}>—</span>}
            </div>
          )}
        </div>
      )}

      {!editing && (
        <div style={{ marginTop: "24px", paddingTop: "20px", borderTop: "1px solid var(--border)", display: "flex", gap: "8px" }}>
          {entry.username && (
            <button className="btn-ghost" onClick={() => copy("username")}>
              {copied === "username" ? "Username copied!" : "Copy username"}
            </button>
          )}
          <button className="btn-ghost" onClick={() => copy("email")}>
            {copied === "email" ? "Email copied!" : "Copy email"}
          </button>
          <button className="btn-ghost" onClick={() => copy("password")}>
            {copied === "password" ? "Password copied!" : "Copy password"}
          </button>
        </div>
      )}

      {error && <p className="error" style={{ marginTop: "12px" }}>{error}</p>}
    </div>
  );
}
