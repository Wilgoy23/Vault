import { useEffect, useRef, useState } from "react";
import { Copy, Check, Eye, EyeOff, Pencil, Trash2, X, Save, ShieldCheck } from "lucide-react";
import { Entry, Folder } from "../types";
import { updateEntry, deleteEntry, clearClipboard } from "../api";
import { generateTOTP, totpSecondsLeft } from "../utils/totp";
import PasswordInput from "./PasswordInput";

interface Props {
  entry: Entry;
  folders: Folder[];
  onUpdated: (entry: Entry) => void;
  onDeleted: (id: string) => void;
  editTrigger?: number;
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
  const [countdown, setCountdown] = useState<number | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const copy = async (key: string, value: string) => {
    await navigator.clipboard.writeText(value);
    setCopied(key);
    setTimeout(() => setCopied(null), 1500);

    if (timerRef.current) clearInterval(timerRef.current);
    setCountdown(30);
    timerRef.current = setInterval(() => {
      setCountdown(prev => {
        if (prev === null || prev <= 1) {
          clearInterval(timerRef.current!);
          timerRef.current = null;
          clearClipboard().catch(() => {});
          return null;
        }
        return prev - 1;
      });
    }, 1000);
  };

  useEffect(() => () => { if (timerRef.current) clearInterval(timerRef.current); }, []);

  return { copied, copy, countdown };
}

function CopyBtn({ id, value, copied, onCopy }: {
  id: string; value: string; copied: string | null; onCopy: (id: string, val: string) => void;
}) {
  const done = copied === id;
  return (
    <button
      className={`btn-action ${done ? "copied" : ""}`}
      onClick={() => onCopy(id, value)}
      title={done ? "Copied!" : "Copy"}
    >
      {done ? <Check size={11} strokeWidth={2.5} /> : <Copy size={11} strokeWidth={2} />}
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

function TotpDisplay({ secret, copied, onCopy }: {
  secret: string; copied: string | null; onCopy: (id: string, val: string) => void;
}) {
  const [code, setCode] = useState("------");
  const [secsLeft, setSecsLeft] = useState(30);

  useEffect(() => {
    let mounted = true;
    const update = async () => {
      if (!mounted) return;
      const [newCode, secs] = await Promise.all([generateTOTP(secret), Promise.resolve(totpSecondsLeft())]);
      if (mounted) { setCode(newCode); setSecsLeft(secs); }
    };
    update();
    const iv = setInterval(update, 1000);
    return () => { mounted = false; clearInterval(iv); };
  }, [secret]);

  const danger = secsLeft <= 5;
  const pct = (secsLeft / 30) * 100;

  return (
    <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
      <span style={{
        fontFamily: "var(--mono)", fontSize: "17px", letterSpacing: "0.18em", fontWeight: 600,
        color: danger ? "var(--danger)" : "var(--fg-mid)",
        transition: "color 0.3s",
      }}>
        {code.slice(0, 3)}&thinsp;{code.slice(3)}
      </span>
      {/* Arc-style countdown ring */}
      <svg width="22" height="22" viewBox="0 0 22 22" style={{ flexShrink: 0 }}>
        <circle cx="11" cy="11" r="8" fill="none" stroke="rgba(255,255,255,0.08)" strokeWidth="2.5" />
        <circle
          cx="11" cy="11" r="8" fill="none"
          stroke={danger ? "var(--danger)" : "var(--accent)"}
          strokeWidth="2.5"
          strokeLinecap="round"
          strokeDasharray={`${2 * Math.PI * 8}`}
          strokeDashoffset={`${2 * Math.PI * 8 * (1 - pct / 100)}`}
          transform="rotate(-90 11 11)"
          style={{ transition: "stroke-dashoffset 1s linear, stroke 0.3s" }}
        />
        <text x="11" y="14.5" textAnchor="middle" fontSize="6.5" fontWeight="700"
          fill={danger ? "var(--danger)" : "var(--muted)"}>
          {secsLeft}
        </text>
      </svg>
      <CopyBtn id="totp" value={code} copied={copied} onCopy={onCopy} />
    </div>
  );
}

function ClipboardBanner({ countdown }: { countdown: number | null }) {
  if (countdown === null) return null;
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: "12px",
      padding: "8px 16px",
      background: "rgba(77,157,224,0.07)",
      borderTop: "1px solid rgba(77,157,224,0.15)",
      flexShrink: 0,
    }}>
      <span style={{ fontSize: "12px", color: "var(--muted)", whiteSpace: "nowrap" }}>
        Clipboard clears in {countdown}s
      </span>
      <div style={{ flex: 1, height: "2px", background: "rgba(255,255,255,0.08)", borderRadius: "1px", overflow: "hidden" }}>
        <div style={{
          height: "100%", background: "var(--accent)", borderRadius: "1px",
          width: `${(countdown / 30) * 100}%`,
          transition: "width 1s linear",
        }} />
      </div>
    </div>
  );
}

export default function EntryDetail({ entry, folders, onUpdated, onDeleted, editTrigger }: Props) {
  const [editing, setEditing] = useState(false);
  const [showPassword, setShowPassword] = useState(false);
  const [form, setForm] = useState({ ...entry });
  const [error, setError] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const { copied, copy, countdown } = useClipboard();

  useEffect(() => {
    if (editTrigger && editTrigger > 0) setEditing(true);
  }, [editTrigger]);

  // Reset form when entry changes
  useEffect(() => {
    setForm({ ...entry });
    setEditing(false);
    setError("");
    setConfirmDelete(false);
  }, [entry.id]);

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
        folderId: form.folder_id || undefined,
        totpSecret: form.totp_secret || undefined,
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
    <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", overflow: "hidden" }}>
      <div style={{ flex: 1, padding: "22px 26px 20px", overflowY: "auto", display: "flex", flexDirection: "column" }}>

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
                <button className="btn-icon" onClick={() => { setEditing(true); setConfirmDelete(false); setForm({ ...entry }); }} title="Edit (E)">
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
                : <span className="field-val">{entry.name}</span>
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
                    <button className="btn-action" onClick={() => setShowPassword(!showPassword)} title={showPassword ? "Hide" : "Show"}>
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
                : <span className="field-val" style={{ whiteSpace: "pre-wrap" }}>
                    {entry.notes || <span className="field-val muted">—</span>}
                  </span>
              }
            </div>
          </div>

          {/* TOTP */}
          <div className={`field-row${editing ? " editing" : ""}`}>
            <span className="field-label-col" style={{ display: "flex", alignItems: "center", gap: "4px" }}>
              <ShieldCheck size={10} strokeWidth={2.5} style={{ flexShrink: 0 }} />
              2FA
            </span>
            <div className="field-divider-v" />
            <div className="field-body">
              {editing
                ? <input
                    value={form.totp_secret ?? ""}
                    onChange={(e) => setForm({ ...form, totp_secret: e.target.value })}
                    placeholder="Base32 secret (optional)"
                    style={{ fontFamily: "var(--mono)", fontSize: "12.5px", letterSpacing: "0.05em" }}
                  />
                : entry.totp_secret
                  ? <TotpDisplay secret={entry.totp_secret} copied={copied} onCopy={copy} />
                  : <span className="field-val muted">—</span>
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
                  ? <select value={form.folder_id ?? ""} onChange={(e) => setForm({ ...form, folder_id: e.target.value || undefined })}>
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

      <ClipboardBanner countdown={countdown} />
    </div>
  );
}
