import { THEMES, Theme } from "../themes";
import { exportVault, importVault, enableAutostart, disableAutostart } from "../api";

const TIMEOUT_OPTIONS = [
  { label: "1 min",       ms: 1 * 60 * 1000 },
  { label: "5 min",       ms: 5 * 60 * 1000 },
  { label: "15 min",      ms: 15 * 60 * 1000 },
  { label: "30 min",      ms: 30 * 60 * 1000 },
  { label: "1 hour",      ms: 60 * 60 * 1000 },
  { label: "Never",       ms: 0 },
];

interface Props {
  themeId: string;
  onThemeChange: (id: string) => void;
  timeoutMs: number;
  onTimeoutChange: (ms: number) => void;
  autostart: boolean;
  onAutostartChange: (v: boolean) => void;
  onImported: () => void;
  onClose: () => void;
}

export default function SettingsModal({
  themeId, onThemeChange,
  timeoutMs, onTimeoutChange,
  autostart, onAutostartChange,
  onImported, onClose,
}: Props) {
  const handleAutostartToggle = async () => {
    try {
      if (autostart) {
        await disableAutostart();
        onAutostartChange(false);
      } else {
        await enableAutostart();
        onAutostartChange(true);
      }
    } catch (e) {
      console.error("Autostart toggle failed:", e);
    }
  };

  const handleExport = async () => {
    try { await exportVault(); } catch (e) { console.error("Export failed:", e); }
  };

  const handleImport = async () => {
    if (!window.confirm("Importing a vault will replace your current vault and lock the app. Continue?")) return;
    try {
      await importVault();
      onImported();
    } catch (e) {
      window.alert(`Import failed: ${e}`);
    }
  };

  return (
    <div style={{
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
          padding: "20px 24px 16px",
          borderBottom: "1px solid var(--border)",
          flexShrink: 0,
        }}>
          <span style={{ fontWeight: 600, fontSize: "16px" }}>Settings</span>
          <button
            className="btn-ghost"
            onClick={onClose}
            style={{ padding: "4px 10px", fontSize: "16px", lineHeight: 1 }}
          >
            ×
          </button>
        </div>

        {/* Scrollable body */}
        <div style={{ overflowY: "auto", flex: 1, padding: "20px 24px" }}>

          {/* Appearance */}
          <Section label="Appearance">
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

          {/* Security */}
          <Section label="Security">
            <Row label="Auto-lock">
              <select
                value={timeoutMs}
                onChange={(e) => onTimeoutChange(Number(e.target.value))}
                style={{
                  background: "rgba(255,255,255,0.06)", color: "var(--text)",
                  border: "1px solid var(--border)",
                  borderRadius: "var(--radius)", padding: "7px 10px",
                  fontSize: "13px", cursor: "pointer", width: "140px",
                }}
              >
                {TIMEOUT_OPTIONS.map((o) => (
                  <option key={o.ms} value={o.ms}>
                    {o.label === "Never" ? "Never" : `After ${o.label}`}
                  </option>
                ))}
              </select>
            </Row>
          </Section>

          <Divider />

          {/* System */}
          <Section label="System">
            <Row label="Launch on startup">
              <Toggle active={autostart} onToggle={handleAutostartToggle} />
            </Row>
          </Section>

          <Divider />

          {/* Data */}
          <Section label="Data">
            <Row label="Backup vault">
              <div style={{ display: "flex", gap: "8px" }}>
                <button
                  className="btn-ghost"
                  style={{ fontSize: "13px", padding: "6px 14px" }}
                  onClick={handleExport}
                  title="Save an encrypted backup"
                >
                  Export
                </button>
                <button
                  className="btn-ghost"
                  style={{ fontSize: "13px", padding: "6px 14px" }}
                  onClick={handleImport}
                  title="Restore from an encrypted backup"
                >
                  Import
                </button>
              </div>
            </Row>
          </Section>

        </div>
      </div>
    </div>
  );
}

function Section({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div style={{ marginBottom: "4px" }}>
      <div style={{ fontSize: "11px", fontWeight: 600, textTransform: "uppercase", letterSpacing: "0.08em", color: "var(--muted)", marginBottom: "14px" }}>
        {label}
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
      style={{
        background: "transparent",
        border: "none",
        padding: 0,
        cursor: "pointer",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        gap: "6px",
      }}
    >
      <div style={{
        width: "48px", height: "48px",
        borderRadius: "12px",
        background: theme.bg,
        border: active ? `2px solid ${theme.accent}` : "2px solid rgba(255,255,255,0.12)",
        boxShadow: active ? `0 0 14px ${theme.accent}55` : "none",
        display: "flex", alignItems: "center", justifyContent: "center",
        transition: "border-color 0.15s, box-shadow 0.15s",
        position: "relative",
        overflow: "hidden",
      }}>
        {/* Mini accent bar at bottom */}
        <div style={{
          position: "absolute", bottom: 0, left: 0, right: 0,
          height: "10px", background: theme.accent, opacity: 0.85,
        }} />
        {active && (
          <div style={{
            position: "absolute", top: "6px",
            width: "8px", height: "8px", borderRadius: "50%",
            background: theme.accent,
            boxShadow: `0 0 6px ${theme.accent}`,
          }} />
        )}
      </div>
      <span style={{
        fontSize: "11px",
        color: active ? "var(--accent)" : "var(--muted)",
        fontWeight: active ? 600 : 400,
        transition: "color 0.15s",
      }}>
        {theme.name}
      </span>
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
        transition: "background 0.2s, box-shadow 0.2s",
        flexShrink: 0,
      }}
    >
      <div style={{
        position: "absolute", top: "3px",
        left: active ? "21px" : "3px",
        width: "16px", height: "16px", borderRadius: "50%",
        background: "#fff",
        transition: "left 0.2s",
        boxShadow: "0 1px 3px rgba(0,0,0,0.3)",
      }} />
    </button>
  );
}
