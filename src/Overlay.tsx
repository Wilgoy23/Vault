import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isUnlocked, listEntries, unlock } from "./api";
import { Entry } from "./types";
import "./app.css";

export default function Overlay() {
  const [search, setSearch] = useState("");
  const [entries, setEntries] = useState<Entry[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [locked, setLocked] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const [password, setPassword] = useState("");
  const [unlockError, setUnlockError] = useState("");
  const [unlocking, setUnlocking] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const passwordRef = useRef<HTMLInputElement>(null);

  const resetAndLoad = async () => {
    setSearch("");
    setCopied(null);
    setSelectedIndex(0);
    setPassword("");
    setUnlockError("");
    const ok = await isUnlocked();
    setLocked(!ok);
    if (ok) {
      const data = await listEntries();
      setEntries(data);
      inputRef.current?.focus();
    } else {
      passwordRef.current?.focus();
    }
  };

  useEffect(() => {
    const win = getCurrentWindow();
    const unlisten = win.onFocusChanged(({ payload: focused }) => {
      if (focused) resetAndLoad();
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  const handleUnlock = async (e: React.SubmitEvent) => {
    e.preventDefault();
    setUnlockError("");
    setUnlocking(true);
    try {
      await unlock(password);
      const data = await listEntries();
      setEntries(data);
      setLocked(false);
      setPassword("");
      setTimeout(() => inputRef.current?.focus(), 0);
    } catch {
      setUnlockError("Wrong password.");
      passwordRef.current?.select();
    } finally {
      setUnlocking(false);
    }
  };

  const filtered = entries.filter(
    (e) =>
      e.name.toLowerCase().includes(search.toLowerCase()) ||
      e.email.toLowerCase().includes(search.toLowerCase())
  );

  const copyValue = async (entry: Entry, type: "password" | "email") => {
    const value = type === "password" ? entry.password : entry.email;
    await navigator.clipboard.writeText(value);
    setCopied(type === "password" ? "Password copied!" : "Email copied!");
    setTimeout(() => navigator.clipboard.writeText(""), 30000);
    setTimeout(() => setCopied(null), 1500);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      getCurrentWindow().hide();
      return;
    }
    if (locked) return;
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setSelectedIndex((i) => Math.min(i + 1, filtered.length - 1));
        break;
      case "ArrowUp":
        e.preventDefault();
        setSelectedIndex((i) => Math.max(i - 1, 0));
        break;
      case "Enter":
        if (filtered[selectedIndex]) copyValue(filtered[selectedIndex], "password");
        break;
      case "Tab":
        e.preventDefault();
        if (filtered[selectedIndex]) copyValue(filtered[selectedIndex], "email");
        break;
    }
  };

  useEffect(() => setSelectedIndex(0), [search]);

  return (
    <div
      style={{
        display: "flex", flexDirection: "column",
        height: "100vh",
        background: "rgba(6,14,32,0.80)",
        backdropFilter: "blur(24px) saturate(200%)",
        WebkitBackdropFilter: "blur(24px) saturate(200%)",
        border: "1px solid rgba(255,255,255,0.10)",
        borderRadius: "var(--radius-lg)",
        overflow: "hidden",
        boxShadow: "0 16px 64px rgba(0,0,0,0.7), 0 0 80px rgba(30,80,200,0.15)",
      }}
      onKeyDown={handleKeyDown}
    >
      {locked ? (
        /* ── Inline unlock form ── */
        <div style={{
          display: "flex", flexDirection: "column", alignItems: "center",
          justifyContent: "center", flex: 1, padding: "24px", gap: "12px",
        }}>
          <div style={{ fontSize: "14px", fontWeight: 500, marginBottom: "4px" }}>
            Vault is locked
          </div>
          <form
            onSubmit={handleUnlock}
            style={{ display: "flex", flexDirection: "column", gap: "8px", width: "100%" }}
          >
            <input
              ref={passwordRef}
              type="password"
              placeholder="Master password"
              value={password}
              onChange={(e) => { setPassword(e.target.value); setUnlockError(""); }}
              style={{ fontSize: "14px", padding: "10px 14px" }}
              autoFocus
            />
            {unlockError && (
              <p className="error" style={{ margin: 0, fontSize: "12px" }}>{unlockError}</p>
            )}
            <button
              type="submit"
              className="btn-primary"
              disabled={unlocking || password.length === 0}
              style={{ padding: "9px", fontSize: "13px" }}
            >
              {unlocking ? "Unlocking…" : "Unlock"}
            </button>
          </form>
          <span style={{ color: "var(--muted)", fontSize: "11px" }}>Escape → close</span>
        </div>
      ) : (
        <>
          {/* ── Search input ── */}
          <div style={{ padding: "12px", borderBottom: "1px solid var(--border)" }}>
            <input
              ref={inputRef}
              placeholder="Search passwords…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              autoFocus
              style={{ fontSize: "15px", padding: "10px 14px" }}
            />
          </div>

          {/* ── Results ── */}
          <div style={{ flex: 1, overflowY: "auto" }}>
            {copied ? (
              <div style={{
                display: "flex", alignItems: "center", justifyContent: "center",
                height: "100%", color: "var(--success)", fontSize: "15px", fontWeight: 500,
              }}>
                {copied}
              </div>
            ) : filtered.length === 0 && search ? (
              <div style={{ padding: "20px", color: "var(--muted)", fontSize: "13px", textAlign: "center" }}>
                No results for "{search}"
              </div>
            ) : (
              filtered.map((entry, i) => (
                <div
                  key={entry.id}
                  onClick={() => copyValue(entry, "password")}
                  style={{
                    padding: "11px 16px", cursor: "pointer",
                    borderBottom: "1px solid var(--border)",
                    background: i === selectedIndex ? "rgba(77,157,224,0.12)" : "transparent",
                    borderLeft: i === selectedIndex ? "2px solid var(--accent)" : "2px solid transparent",
                    transition: "background 0.1s",
                  }}
                  onMouseEnter={() => setSelectedIndex(i)}
                >
                  <div style={{ fontWeight: 500, marginBottom: "2px" }}>{entry.name}</div>
                  <div style={{ color: "var(--muted)", fontSize: "12px" }}>{entry.email}</div>
                </div>
              ))
            )}
          </div>

          {/* ── Hint bar ── */}
          {!copied && (
            <div style={{
              padding: "8px 14px", borderTop: "1px solid var(--border)",
              display: "flex", gap: "16px",
              color: "var(--muted)", fontSize: "11px",
            }}>
              <span>↑↓ navigate</span>
              <span>Enter → copy password</span>
              <span>Tab → copy email</span>
              <span>Escape → close</span>
            </div>
          )}
        </>
      )}
    </div>
  );
}
