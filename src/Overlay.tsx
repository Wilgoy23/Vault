import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { emit } from "@tauri-apps/api/event";
import { Eye, EyeOff, ArrowLeft } from "lucide-react";
import { isUnlocked, listEntries, unlock, clearClipboard } from "./api";
import { Entry } from "./types";
import { generateTOTP, totpSecondsLeft } from "./utils/totp";
import { applyTheme, DEFAULT_THEME_ID } from "./themes";
import "./App.css";

const AV_CLASSES = [
  "av-blue", "av-purple", "av-green", "av-red", "av-orange",
  "av-cyan", "av-pink", "av-yellow", "av-teal", "av-indigo",
];
function avatarClass(name: string) {
  const code = (name.charCodeAt(0) || 0) + (name.charCodeAt(1) || 0);
  return AV_CLASSES[code % AV_CLASSES.length];
}

function useCopy() {
  const [copied, setCopied] = useState<string | null>(null);
  const [countdown, setCountdown] = useState<number | null>(null);
  const timer = useRef<ReturnType<typeof setInterval> | null>(null);

  const copy = async (key: string, value: string) => {
    await navigator.clipboard.writeText(value);
    setCopied(key);
    setTimeout(() => setCopied(null), 1500);
    if (timer.current) clearInterval(timer.current);
    setCountdown(30);
    timer.current = setInterval(() => {
      setCountdown(prev => {
        if (prev === null || prev <= 1) {
          clearInterval(timer.current!);
          timer.current = null;
          clearClipboard().catch(() => {});
          return null;
        }
        return prev - 1;
      });
    }, 1000);
  };

  useEffect(() => () => { if (timer.current) clearInterval(timer.current); }, []);
  return { copied, copy, countdown };
}

function useTOTP(secret: string) {
  const [code, setCode] = useState("------");
  const [secsLeft, setSecsLeft] = useState(30);
  useEffect(() => {
    let mounted = true;
    const update = async () => {
      if (!mounted) return;
      const c = await generateTOTP(secret);
      if (mounted) { setCode(c); setSecsLeft(totpSecondsLeft()); }
    };
    update();
    const iv = setInterval(update, 1000);
    return () => { mounted = false; clearInterval(iv); };
  }, [secret]);
  return { code, secsLeft };
}

/** Compact TOTP badge shown inline in the search list */
function TotpBadge({ secret }: { secret: string }) {
  const { code, secsLeft } = useTOTP(secret);
  const danger = secsLeft <= 5;
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "5px", marginLeft: "auto", flexShrink: 0 }}>
      <span style={{
        fontFamily: "var(--mono)", fontSize: "12.5px", fontWeight: 600,
        letterSpacing: "0.12em",
        color: danger ? "var(--danger)" : "var(--accent)",
      }}>
        {code.slice(0, 3)} {code.slice(3)}
      </span>
      <span style={{
        fontSize: "10px", fontWeight: 600,
        color: danger ? "var(--danger)" : "var(--muted-dim)",
        minWidth: "20px", textAlign: "right",
      }}>
        {secsLeft}s
      </span>
    </div>
  );
}

/** Full TOTP row shown in the entry detail */
function TotpDisplay({ secret }: { secret: string }) {
  const { code, secsLeft } = useTOTP(secret);
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
      <svg width="20" height="20" viewBox="0 0 22 22" style={{ flexShrink: 0 }}>
        <circle cx="11" cy="11" r="8" fill="none" stroke="rgba(255,255,255,0.08)" strokeWidth="2.5" />
        <circle
          cx="11" cy="11" r="8" fill="none"
          stroke={danger ? "var(--danger)" : "var(--accent)"}
          strokeWidth="2.5" strokeLinecap="round"
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
    </div>
  );
}

function CopiedToast({ label }: { label: string }) {
  return (
    <div style={{
      position: "absolute", top: "50px", left: "50%", transform: "translateX(-50%)",
      background: "rgba(34,197,94,0.18)", border: "1px solid rgba(34,197,94,0.35)",
      borderRadius: "var(--r-md)", padding: "5px 14px",
      fontSize: "12px", fontWeight: 600, color: "#4ADE80",
      whiteSpace: "nowrap", pointerEvents: "none", zIndex: 10,
    }}>
      {label} copied!
    </div>
  );
}

function EntryDetail({ entry, onClose }: { entry: Entry; onClose: () => void }) {
  const [showPw, setShowPw] = useState(false);
  const { copied, copy, countdown } = useCopy();

  // Get current TOTP code for keyboard copy (non-reactive snapshot is fine here)
  const totpCodeRef = useRef<string>("------");
  useEffect(() => {
    if (!entry.totp_secret) return;
    const update = async () => { totpCodeRef.current = await generateTOTP(entry.totp_secret!); };
    update();
    const iv = setInterval(update, 1000);
    return () => clearInterval(iv);
  }, [entry.totp_secret]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (target.tagName === "INPUT" || target.tagName === "TEXTAREA") return;
      switch (e.key.toLowerCase()) {
        case "enter":
        case "p":
          e.preventDefault();
          copy("password", entry.password);
          break;
        case "e":
          e.preventDefault();
          copy("email", entry.email);
          break;
        case "u":
          if (entry.username) { e.preventDefault(); copy("username", entry.username); }
          break;
        case "t":
          if (entry.totp_secret) { e.preventDefault(); copy("totp", totpCodeRef.current); }
          break;
        case "v":
          e.preventDefault();
          setShowPw(s => !s);
          break;
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [entry]);

  const fieldRow = (label: string, content: React.ReactNode, tall = false) => (
    <div style={{
      display: "flex", alignItems: "center", minHeight: tall ? "52px" : "44px",
      padding: "0 14px", borderBottom: "1px solid rgba(255,255,255,0.045)",
    }}>
      <span style={{
        fontSize: "10px", fontWeight: 700, textTransform: "uppercase",
        letterSpacing: "0.09em", color: "var(--muted-dim)", width: "72px", flexShrink: 0,
      }}>
        {label}
      </span>
      <div style={{ width: "1px", height: "16px", background: "rgba(255,255,255,0.055)", margin: "0 10px 0 4px", flexShrink: 0 }} />
      <div style={{ display: "flex", alignItems: "center", gap: "8px", flex: 1, minWidth: 0 }}>
        {content}
      </div>
    </div>
  );

  const copiedLabel = copied === "password" ? "Password"
    : copied === "email" ? "Email"
    : copied === "username" ? "Username"
    : copied === "totp" ? "2FA code"
    : null;

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", position: "relative" }}>
      {copiedLabel && <CopiedToast label={copiedLabel} />}

      {/* Header */}
      <div style={{
        display: "flex", alignItems: "center", gap: "10px",
        padding: "10px 12px", borderBottom: "1px solid var(--border)", flexShrink: 0,
      }}>
        <button className="btn-icon" onClick={onClose} title="Back (Escape)" style={{ padding: "4px" }}>
          <ArrowLeft size={14} strokeWidth={2} />
        </button>
        <div className={`avatar ${avatarClass(entry.name)}`}
          style={{ width: "28px", height: "28px", borderRadius: "7px", fontSize: "11px" }}>
          {entry.name.charAt(0).toUpperCase()}
        </div>
        <span style={{ fontWeight: 600, fontSize: "14px", flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {entry.name}
        </span>
      </div>

      {/* Fields */}
      <div style={{ flex: 1, overflowY: "auto" }}>
        <div style={{
          background: "rgba(255,255,255,0.03)", margin: "10px",
          borderRadius: "var(--r-md)", overflow: "hidden", border: "1px solid var(--border-dim)",
        }}>
          {fieldRow("Email",
            <span style={{ fontSize: "13px", color: copied === "email" ? "var(--success)" : "var(--fg-mid)", flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", transition: "color 0.2s" }}>
              {entry.email}
            </span>
          )}
          {entry.username && fieldRow("Username",
            <span style={{ fontSize: "13px", color: copied === "username" ? "var(--success)" : "var(--fg-mid)", flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", transition: "color 0.2s" }}>
              {entry.username}
            </span>
          )}
          {fieldRow("Password",
            <>
              <span style={{
                fontFamily: "var(--mono)", fontSize: "13.5px",
                color: copied === "password" ? "var(--success)" : "var(--fg-mid)",
                flex: 1, letterSpacing: showPw ? "0.04em" : "0.08em",
                transition: "color 0.2s",
              }}>
                {showPw ? entry.password : "●".repeat(Math.min(entry.password.length, 18))}
              </span>
              <button className="btn-icon" onClick={() => setShowPw(s => !s)} title="Toggle (V)" style={{ padding: "2px" }}>
                {showPw ? <EyeOff size={12} strokeWidth={2} /> : <Eye size={12} strokeWidth={2} />}
              </button>
            </>
          )}
          {entry.totp_secret && fieldRow("2FA",
            <div style={{ opacity: copied === "totp" ? 0.6 : 1, transition: "opacity 0.2s" }}>
              <TotpDisplay secret={entry.totp_secret} />
            </div>,
            true
          )}
        </div>
      </div>

      {/* Clipboard countdown */}
      {countdown !== null && (
        <div style={{
          display: "flex", alignItems: "center", gap: "10px",
          padding: "5px 14px", background: "rgba(77,157,224,0.07)",
          borderTop: "1px solid rgba(77,157,224,0.15)", flexShrink: 0,
        }}>
          <span style={{ fontSize: "11px", color: "var(--muted)", whiteSpace: "nowrap" }}>
            Clears in {countdown}s
          </span>
          <div style={{ flex: 1, height: "2px", background: "rgba(255,255,255,0.08)", borderRadius: "1px", overflow: "hidden" }}>
            <div style={{
              height: "100%", background: "var(--accent)", borderRadius: "1px",
              width: `${(countdown / 30) * 100}%`, transition: "width 1s linear",
            }} />
          </div>
        </div>
      )}

      {/* Keyboard hints */}
      <div style={{
        padding: "6px 12px", borderTop: "1px solid var(--border)",
        display: "flex", gap: "10px", flexWrap: "wrap",
        color: "var(--muted-dim)", fontSize: "10.5px", flexShrink: 0,
      }}>
        <span><kbd style={kbdStyle}>Enter</kbd> password</span>
        <span><kbd style={kbdStyle}>E</kbd> email</span>
        {entry.username && <span><kbd style={kbdStyle}>U</kbd> username</span>}
        {entry.totp_secret && <span><kbd style={kbdStyle}>T</kbd> 2FA</span>}
        <span><kbd style={kbdStyle}>V</kbd> reveal</span>
        <span><kbd style={kbdStyle}>Esc</kbd> back</span>
      </div>
    </div>
  );
}

const kbdStyle: React.CSSProperties = {
  display: "inline-block",
  background: "rgba(255,255,255,0.08)",
  border: "1px solid rgba(255,255,255,0.14)",
  borderRadius: "3px",
  padding: "1px 4px",
  fontSize: "9.5px",
  fontFamily: "var(--mono)",
  lineHeight: 1.4,
  marginRight: "3px",
};

export default function Overlay() {
  const [search, setSearch] = useState("");
  const [entries, setEntries] = useState<Entry[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [selectedEntry, setSelectedEntry] = useState<Entry | null>(null);
  const [locked, setLocked] = useState(false);
  const [password, setPassword] = useState("");
  const [unlockError, setUnlockError] = useState("");
  const [unlocking, setUnlocking] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const passwordRef = useRef<HTMLInputElement>(null);
  const selectedItemRef = useRef<HTMLDivElement>(null);

  const resetAndLoad = async () => {
    applyTheme(localStorage.getItem("vault_theme") ?? DEFAULT_THEME_ID);
    setSearch("");
    setSelectedEntry(null);
    setSelectedIndex(0);
    setPassword("");
    setUnlockError("");
    const ok = await isUnlocked();
    setLocked(!ok);
    if (ok) {
      const data = await listEntries();
      setEntries(data);
      setTimeout(() => inputRef.current?.focus(), 0);
    } else {
      setTimeout(() => passwordRef.current?.focus(), 0);
    }
  };

  useEffect(() => {
    const win = getCurrentWindow();
    const unlisten = win.onFocusChanged(({ payload: focused }) => {
      if (focused) resetAndLoad();
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  const handleUnlock = async (e: React.FormEvent) => {
    e.preventDefault();
    setUnlockError("");
    setUnlocking(true);
    try {
      await unlock(password);
      const data = await listEntries();
      setEntries(data);
      setLocked(false);
      setPassword("");
      await emit("vault-unlocked");
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

  const openEntry = (entry: Entry) => setSelectedEntry(entry);
  const closeEntry = () => {
    setSelectedEntry(null);
    setTimeout(() => inputRef.current?.focus(), 0);
  };

  useEffect(() => {
    if (selectedEntry) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") { getCurrentWindow().hide(); return; }
      if (locked) return;
      if (e.key === "ArrowDown") { e.preventDefault(); setSelectedIndex(i => Math.min(i + 1, filtered.length - 1)); }
      else if (e.key === "ArrowUp") { e.preventDefault(); setSelectedIndex(i => Math.max(i - 1, 0)); }
      else if (e.key === "Enter" && filtered[selectedIndex]) openEntry(filtered[selectedIndex]);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [selectedEntry, locked, filtered, selectedIndex]);

  useEffect(() => {
    if (!selectedEntry) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") { closeEntry(); }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [selectedEntry]);

  useEffect(() => setSelectedIndex(0), [search]);
  useEffect(() => { selectedItemRef.current?.scrollIntoView({ block: "nearest" }); }, [selectedIndex]);

  return (
    <div style={{
      display: "flex", flexDirection: "column", height: "100vh",
      background: "rgba(6,14,32,0.80)",
      backdropFilter: "blur(24px) saturate(200%)",
      WebkitBackdropFilter: "blur(24px) saturate(200%)",
      border: "1px solid rgba(255,255,255,0.10)",
      borderRadius: "var(--radius-lg)", overflow: "hidden",
      boxShadow: "0 16px 64px rgba(0,0,0,0.7), 0 0 80px rgba(30,80,200,0.15)",
    }}>
      {locked ? (
        <div style={{
          display: "flex", flexDirection: "column", alignItems: "center",
          justifyContent: "center", flex: 1, padding: "24px", gap: "12px",
        }}>
          <div style={{ fontSize: "14px", fontWeight: 500, marginBottom: "4px" }}>Vault is locked</div>
          <form onSubmit={handleUnlock} style={{ display: "flex", flexDirection: "column", gap: "8px", width: "100%" }}>
            <input
              ref={passwordRef}
              type="password"
              placeholder="Master password"
              value={password}
              onChange={(e) => { setPassword(e.target.value); setUnlockError(""); }}
              style={{ fontSize: "14px", padding: "10px 14px" }}
              autoFocus
            />
            {unlockError && <p className="error" style={{ margin: 0, fontSize: "12px" }}>{unlockError}</p>}
            <button type="submit" className="btn-primary" disabled={unlocking || password.length === 0} style={{ padding: "9px", fontSize: "13px" }}>
              {unlocking ? "Unlocking…" : "Unlock"}
            </button>
          </form>
          <span style={{ color: "var(--muted)", fontSize: "11px" }}>Escape → close</span>
        </div>
      ) : selectedEntry ? (
        <EntryDetail entry={selectedEntry} onClose={closeEntry} />
      ) : (
        <>
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

          <div style={{ flex: 1, overflowY: "auto" }}>
            {filtered.length === 0 && search ? (
              <div style={{ padding: "20px", color: "var(--muted)", fontSize: "13px", textAlign: "center" }}>
                No results for "{search}"
              </div>
            ) : (
              filtered.map((entry, i) => (
                <div
                  key={entry.id}
                  ref={i === selectedIndex ? selectedItemRef : null}
                  onClick={() => openEntry(entry)}
                  style={{
                    padding: "9px 14px", cursor: "pointer",
                    borderBottom: "1px solid var(--border)",
                    background: i === selectedIndex ? "rgba(77,157,224,0.12)" : "transparent",
                    borderLeft: i === selectedIndex ? "2px solid var(--accent)" : "2px solid transparent",
                    transition: "background 0.1s",
                    display: "flex", alignItems: "center", gap: "10px",
                  }}
                  onMouseEnter={() => setSelectedIndex(i)}
                >
                  <div className={`avatar ${avatarClass(entry.name)}`}
                    style={{ width: "28px", height: "28px", borderRadius: "7px", fontSize: "11px", flexShrink: 0 }}>
                    {entry.name.charAt(0).toUpperCase()}
                  </div>
                  <div style={{ minWidth: 0, flex: 1 }}>
                    <div style={{ fontWeight: 500, fontSize: "13px", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                      {entry.name}
                    </div>
                    <div style={{ color: "var(--muted)", fontSize: "11.5px", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                      {entry.email}
                    </div>
                  </div>
                  {entry.totp_secret && <TotpBadge secret={entry.totp_secret} />}
                </div>
              ))
            )}
          </div>

          <div style={{
            padding: "8px 14px", borderTop: "1px solid var(--border)",
            display: "flex", gap: "14px", color: "var(--muted)", fontSize: "11px",
          }}>
            <span>↑↓ navigate</span>
            <span>Enter → open</span>
            <span>Escape → close</span>
          </div>
        </>
      )}
    </div>
  );
}
