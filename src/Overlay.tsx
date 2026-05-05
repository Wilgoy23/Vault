import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isUnlocked, listEntries } from "./api";
import { Entry } from "./types";
import "./app.css";

export default function Overlay() {
  const [search, setSearch] = useState("");
  const [entries, setEntries] = useState<Entry[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [locked, setLocked] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Load entries and focus input whenever the window becomes visible
  useEffect(() => {
    const win = getCurrentWindow();
    const unlisten = win.onFocusChanged(({ payload: focused }) => {
      if (focused) {
        setSearch("");
        setCopied(null);
        setSelectedIndex(0);
        inputRef.current?.focus();
        isUnlocked().then((ok) => {
          setLocked(!ok);
          if (ok) listEntries().then(setEntries);
        });
      }
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  const filtered = entries.filter(
    (e) =>
      e.name.toLowerCase().includes(search.toLowerCase()) ||
      e.email.toLowerCase().includes(search.toLowerCase())
  );

  const copyAndClose = async (entry: Entry, type: "password" | "email") => {
    const value = type === "password" ? entry.password : entry.email;
    await navigator.clipboard.writeText(value);
    setCopied(type === "password" ? "Password copied!" : "Email copied!");
    setTimeout(() => navigator.clipboard.writeText(""), 30000);
    setTimeout(() => getCurrentWindow().hide(), 800);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    switch (e.key) {
      case "Escape":
        getCurrentWindow().hide();
        break;
      case "ArrowDown":
        e.preventDefault();
        setSelectedIndex((i) => Math.min(i + 1, filtered.length - 1));
        break;
      case "ArrowUp":
        e.preventDefault();
        setSelectedIndex((i) => Math.max(i - 1, 0));
        break;
      case "Enter":
        if (filtered[selectedIndex]) copyAndClose(filtered[selectedIndex], "password");
        break;
      case "Tab":
        e.preventDefault();
        if (filtered[selectedIndex]) copyAndClose(filtered[selectedIndex], "email");
        break;
    }
  };

  // Reset selection when search changes
  useEffect(() => setSelectedIndex(0), [search]);

  return (
    <div
      style={{
        display: "flex", flexDirection: "column",
        height: "100vh", background: "var(--bg2)",
        border: "1px solid var(--border)", borderRadius: "var(--radius-lg)",
        overflow: "hidden",
      }}
      onKeyDown={handleKeyDown}
    >
      {/* Search input */}
      <div style={{ padding: "12px", borderBottom: "1px solid var(--border)" }}>
        <input
          ref={inputRef}
          placeholder={locked ? "Vault is locked — open the app to unlock" : "Search passwords…"}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          disabled={locked}
          autoFocus
          style={{ fontSize: "15px", padding: "10px 14px" }}
        />
      </div>

      {/* Results */}
      <div style={{ flex: 1, overflowY: "auto" }}>
        {copied ? (
          <div style={{
            display: "flex", alignItems: "center", justifyContent: "center",
            height: "100%", color: "var(--success)", fontSize: "15px", fontWeight: 500,
          }}>
            {copied}
          </div>
        ) : locked ? (
          <div style={{ padding: "20px", color: "var(--muted)", fontSize: "13px", textAlign: "center" }}>
            Unlock the vault first.
          </div>
        ) : filtered.length === 0 && search ? (
          <div style={{ padding: "20px", color: "var(--muted)", fontSize: "13px", textAlign: "center" }}>
            No results for "{search}"
          </div>
        ) : (
          filtered.map((entry, i) => (
            <div
              key={entry.id}
              onClick={() => copyAndClose(entry, "password")}
              style={{
                padding: "11px 16px", cursor: "pointer",
                borderBottom: "1px solid var(--border)",
                background: i === selectedIndex ? "var(--bg3)" : "transparent",
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

      {/* Hint bar */}
      {!locked && !copied && (
        <div style={{
          padding: "8px 14px", borderTop: "1px solid var(--border)",
          display: "flex", gap: "16px",
          color: "var(--muted)", fontSize: "11px",
        }}>
          <span>↑↓ navigate</span>
          <span>Enter → copy password</span>
          <span>Tab → copy email</span>
          <span>Esc → close</span>
        </div>
      )}
    </div>
  );
}