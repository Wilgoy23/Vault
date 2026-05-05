import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { isUnlocked, listEntries } from "./api";
import { Entry } from "./types";
import "./app.css";

const AVATAR_COLORS = ["#7c6af7", "#e05593", "#05b8cc", "#e8a825", "#4caf82", "#e07a55", "#5b9cf7"];

function avatarColor(name: string) {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) & 0xffffffff;
  return AVATAR_COLORS[Math.abs(h) % AVATAR_COLORS.length];
}

type CopyType = "password" | "email" | "username";

export default function Overlay() {
  const [search, setSearch] = useState("");
  const [entries, setEntries] = useState<Entry[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [locked, setLocked] = useState(false);
  const [copiedMsg, setCopiedMsg] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const win = getCurrentWindow();

    // Reload state whenever the overlay window is focused
    const focusUnlisten = win.onFocusChanged(({ payload: focused }) => {
      if (focused) {
        setSearch("");
        setCopiedMsg(null);
        setSelectedIndex(0);
        inputRef.current?.focus();
        isUnlocked().then((ok) => {
          setLocked(!ok);
          if (ok) listEntries().then(setEntries);
        });
      }
    });

    // Stay in sync with vault state changes from any window
    const lockUnlisten = listen("vault:locked", () => {
      setLocked(true);
      setEntries([]);
    });

    const unlockUnlisten = listen("vault:unlocked", () => {
      setLocked(false);
      listEntries().then(setEntries);
    });

    return () => {
      focusUnlisten.then((f) => f());
      lockUnlisten.then((f) => f());
      unlockUnlisten.then((f) => f());
    };
  }, []);

  const filtered = entries.filter(
    (e) =>
      e.name.toLowerCase().includes(search.toLowerCase()) ||
      e.email.toLowerCase().includes(search.toLowerCase()) ||
      (e.username ?? "").toLowerCase().includes(search.toLowerCase())
  );

  const copyAndClose = async (entry: Entry, type: CopyType) => {
    const value =
      type === "password" ? entry.password :
      type === "email"    ? entry.email :
      (entry.username ?? "");
    if (!value) return;
    await navigator.clipboard.writeText(value);
    const label = type === "password" ? "Password" : type === "email" ? "Email" : "Username";
    setCopiedMsg(`${label} copied — clears in 30s`);
    setTimeout(() => navigator.clipboard.writeText(""), 30_000);
    setTimeout(() => getCurrentWindow().hide(), 900);
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
      case "u":
        if (e.ctrlKey && filtered[selectedIndex]?.username) {
          e.preventDefault();
          copyAndClose(filtered[selectedIndex], "username");
        }
        break;
    }
  };

  // Scroll selected item into view
  useEffect(() => {
    const item = listRef.current?.children[selectedIndex] as HTMLElement | undefined;
    item?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  useEffect(() => setSelectedIndex(0), [search]);

  const selectedEntry = filtered[selectedIndex] ?? null;

  return (
    <div
      style={{
        display: "flex", flexDirection: "column",
        height: "100vh",
        background: "rgba(13,13,14,0.96)",
        borderRadius: "var(--radius-xl)",
        border: "1px solid var(--border-hi)",
        overflow: "hidden",
        backdropFilter: "blur(24px)",
        WebkitBackdropFilter: "blur(24px)",
      }}
      onKeyDown={handleKeyDown}
    >
      {/* ── Search bar ───────────────────────────────────── */}
      <div style={{
        display: "flex", alignItems: "center", gap: "10px",
        padding: "14px 16px",
        borderBottom: `1px solid ${filtered.length > 0 || locked ? "var(--border)" : "transparent"}`,
      }}>
        {locked ? (
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--danger)"
            strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={{ flexShrink: 0 }}>
            <rect x="3" y="11" width="18" height="11" rx="2"/>
            <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
          </svg>
        ) : (
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="var(--muted)"
            strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={{ flexShrink: 0 }}>
            <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
          </svg>
        )}
        <input
          ref={inputRef}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          disabled={locked}
          autoFocus
          placeholder={locked ? "Vault is locked — open Vault to unlock" : "Search passwords…"}
          style={{
            flex: 1, background: "transparent", border: "none", outline: "none",
            boxShadow: "none", fontSize: "15px", padding: 0,
            color: locked ? "var(--muted)" : "var(--text)",
          }}
        />
        {search && (
          <button
            onClick={() => setSearch("")}
            style={{
              background: "transparent", border: "none", color: "var(--muted)",
              padding: "2px 6px", fontSize: "16px", lineHeight: 1, flexShrink: 0,
              cursor: "pointer",
            }}
          >
            ×
          </button>
        )}
      </div>

      {/* ── Results / states ─────────────────────────────── */}
      <div ref={listRef} style={{ flex: 1, overflowY: "auto" }}>
        {copiedMsg ? (
          <div style={{
            display: "flex", flexDirection: "column", alignItems: "center",
            justifyContent: "center", height: "100%", gap: "8px",
          }}>
            <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="var(--success)"
              strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="20 6 9 17 4 12"/>
            </svg>
            <span style={{ color: "var(--success)", fontSize: "14px", fontWeight: 500 }}>{copiedMsg}</span>
          </div>
        ) : locked ? (
          <div style={{
            display: "flex", flexDirection: "column", alignItems: "center",
            justifyContent: "center", height: "100%", gap: "10px",
            color: "var(--muted)",
          }}>
            <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor"
              strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" style={{ opacity: 0.5 }}>
              <rect x="3" y="11" width="18" height="11" rx="2"/>
              <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
            </svg>
            <span style={{ fontSize: "13px" }}>Open Vault to unlock</span>
          </div>
        ) : filtered.length === 0 ? (
          <div style={{
            display: "flex", alignItems: "center", justifyContent: "center",
            height: "100%", color: "var(--muted)", fontSize: "13px",
          }}>
            {search ? `No results for "${search}"` : "No entries in vault"}
          </div>
        ) : (
          filtered.map((entry, i) => {
            const active = i === selectedIndex;
            const color = avatarColor(entry.name);
            return (
              <div
                key={entry.id}
                onClick={() => copyAndClose(entry, "password")}
                style={{
                  display: "flex", alignItems: "center", gap: "11px",
                  padding: "9px 14px", cursor: "pointer",
                  background: active ? "var(--bg3)" : "transparent",
                  borderLeft: `2px solid ${active ? "var(--accent)" : "transparent"}`,
                  transition: "background 0.1s",
                }}
                onMouseEnter={() => setSelectedIndex(i)}
              >
                {/* Avatar */}
                <div style={{
                  width: "32px", height: "32px", borderRadius: "8px", flexShrink: 0,
                  background: `${color}1a`, border: `1px solid ${color}33`,
                  display: "flex", alignItems: "center", justifyContent: "center",
                  fontSize: "13px", fontWeight: 600, color,
                }}>
                  {entry.name.charAt(0).toUpperCase()}
                </div>

                {/* Text */}
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{
                    fontWeight: 500, fontSize: "13px",
                    whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
                  }}>
                    {entry.name}
                  </div>
                  <div style={{
                    color: "var(--muted)", fontSize: "12px",
                    whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
                  }}>
                    {entry.username || entry.email}
                  </div>
                </div>

                {/* Active hint */}
                {active && (
                  <div style={{ display: "flex", gap: "5px", flexShrink: 0 }}>
                    <kbd>↵</kbd>
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>

      {/* ── Hint bar ─────────────────────────────────────── */}
      {!locked && !copiedMsg && filtered.length > 0 && (
        <div style={{
          display: "flex", gap: "14px", flexWrap: "wrap",
          padding: "8px 14px", borderTop: "1px solid var(--border)",
          color: "var(--muted)", fontSize: "11px", lineHeight: 1,
        }}>
          <span style={{ display: "flex", alignItems: "center", gap: "4px" }}>
            <kbd>↑↓</kbd> navigate
          </span>
          <span style={{ display: "flex", alignItems: "center", gap: "4px" }}>
            <kbd>↵</kbd> copy password
          </span>
          <span style={{ display: "flex", alignItems: "center", gap: "4px" }}>
            <kbd>Tab</kbd> copy email
          </span>
          {selectedEntry?.username && (
            <span style={{ display: "flex", alignItems: "center", gap: "4px" }}>
              <kbd>Ctrl</kbd><kbd>U</kbd> copy username
            </span>
          )}
          <span style={{ display: "flex", alignItems: "center", gap: "4px", marginLeft: "auto" }}>
            <kbd>Esc</kbd> close
          </span>
        </div>
      )}
    </div>
  );
}
