import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { vaultExists, isUnlocked, getOverlayShortcut, setOverlayShortcut, lock, nativeDialogOpen } from "./api";
import LockScreen from "./components/LockScreen";
import MainWindow from "./components/MainWindow";
import { useIsMobile } from "./utils/platform";
import { applyTheme, applyStyle, savedAppearance, THEME_KEY, STYLE_KEY } from "./themes";
import "./app.css";

type Screen = "loading" | "lock" | "main";

const TIMEOUT_KEY = "vault_auto_lock_ms";
const DEFAULT_TIMEOUT = 5 * 60 * 1000;

function ContextMenu() {
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onContext = (e: MouseEvent) => {
      // Leave the native menu (Paste, etc.) on text fields
      const target = e.target as HTMLElement;
      if (target.closest("input, textarea, [contenteditable='true']")) return;
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
      className="float"
      style={{
        position: "fixed",
        top: pos.y,
        left: pos.x,
        zIndex: 9999,
        borderRadius: "var(--r-md)",
        padding: "4px",
        minWidth: "140px",
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
          borderRadius: "var(--r-sm)", cursor: "pointer",
          transition: "background 0.1s",
        }}
        onMouseEnter={(e) => (e.currentTarget.style.background = "var(--danger-glow)")}
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
  const isMobile = useIsMobile();

  const [timeoutMs, setTimeoutMs] = useState<number>(() => {
    const saved = localStorage.getItem(TIMEOUT_KEY);
    return saved !== null ? Number(saved) : DEFAULT_TIMEOUT;
  });

  const [themeId, setThemeId] = useState<string>(() => savedAppearance().themeId);
  const [styleId, setStyleId] = useState<string>(() => savedAppearance().styleId);

  const [shortcut, setShortcut] = useState("Ctrl+Shift+P");
  // Why the vault was locked, when it wasn't the user's doing
  const [lockNotice, setLockNotice] = useState("");
  // Bumped when another vault becomes active, so the lock screen starts over
  // for it (whether it exists yet, Hello, its sync folder)
  const [vaultGeneration, setVaultGeneration] = useState(0);

  useEffect(() => {
    getOverlayShortcut().then(setShortcut).catch(() => {});
  }, []);

  // Apply theme on mount and whenever it changes
  useEffect(() => {
    applyTheme(themeId);
  }, [themeId]);

  useEffect(() => {
    applyStyle(styleId);
  }, [styleId]);

  useEffect(() => {
    (async () => {
      const exists = await vaultExists();
      if (!exists) { setScreen("lock"); return; }
      const unlocked = await isUnlocked();
      setScreen(unlocked ? "main" : "lock");
    })();
  }, []);

  const handleTimeoutChange = (ms: number) => {
    setTimeoutMs(ms);
    localStorage.setItem(TIMEOUT_KEY, String(ms));
  };

  const handleThemeChange = (id: string) => {
    setThemeId(id);
    localStorage.setItem(THEME_KEY, id);
  };

  const handleStyleChange = (id: string) => {
    setStyleId(id);
    localStorage.setItem(STYLE_KEY, id);
  };

  // Throws on failure so Settings can show why the shortcut was rejected
  const handleShortcutChange = async (s: string) => {
    await setOverlayShortcut(s);
    setShortcut(s);
  };

  // Lets CSS target the phone layout by platform rather than window width
  useEffect(() => {
    document.documentElement.classList.toggle("is-mobile", isMobile);
  }, [isMobile]);

  // iOS pans the page up to keep a focused input above the keyboard and
  // doesn't always pan it back when the keyboard closes, leaving the app
  // shifted with a gap at the bottom. Snap back once focus leaves inputs.
  useEffect(() => {
    if (!isMobile) return;
    const onFocusOut = () => {
      setTimeout(() => {
        const el = document.activeElement;
        if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) return;
        window.scrollTo(0, 0);
      }, 50);
    };
    document.addEventListener("focusout", onFocusOut);
    return () => document.removeEventListener("focusout", onFocusOut);
  }, [isMobile]);

  // Mobile: lock as soon as the app leaves the foreground. The webview's
  // visibility is the only lifecycle signal the frontend gets on Android and
  // iOS, and it flips when the OS backgrounds the app — including when the
  // screen is turned off or the app switcher is opened.
  useEffect(() => {
    if (!isMobile || screen !== "main") return;
    const onVisibilityChange = () => {
      if (document.visibilityState !== "hidden") return;
      // A native file picker hides the webview too; that is not the app going
      // away, and locking there would break an import mid-flight.
      if (nativeDialogOpen()) return;
      lock().catch(() => {});
    };
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => document.removeEventListener("visibilitychange", onVisibilityChange);
  }, [isMobile, screen]);

  useEffect(() => {
    const unlistenUnlocked = listen("vault-unlocked", () => setScreen("main"));
    // Locks from the tray, the overlay, or a backup import
    const unlistenLocked = listen("vault-locked", () => setScreen("lock"));
    // The master password was changed on another device
    const unlistenSyncError = listen<string>("vault-sync-error", (e) => setLockNotice(e.payload));
    // Always follows a "vault-locked"; a notice was about the vault left behind
    const unlistenSwitched = listen("vault-switched", () => {
      setLockNotice("");
      setVaultGeneration((g) => g + 1);
      setScreen("lock");
    });
    return () => {
      unlistenUnlocked.then((f) => f());
      unlistenLocked.then((f) => f());
      unlistenSyncError.then((f) => f());
      unlistenSwitched.then((f) => f());
    };
  }, []);

  if (screen === "loading") {
    return (
      <>
        {!isMobile && <ContextMenu />}
        <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100%" }}>
          <span style={{ color: "var(--muted)" }}>Loading…</span>
        </div>
      </>
    );
  }

  if (screen === "lock") {
    return (
      <>
        {!isMobile && <ContextMenu />}
        <LockScreen key={vaultGeneration} notice={lockNotice} onUnlocked={() => { setLockNotice(""); setScreen("main"); }} />
      </>
    );
  }

  return (
    <>
      {!isMobile && <ContextMenu />}
      <MainWindow
        onLocked={() => setScreen("lock")}
        timeoutMs={timeoutMs}
        onTimeoutChange={handleTimeoutChange}
        themeId={themeId}
        onThemeChange={handleThemeChange}
        styleId={styleId}
        onStyleChange={handleStyleChange}
        shortcut={shortcut}
        onShortcutChange={handleShortcutChange}
      />
    </>
  );
}
