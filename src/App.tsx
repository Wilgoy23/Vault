import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { vaultExists, isUnlocked, lock } from "./api";
import LockScreen from "./components/LockScreen";
import MainWindow from "./components/MainWindow";
import { useAutoLock } from "./utils/useAutoLock";
import { applyTheme, DEFAULT_THEME_ID } from "./themes";
import "./App.css";

type Screen = "loading" | "lock" | "main";

const TIMEOUT_KEY = "vault_auto_lock_ms";
const THEME_KEY   = "vault_theme";
const DEFAULT_TIMEOUT = 5 * 60 * 1000;

export default function App() {
  const [screen, setScreen] = useState<Screen>("loading");

  const [timeoutMs, setTimeoutMs] = useState<number>(() => {
    const saved = localStorage.getItem(TIMEOUT_KEY);
    return saved !== null ? Number(saved) : DEFAULT_TIMEOUT;
  });

  const [themeId, setThemeId] = useState<string>(() => {
    return localStorage.getItem(THEME_KEY) ?? DEFAULT_THEME_ID;
  });

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

  useEffect(() => {
    const unlisten = listen("vault-unlocked", () => setScreen("main"));
    return () => { unlisten.then((f) => f()); };
  }, []);

  useAutoLock(timeoutMs, handleLock, screen === "main");

  if (screen === "loading") {
    return (
      <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100vh" }}>
        <span style={{ color: "var(--muted)" }}>Loading…</span>
      </div>
    );
  }

  if (screen === "lock") {
    return <LockScreen onUnlocked={() => setScreen("main")} />;
  }

  return (
    <MainWindow
      onLocked={handleLock}
      timeoutMs={timeoutMs}
      onTimeoutChange={handleTimeoutChange}
      themeId={themeId}
      onThemeChange={handleThemeChange}
    />
  );
}
