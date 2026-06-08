import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { vaultExists, isUnlocked, lock, getOverlayShortcut, setOverlayShortcut } from "./api";
import LockScreen from "./components/LockScreen";
import MainWindow from "./components/MainWindow";
import { useAutoLock } from "./utils/useAutoLock";
import { applyTheme, DEFAULT_THEME_ID } from "./themes";
import "./App.css";

type Screen = "loading" | "lock" | "main";

const TIMEOUT_KEY = "vault_auto_lock_ms";
const THEME_KEY   = "vault_theme";
const DEFAULT_TIMEOUT = 5 * 60 * 1000;

function ContextMenu() {
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onContext = (e: MouseEvent) => {
      e.preventDefault();
      setPos({ x: e.clientX, y: e.clientY });
    };
    const onDismiss = (e: MouseEvent | KeyboardEvent) => {
      if (e instanceof KeyboardEvent && e.key !== "Escape") return;
      if (e instanceof MouseEvent && menuRef.current?.contains(e.target as Node)) return;
      setPos(null);
    };
    document.addEventListener("contextmenu", onContext);
    document.addEventListener("mousedown", onDismiss);
    document.addEventListener("keydown", onDismiss);
    return () => {
      document.removeEventListener("contextmenu", onContext);
      document.removeEventListener("mousedown", onDismiss);
      document.removeEventListener("keydown", onDismiss);
    };
  }, []);

  if (!pos) return null;

  const handleClose = async () => {
    setPos(null);
    await getCurrentWindow().close();
  };

  return (
    <div
      ref={menuRef}
      style={{
        position: "fixed",
        top: pos.y,
        left: pos.x,
        zIndex: 9999,
        background: "rgba(8,18,38,0.96)",
        backdropFilter: "blur(16px)",
        WebkitBackdropFilter: "blur(16px)",
        border: "1px solid rgba(255,255,255,0.10)",
        borderRadius: "8px",
        padding: "4px",
        minWidth: "140px",
        boxShadow: "0 8px 32px rgba(0,0,0,0.6)",
      }}
    >
      <button
        onClick={handleClose}
        style={{
          display: "flex", alignItems: "center", gap: "8px",
          width: "100%", textAlign: "left",
          padding: "7px 10px",
          background: "transparent", border: "none",
          color: "var(--danger)", fontSize: "13px", fontWeight: 500,
          borderRadius: "5px", cursor: "pointer",
          transition: "background 0.1s",
        }}
        onMouseEnter={(e) => (e.currentTarget.style.background = "rgba(248,113,113,0.10)")}
        onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
      >
        <svg width="13" height="13" viewBox="0 0 13 13" fill="none">
          <path d="M2 2L11 11M11 2L2 11" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round"/>
        </svg>
        Close Vault
      </button>
    </div>
  );
}

export default function App() {
  const [screen, setScreen] = useState<Screen>("loading");

  const [timeoutMs, setTimeoutMs] = useState<number>(() => {
    const saved = localStorage.getItem(TIMEOUT_KEY);
    return saved !== null ? Number(saved) : DEFAULT_TIMEOUT;
  });

  const [themeId, setThemeId] = useState<string>(() => {
    return localStorage.getItem(THEME_KEY) ?? DEFAULT_THEME_ID;
  });

  const [shortcut, setShortcut] = useState("Ctrl+Shift+P");

  useEffect(() => {
    getOverlayShortcut().then(setShortcut).catch(() => {});
  }, []);

  // Apply theme on mount and whenever it changes
  useEffect(() => {
    applyTheme(themeId);
  }, [themeId]);

  useEffect(() => {
    (async () => {
      const exists = await vaultExists();
      if (!exists) { setScreen("lock"); return; }
      const unlocked = await isUnlocked();
      setScreen(unlocked ? "main" : "lock");
    })();
  }, []);

  const handleLock = async () => {
    await lock();
    setScreen("lock");
  };

  const handleTimeoutChange = (ms: number) => {
    setTimeoutMs(ms);
    localStorage.setItem(TIMEOUT_KEY, String(ms));
  };

  const handleThemeChange = (id: string) => {
    setThemeId(id);
    localStorage.setItem(THEME_KEY, id);
  };

  const handleShortcutChange = async (s: string) => {
    try {
      await setOverlayShortcut(s);
      setShortcut(s);
    } catch (e) {
      console.error("Failed to set shortcut:", e);
    }
  };

  useEffect(() => {
    const unlisten = listen("vault-unlocked", () => setScreen("main"));
    return () => { unlisten.then((f) => f()); };
  }, []);

  useAutoLock(timeoutMs, handleLock, screen === "main");

  if (screen === "loading") {
    return (
      <>
        <ContextMenu />
        <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100vh" }}>
          <span style={{ color: "var(--muted)" }}>Loading…</span>
        </div>
      </>
    );
  }

  if (screen === "lock") {
    return (
      <>
        <ContextMenu />
        <LockScreen onUnlocked={() => setScreen("main")} />
      </>
    );
  }

  return (
    <>
      <ContextMenu />
      <MainWindow
        onLocked={handleLock}
        timeoutMs={timeoutMs}
        onTimeoutChange={handleTimeoutChange}
        themeId={themeId}
        onThemeChange={handleThemeChange}
        shortcut={shortcut}
        onShortcutChange={handleShortcutChange}
      />
    </>
  );
}
