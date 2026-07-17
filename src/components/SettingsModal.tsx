import { useEffect, useRef, useState } from "react";
import { Palette, Shield, Monitor, Database, X, Keyboard } from "lucide-react";
import { THEMES, Theme } from "../themes";
import { exportVault, importVault, importCsv, enableAutostart, disableAutostart, changeMasterPassword } from "../api";
import MasterPasswordMeter from "./MasterPasswordMeter";
import ConfirmDialog from "./ConfirmDialog";

const TIMEOUT_OPTIONS = [
  { label: "1 min",  ms: 1 * 60 * 1000 },
  { label: "5 min",  ms: 5 * 60 * 1000 },
  { label: "15 min", ms: 15 * 60 * 1000 },
  { label: "30 min", ms: 30 * 60 * 1000 },
  { label: "1 hour", ms: 60 * 60 * 1000 },
  { label: "Never",  ms: 0 },
];

interface Props {
  themeId: string;
  onThemeChange: (id: string) => void;
  timeoutMs: number;
  onTimeoutChange: (ms: number) => void;
  autostart: boolean;
  onAutostartChange: (v: boolean) => void;
  shortcut: string;
  onShortcutChange: (s: string) => void;
  onImported: () => void;
  onCsvImported: () => void;
  onClose: () => void;
}

export default function SettingsModal({
  themeId, onThemeChange,
  timeoutMs, onTimeoutChange,
  autostart, onAutostartChange,
  shortcut, onShortcutChange,
  onImported, onCsvImported, onClose,
}: Props) {
  const handleAutostartToggle = async () => {
    try {
      if (autostart) { await disableAutostart(); onAutostartChange(false); }
      else { await enableAutostart(); onAutostartChange(true); }
    } catch (e) { console.error("Autostart toggle failed:", e); }
  };

  const [confirmImport, setConfirmImport] = useState(false);
  const [importError, setImportError] = useState("");

  const handleExport = async () => {
    try { await exportVault(); } catch (e) { console.error("Export failed:", e); }
  };

  const handleImport = async () => {
    setConfirmImport(false);
    setImportError("");
    try { if (await importVault()) onImported(); }
    catch (e: any) { setImportError(e?.toString() ?? "Import failed."); }
  };

  return (
    <div
      style={{
        position: "fixed", inset: 0, background: "rgba(3,8,20,0.70)",
        backdropFilter: "blur(8px)", WebkitBackdropFilter: "blur(8px)",
        display: "flex", alignItems: "center", justifyContent: "center", zIndex: 100,
      }}
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div className="glass" style={{
        borderRadius: "var(--radius-lg)", width: "480px", maxHeight: "80vh",
        overflow: "hidden", display: "flex", flexDirection: "column",
        boxShadow: "0 8px 48px rgba(0,0,0,0.6), 0 0 80px rgba(30,80,200,0.08)",
      }}>
        {/* Header */}
        <div style={{
          display: "flex", alignItems: "center", justifyContent: "space-between",
          padding: "18px 22px 14px",
          borderBottom: "1px solid var(--border)", flexShrink: 0,
        }}>
          <span style={{ fontWeight: 600, fontSize: "15px" }}>Settings</span>
          <button className="btn-icon" onClick={onClose} style={{ width: "28px", height: "28px" }}>
            <X size={14} strokeWidth={2} />
          </button>
        </div>

        {/* Scrollable body */}
        <div style={{ overflowY: "auto", flex: 1, padding: "18px 22px" }}>

          <Section icon={<Palette size={13} strokeWidth={2} />} label="Appearance">
            <div style={{ display: "grid", gridTemplateColumns: "repeat(5, 1fr)", gap: "10px" }}>
              {THEMES.map((theme) => (
                <ThemeSwatch
                  key={theme.id}
                  theme={theme}
                  active={themeId === theme.id}
                  onSelect={() => onThemeChange(theme.id)}
                />
              ))}
            </div>
          </Section>

          <Divider />

          <Section icon={<Shield size={13} strokeWidth={2} />} label="Security">
            <Row label="Auto-lock">
              <select
                value={timeoutMs}
                onChange={(e) => onTimeoutChange(Number(e.target.value))}
                style={{ width: "140px" }}
              >
                {TIMEOUT_OPTIONS.map((o) => (
                  <option key={o.ms} value={o.ms}>
                    {o.label === "Never" ? "Never" : `After ${o.label}`}
                  </option>
                ))}
              </select>
            </Row>
            <ChangeMasterPassword />
          </Section>

          <Divider />

          <Section icon={<Monitor size={13} strokeWidth={2} />} label="System">
            <Row label="Launch on startup">
              <Toggle active={autostart} onToggle={handleAutostartToggle} />
            </Row>
          </Section>

          <Divider />

          <Section icon={<Keyboard size={13} strokeWidth={2} />} label="Shortcuts">
            <Row label="Open overlay">
              <KeybindRecorder value={shortcut} onChange={onShortcutChange} />
            </Row>
          </Section>

          <Divider />

          <Section icon={<Database size={13} strokeWidth={2} />} label="Data">
            <Row label="Vault backup">
              <div style={{ display: "flex", gap: "8px" }}>
                <button className="btn-ghost" style={{ fontSize: "13px", padding: "6px 14px" }} onClick={handleExport} title="Save an encrypted backup">
                  Export
                </button>
                <button className="btn-ghost" style={{ fontSize: "13px", padding: "6px 14px" }} onClick={() => { setImportError(""); setConfirmImport(true); }} title="Restore from an encrypted backup">
                  Import
                </button>
              </div>
            </Row>
            {importError && <p className="error" style={{ margin: "0 0 12px", fontSize: "12px" }}>{importError}</p>}
            <CsvImportRow onImported={onCsvImported} />
          </Section>

        </div>
      </div>

      {confirmImport && (
        <ConfirmDialog
          title="Replace current vault?"
          message="Importing a backup replaces your current vault and locks the app. Your existing vault is saved as vault.enc.bak first, and you'll unlock with the backup's master password."
          confirmLabel="Import backup"
          danger
          onConfirm={handleImport}
          onCancel={() => setConfirmImport(false)}
        />
      )}
    </div>
  );
}

function CsvImportRow({ onImported }: { onImported: () => void }) {
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const handleImport = async () => {
    setStatus("");
    setError("");
    setBusy(true);
    try {
      const report = await importCsv();
      if (report) {
        const entries = `${report.imported} ${report.imported === 1 ? "entry" : "entries"}`;
        setStatus(report.skipped > 0
          ? `Imported ${entries} · ${report.skipped} row${report.skipped === 1 ? "" : "s"} skipped`
          : `Imported ${entries}`);
        if (report.imported > 0) onImported();
      }
    } catch (e: any) {
      setError(e?.toString() ?? "Import failed.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Row label="CSV import">
        <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
          {status && (
            <span style={{ fontSize: "12.5px", color: "var(--success)" }}>{status}</span>
          )}
          <button
            className="btn-ghost"
            style={{ fontSize: "13px", padding: "6px 14px" }}
            onClick={handleImport}
            disabled={busy}
            title="Import logins from a browser or password manager CSV export"
          >
            {busy ? "Importing…" : "Import…"}
          </button>
        </div>
      </Row>
      {error && <p className="error" style={{ margin: "0 0 12px", fontSize: "12px" }}>{error}</p>}
      <p style={{ fontSize: "11px", color: "var(--muted-dim)", lineHeight: 1.5, margin: "-4px 0 0" }}>
        Supports Chrome, Edge, Firefox, Bitwarden and LastPass exports.
        Delete the CSV file afterwards — it contains your passwords in plain text.
      </p>
    </>
  );
}

function ChangeMasterPassword() {
  const [open, setOpen] = useState(false);
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const [done, setDone] = useState(false);
  const currentRef = useRef<HTMLInputElement>(null);

  const reset = () => { setCurrent(""); setNext(""); setConfirm(""); setError(""); };

  const toggle = () => {
    reset();
    setDone(false);
    setOpen((o) => !o);
    if (!open) setTimeout(() => currentRef.current?.focus(), 0);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    if (next.length < 8) { setError("New password must be at least 8 characters."); return; }
    if (next !== confirm) { setError("New passwords don't match."); return; }
    setSaving(true);
    try {
      await changeMasterPassword(current, next);
      reset();
      setOpen(false);
      setDone(true);
    } catch (err: any) {
      setError(err?.toString() ?? "Failed to change password.");
      currentRef.current?.focus();
    } finally {
      setSaving(false);
    }
  };

  return (
    <>
      <Row label="Master password">
        {done && !open && (
          <span style={{ fontSize: "12.5px", color: "var(--success)", marginRight: "10px" }}>
            Password updated
          </span>
        )}
        <button className="btn-ghost" style={{ fontSize: "13px", padding: "6px 14px" }} onClick={toggle}>
          {open ? "Cancel" : "Change…"}
        </button>
      </Row>
      {open && (
        <form
          onSubmit={handleSubmit}
          style={{
            display: "flex", flexDirection: "column", gap: "10px",
            padding: "14px", marginBottom: "12px",
            background: "rgba(255,255,255,0.03)",
            border: "1px solid var(--border-dim)", borderRadius: "var(--r-md)",
          }}
        >
          <input
            ref={currentRef}
            type="password"
            placeholder="Current password"
            value={current}
            onChange={(e) => { setCurrent(e.target.value); setError(""); }}
            autoFocus
          />
          <input
            type="password"
            placeholder="New password"
            value={next}
            onChange={(e) => { setNext(e.target.value); setError(""); }}
          />
          <MasterPasswordMeter password={next} />
          <input
            type="password"
            placeholder="Confirm new password"
            value={confirm}
            onChange={(e) => { setConfirm(e.target.value); setError(""); }}
          />
          {error && <p className="error" style={{ margin: 0, fontSize: "12px" }}>{error}</p>}
          <button
            type="submit"
            className="btn-primary"
            disabled={saving || !current || !next || !confirm}
            style={{ fontSize: "13px", padding: "8px" }}
          >
            {saving ? "Updating…" : "Update password"}
          </button>
        </form>
      )}
    </>
  );
}

function Section({ icon, label, children }: { icon: React.ReactNode; label: string; children: React.ReactNode }) {
  return (
    <div style={{ marginBottom: "4px" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "6px", marginBottom: "14px" }}>
        <span style={{ color: "var(--accent)" }}>{icon}</span>
        <span style={{ fontSize: "11px", fontWeight: 700, textTransform: "uppercase", letterSpacing: "0.08em", color: "var(--muted)" }}>
          {label}
        </span>
      </div>
      {children}
    </div>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "12px" }}>
      <span style={{ fontSize: "14px", color: "var(--text)" }}>{label}</span>
      {children}
    </div>
  );
}

function Divider() {
  return <div style={{ borderTop: "1px solid var(--border)", margin: "18px 0" }} />;
}

function ThemeSwatch({ theme, active, onSelect }: { theme: Theme; active: boolean; onSelect: () => void }) {
  return (
    <button
      onClick={onSelect}
      style={{ background: "transparent", border: "none", padding: 0, cursor: "pointer", display: "flex", flexDirection: "column", alignItems: "center", gap: "6px" }}
    >
      <div style={{
        width: "48px", height: "48px", borderRadius: "12px",
        background: theme.bg,
        border: active ? `2px solid ${theme.accent}` : "2px solid rgba(255,255,255,0.12)",
        boxShadow: active ? `0 0 14px ${theme.accent}55` : "none",
        display: "flex", alignItems: "center", justifyContent: "center",
        transition: "border-color 0.15s, box-shadow 0.15s",
        position: "relative", overflow: "hidden",
      }}>
        <div style={{ position: "absolute", bottom: 0, left: 0, right: 0, height: "10px", background: theme.accent, opacity: 0.85 }} />
        {active && (
          <div style={{ position: "absolute", top: "6px", width: "8px", height: "8px", borderRadius: "50%", background: theme.accent, boxShadow: `0 0 6px ${theme.accent}` }} />
        )}
      </div>
      <span style={{ fontSize: "11px", color: active ? "var(--accent)" : "var(--muted)", fontWeight: active ? 600 : 400, transition: "color 0.15s" }}>
        {theme.name}
      </span>
    </button>
  );
}

function KeybindRecorder({ value, onChange }: { value: string; onChange: (s: string) => void }) {
  const [recording, setRecording] = useState(false);
  const btnRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!recording) return;
    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      if (e.key === "Escape") { setRecording(false); return; }
      const isModifierOnly = ["Control", "Alt", "Shift", "Meta"].includes(e.key);
      if (isModifierOnly) return;
      const mods = [
        e.ctrlKey  && "Ctrl",
        e.altKey   && "Alt",
        e.shiftKey && "Shift",
        e.metaKey  && "Meta",
      ].filter(Boolean) as string[];
      const key = e.code
        .replace(/^Key/, "")
        .replace(/^Digit/, "")
        .replace(/^Arrow/, "Arrow");
      onChange([...mods, key].join("+"));
      setRecording(false);
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [recording, onChange]);

  return (
    <button
      ref={btnRef}
      className="btn-ghost"
      onClick={() => setRecording(true)}
      style={{
        fontSize: "12.5px", padding: "5px 14px",
        fontFamily: "var(--mono)", minWidth: "130px",
        borderColor: recording ? "var(--accent)" : undefined,
        color: recording ? "var(--accent)" : undefined,
        boxShadow: recording ? "0 0 0 3px var(--accent-tint)" : undefined,
      }}
    >
      {recording ? "Press keys…" : value}
    </button>
  );
}

function Toggle({ active, onToggle }: { active: boolean; onToggle: () => void }) {
  return (
    <button
      onClick={onToggle}
      style={{
        position: "relative", width: "40px", height: "22px",
        borderRadius: "11px", border: "none", padding: 0, cursor: "pointer",
        background: active ? "var(--accent)" : "rgba(255,255,255,0.15)",
        boxShadow: active ? "0 0 10px var(--accent-glow)" : "none",
        transition: "background 0.2s, box-shadow 0.2s", flexShrink: 0,
      }}
    >
      <div style={{
        position: "absolute", top: "3px", left: active ? "21px" : "3px",
        width: "16px", height: "16px", borderRadius: "50%",
        background: "#fff", transition: "left 0.2s",
        boxShadow: "0 1px 3px rgba(0,0,0,0.3)",
      }} />
    </button>
  );
}
