import { useState } from "react";

interface Props {
  /** Shown above the fields */
  intro: React.ReactNode;
  submitLabel: string;
  busyLabel: string;
  initialUrl?: string;
  initialUsername?: string;
  /** Throws to show an error under the fields */
  onSubmit: (url: string, username: string, password: string) => Promise<void>;
  onCancel: () => void;
}

/** Plain http is refused by the backend except to this machine */
const allowedUrl = (url: string) =>
  /^https:\/\/.+/i.test(url) || /^http:\/\/(localhost|127\.0\.0\.1|\[::1\])([:/]|$)/i.test(url);

/** The server folder, account and password for WebDAV sync. */
export default function WebDavForm({ intro, submitLabel, busyLabel, initialUrl, initialUsername, onSubmit, onCancel }: Props) {
  const [url, setUrl] = useState(initialUrl ?? "");
  const [username, setUsername] = useState(initialUsername ?? "");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const trimmed = url.trim();
  const badUrl = trimmed.length > 0 && !allowedUrl(trimmed);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    setBusy(true);
    try {
      await onSubmit(trimmed, username.trim(), password);
    } catch (err: any) {
      setError(err?.toString() ?? "Couldn't connect to the server.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <form onSubmit={handleSubmit} style={boxStyle}>
      <div style={{ fontSize: "12.5px", lineHeight: 1.5 }}>{intro}</div>
      <input
        type="url"
        placeholder="https://dav.example.com/files/you/Vault"
        value={url}
        onChange={(e) => { setUrl(e.target.value); setError(""); }}
        autoComplete="off"
        autoCapitalize="off"
        spellCheck={false}
        autoFocus={!initialUrl}
        aria-label="Server folder address"
      />
      {badUrl && (
        <p style={{ fontSize: "11.5px", color: "var(--muted)", margin: "-4px 0 0" }}>
          The address must start with https://
        </p>
      )}
      <input
        type="text"
        placeholder="Username"
        value={username}
        onChange={(e) => { setUsername(e.target.value); setError(""); }}
        autoComplete="username"
        autoCapitalize="off"
        spellCheck={false}
        aria-label="Username"
      />
      <input
        type="password"
        placeholder="Password or app password"
        value={password}
        onChange={(e) => { setPassword(e.target.value); setError(""); }}
        autoComplete="current-password"
        autoFocus={!!initialUrl}
        aria-label="Password"
      />
      {error && <p className="error" style={{ margin: 0, fontSize: "12px" }}>{error}</p>}
      <div style={{ display: "flex", gap: "8px" }}>
        <button
          type="submit"
          className="btn-primary"
          disabled={busy || !trimmed || badUrl || !username.trim() || !password}
          style={{ flex: 1, fontSize: "13px", padding: "8px" }}
        >
          {busy ? busyLabel : submitLabel}
        </button>
        <button type="button" className="btn-ghost" onClick={onCancel} disabled={busy} style={{ fontSize: "13px", padding: "8px 14px" }}>
          Cancel
        </button>
      </div>
    </form>
  );
}

export const boxStyle: React.CSSProperties = {
  display: "flex", flexDirection: "column", gap: "10px",
  padding: "14px", marginBottom: "12px", textAlign: "left",
  background: "rgba(255,255,255,0.03)",
  border: "1px solid var(--border-dim)", borderRadius: "var(--r-md)",
};
