import { useEffect, useState } from "react";
import { vaultExists, isUnlocked } from "./api";
import LockScreen from "./components/LockScreen";
import MainWindow from "./components/MainWindow";
import "./app.css";

type Screen = "loading" | "lock" | "main";

export default function App() {
  const [screen, setScreen] = useState<Screen>("loading");

  useEffect(() => {
    (async () => {
      const exists = await vaultExists();
      if (!exists) { setScreen("lock"); return; }
      const unlocked = await isUnlocked();
      setScreen(unlocked ? "main" : "lock");
    })();
  }, []);

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

  return <MainWindow onLocked={() => setScreen("lock")} />;
}