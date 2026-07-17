import { useEffect, useState } from "react";
import { Undo2 } from "lucide-react";

interface Props {
  message: string;
  /** Epoch ms when the pending delete commits */
  deadline: number;
  onUndo: () => void;
}

export default function UndoToast({ message, deadline, onUndo }: Props) {
  const [secondsLeft, setSecondsLeft] = useState(() => Math.ceil((deadline - Date.now()) / 1000));

  useEffect(() => {
    const iv = setInterval(() => {
      setSecondsLeft(Math.max(0, Math.ceil((deadline - Date.now()) / 1000)));
    }, 250);
    return () => clearInterval(iv);
  }, [deadline]);

  return (
    <div style={{
      position: "fixed", bottom: "18px", left: "50%", transform: "translateX(-50%)",
      zIndex: 200, display: "flex", alignItems: "center", gap: "12px",
      padding: "9px 10px 9px 16px",
      background: "rgba(8,18,40,0.92)",
      backdropFilter: "blur(20px) saturate(180%)",
      WebkitBackdropFilter: "blur(20px) saturate(180%)",
      border: "1px solid rgba(255,255,255,0.12)",
      borderRadius: "var(--r-md)",
      boxShadow: "0 8px 32px rgba(0,0,0,0.5)",
      whiteSpace: "nowrap",
    }}>
      <span style={{ fontSize: "13px", color: "var(--fg-mid)" }}>{message}</span>
      <button
        onClick={onUndo}
        style={{
          display: "flex", alignItems: "center", gap: "5px",
          padding: "5px 12px", fontSize: "12.5px", fontWeight: 600,
          color: "var(--accent)", background: "var(--accent-tint)",
          border: "1px solid rgba(77,157,224,0.3)", borderRadius: "6px",
          cursor: "pointer",
        }}
      >
        <Undo2 size={13} strokeWidth={2.25} />
        Undo ({secondsLeft})
      </button>
    </div>
  );
}
