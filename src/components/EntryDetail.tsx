import { useState } from "react";
import { Copy, Check, Eye, EyeOff, Pencil, Trash2, X, Save } from "lucide-react";
import { Entry, Folder } from "../types";
import { updateEntry, deleteEntry } from "../api";
import PasswordInput from "./PasswordInput";

interface Props {
  entry: Entry;
  folders: Folder[];
  onUpdated: (entry: Entry) => void;
  onDeleted: (id: string) => void;
}

const AV_CLASSES = [
  "av-blue", "av-purple", "av-green", "av-red", "av-orange",
  "av-cyan", "av-pink", "av-yellow", "av-teal", "av-indigo",
];
function avatarClass(name: string) {
  const code = (name.charCodeAt(0) || 0) + (name.charCodeAt(1) || 0);
  return AV_CLASSES[code % AV_CLASSES.length];
}

function passwordStrength(pw: string): number {
  if (!pw) return 0;
  let score = 0;
  if (pw.length >= 8)  score++;
  if (pw.length >= 12) score++;
  if (/[A-Z]/.test(pw) && /[0-9]/.test(pw)) score++;
  if (/[^A-Za-z0-9]/.test(pw) && pw.length >= 14) score++;
  return Math.min(4, Math.max(1, score));
}

function useClipboard() {
  const [copied, setCopied] = useState<string | null>(null);
  const copy = async (key: string, value: string) => {
    await navigator.clipboard.writeText(value);
    setCopied(key);
    setTimeout(() => setCopied(null), 1600);
    setTimeout(() => navigator.clipboard.writeText(""), 30000);
  };
  return { copied, copy };
}

function CopyBtn({ id, value, copied, onCopy }: { id: string; value: string; copied: string | null; onCopy: (id: string, val: string) => void }) {
  const done = copied === id;
  return (
    <button
      className={`btn-action ${done ? "copied" : ""}`}
      onClick={() => onCopy(id, value)}
      title={done ? "Copied!" : "Copy"}
    >
      {done
        ? <Check size={11} strokeWidth={2.5} />
        : <Copy size={11} strokeWidth={2} />
      }
    </button>
  );
}

function StrengthBars({ score }: { score: number }) {
  const colors = ["s1", "s2", "s3", "s4"];
  return (
    <div className="pw-strength" title={["", "Weak", "Fair", "Good", "Strong"][score]}>
      {[1, 2, 3, 4].map((i) => (
        <div key={i} className={`pw-bar ${i <= score ? colors[score - 1] : ""}`} />
      ))}
    </div>
  );
}

export default function EntryDetail({ entry, folders, onUpdated, onDeleted }: Props) {
  const [editing, setEditing] = useState(false);
  const [showPassword, setShowPassword] = useState(false);
  const [form, setForm] = useState({ ...entry });
  const [error, setError] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const { copied, copy } = useClipboard();

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

  const currentFolder = folders.find((f) => f.id === entry.folder_id);
  const avClass = avatarClass(entry.name);
  const strength = passwordStrength(entry.password);

  const updatedAgo = (() => {
    const secs = Math.floor(Date.now() / 1000 - entry.updated_at);
    if (secs < 60) return "just now";
    if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
    if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
    return `${Math.floor(secs / 86400)}d ago`;
  })();

  return (
    <div style={{ flex: 1, minHeight: 0, padding: "22px 26px 20px", overflowY: "auto", display: "flex", flexDirection: "column" }}>

      {/* Header */}
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "22px", flexShrink: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: "14px" }}>
          <div
            className={`detail-av-lg avatar ${avClass}`}
            style={{ boxShadow: "0 0 0 1px rgba(77,157,224,0.22), 0 3px 14px rgba(77,157,224,0.12)" }}
          >
            {entry.name.charAt(0).toUpperCase()}
          </div>
          <div>
            <div style={{ fontSize: "22px", fontWeight: 700, color: "var(--text)", letterSpacing: "-0.03em", lineHeight: 1.05 }}>
              {entry.name}
            </div>
            <div className="detail-meta-row">
              {entry.url && <><span>{entry.url.replace(/^https?:\/\//, "")}</span><div className="detail-meta-dot" /></>}
              {currentFolder && <><span>{currentFolder.name}</span><div className="detail-meta-dot" /></>}
              <span>Updated {updatedAgo}</span>
            </div>
          </div>
        </div>
        <div style={{ display: "flex", gap: "5px" }}>
          {!editing ? (
            <>
              <button className="btn-icon" onClick={() => { setEditing(true); setConfirmDelete(false); setForm({ ...entry }); }} title="Edit">
                <Pencil size={14} strokeWidth={2} />
              </button>
              <button
                className={`btn-icon ${confirmDelete ? "danger" : ""}`}
                onClick={handleDelete}
                title={confirmDelete ? "Click again to confirm" : "Delete"}
                style={{ color: confirmDelete ? "var(--danger)" : undefined }}
              >
                <Trash2 size={14} strokeWidth={2} />
              </button>
            </>
          ) : (
            <>
              <button className="btn-icon" onClick={() => { setEditing(false); setError(""); }} title="Cancel">
                <X size={14} strokeWidth={2} />
              </button>
              <button
                className="btn-primary"
                onClick={handleSave}
                style={{ height: "30px", padding: "0 12px", fontSize: "12.5px", fontWeight: 600, display: "flex", alignItems: "center", gap: "5px" }}
              >
                <Save size={12} strokeWidth={2} />
                Save
              </button>
            </>
          )}
        </div>
      </div>

      {/* Field card */}
      <div className="field-card">

        {/* Name */}
        <div className={`field-row${editing ? " editing" : ""}`}>
          <span className="field-label-col">Name</span>
          <div className="field-divider-v" />
          <div className="field-body">
            {editing
              ? <input value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} />
              : <span className="field-val">{entry.name || <span className="field-val muted">—</span>}</span>
            }
          </div>
        </div>

        {/* Username */}
        <div className={`field-row${editing ? " editing" : ""}`}>
          <span className="field-label-col">Username</span>
          <div className="field-divider-v" />
          <div className="field-body">
            {editing
              ? <input value={form.username ?? ""} onChange={(e) => setForm({ ...form, username: e.target.value })} placeholder="Optional" />
              : <>
                  <span className="field-val">{entry.username || <span className="field-val muted">—</span>}</span>
                  {entry.username && <CopyBtn id="username" value={entry.username} copied={copied} onCopy={copy} />}
                </>
            }
          </div>
        </div>

        {/* Email */}
        <div className={`field-row${editing ? " editing" : ""}`}>
          <span className="field-label-col">Email</span>
          <div className="field-divider-v" />
          <div className="field-body">
            {editing
              ? <input type="email" value={form.email} onChange={(e) => setForm({ ...form, email: e.target.value })} />
              : <>
                  <span className="field-val">{entry.email}</span>
                  <CopyBtn id="email" value={entry.email} copied={copied} onCopy={copy} />
                </>
            }
          </div>
        </div>

        {/* Password */}
        <div className={`field-row${editing ? " editing" : ""}`}>
          <span className="field-label-col">Password</span>
          <div className="field-divider-v" />
          <div className="field-body">
            {editing
              ? <PasswordInput value={form.password} onChange={(v) => setForm({ ...form, password: v })} />
              : <>
                  <span className="field-val mono">
                    {showPassword ? entry.password : "●".repeat(Math.min(entry.password.length, 20))}
                  </span>
                  <StrengthBars score={strength} />
                  <button
                    className="btn-action"
                    onClick={() => setShowPassword(!showPassword)}
                    title={showPassword ? "Hide" : "Show"}
                  >
                    {showPassword ? <EyeOff size={12} strokeWidth={2} /> : <Eye size={12} strokeWidth={2} />}
                  </button>
                  <CopyBtn id="password" value={entry.password} copied={copied} onCopy={copy} />
                </>
            }
          </div>
        </div>

        {/* URL */}
        <div className={`field-row${editing ? " editing" : ""}`}>
          <span className="field-label-col">URL</span>
          <div className="field-divider-v" />
          <div className="field-body">
            {editing
              ? <input value={form.url ?? ""} onChange={(e) => setForm({ ...form, url: e.target.value })} placeholder="https://…" />
              : <>
                  {entry.url
                    ? <span className="field-val link">{entry.url}</span>
                    : <span className="field-val muted">—</span>
                  }
                  {entry.url && <CopyBtn id="url" value={entry.url} copied={copied} onCopy={copy} />}
                </>
            }
          </div>
        </div>

        {/* Notes */}
        <div className={`field-row${editing ? " editing" : ""}`} style={editing ? { height: "auto", alignItems: "flex-start", paddingTop: "10px", paddingBottom: "10px" } : {}}>
          <span className="field-label-col" style={editing ? { paddingTop: "6px" } : {}}>Notes</span>
          <div className="field-divider-v" />
          <div className="field-body">
            {editing
              ? <textarea value={form.notes ?? ""} onChange={(e) => setForm({ ...form, notes: e.target.value })} rows={2} style={{ resize: "vertical" }} placeholder="Optional" />
              : <span className="field-val" style={{ whiteSpace: "pre-wrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                  {entry.notes || <span className="field-val muted">—</span>}
                </span>
            }
          </div>
        </div>

        {/* Folder */}
        {folders.length > 0 && (
          <div className={`field-row${editing ? " editing" : ""}`}>
            <span className="field-label-col">Folder</span>
            <div className="field-divider-v" />
            <div className="field-body">
              {editing
                ? <select
                    value={form.folder_id ?? ""}
                    onChange={(e) => setForm({ ...form, folder_id: e.target.value || undefined })}
                  >
                    <option value="">No folder</option>
                    {folders.map((f) => <option key={f.id} value={f.id}>{f.name}</option>)}
                  </select>
                : <span className="field-val">{currentFolder?.name || <span className="field-val muted">—</span>}</span>
              }
            </div>
          </div>
        )}

      </div>

      {error && <p className="error" style={{ marginTop: "12px" }}>{error}</p>}
    </div>
  );
}
