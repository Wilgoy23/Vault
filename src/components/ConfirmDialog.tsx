import { useEffect, useRef } from "react";
import { AlertTriangle } from "lucide-react";

interface Props {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  /** Styles the confirm button as destructive */
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

/** In-app replacement for window.confirm — matches the glass aesthetic
 * and keeps keyboard flow (Enter confirms, Escape cancels). */
export default function ConfirmDialog({
  title, message,
  confirmLabel = "Continue", cancelLabel = "Cancel",
  danger = false, onConfirm, onCancel,
}: Props) {
  const confirmRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    confirmRef.current?.focus();
    const handler = (e: KeyboardEvent) => {
      // Swallow the event so handlers underneath don't react, and prevent
      // the focused button's native Enter-click from double-firing
      if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onCancel(); }
      else if (e.key === "Enter") { e.preventDefault(); e.stopPropagation(); onConfirm(); }
    };
    window.addEventListener("keydown", handler, true);
    return () => window.removeEventListener("keydown", handler, true);
  }, [onConfirm, onCancel]);

  return (
    <div
      style={{
        position: "fixed", inset: 0, background: "rgba(3,8,20,0.55)",
        backdropFilter: "blur(4px)", WebkitBackdropFilter: "blur(4px)",
        display: "flex", alignItems: "center", justifyContent: "center", zIndex: 300,
      }}
      onClick={(e) => { if (e.target === e.currentTarget) onCancel(); }}
    >
      <div className="glass" style={{
        borderRadius: "var(--radius-lg)", padding: "20px", width: "340px",
        boxShadow: "0 8px 48px rgba(0,0,0,0.6), 0 0 60px rgba(30,80,200,0.08)",
      }}>
        <div style={{ display: "flex", alignItems: "flex-start", gap: "12px", marginBottom: "16px" }}>
          <div style={{
            width: "34px", height: "34px", borderRadius: "9px", flexShrink: 0,
            display: "flex", alignItems: "center", justifyContent: "center",
            background: danger ? "rgba(248,113,113,0.12)" : "rgba(77,157,224,0.12)",
            color: danger ? "var(--danger)" : "var(--accent)",
          }}>
            <AlertTriangle size={16} strokeWidth={2} />
          </div>
          <div style={{ minWidth: 0 }}>
            <div style={{ fontWeight: 600, fontSize: "14px", marginBottom: "4px" }}>{title}</div>
            <p style={{ fontSize: "12.5px", color: "var(--muted)", lineHeight: 1.55, margin: 0 }}>
              {message}
            </p>
          </div>
        </div>
        <div style={{ display: "flex", gap: "8px", justifyContent: "flex-end" }}>
          <button className="btn-ghost" style={{ fontSize: "13px", padding: "7px 14px" }} onClick={onCancel}>
            {cancelLabel}
          </button>
          <button
            ref={confirmRef}
            className="btn-primary"
            onClick={onConfirm}
            style={{
              fontSize: "13px", padding: "7px 14px",
              ...(danger ? {
                background: "rgba(248,113,113,0.16)",
                border: "1px solid rgba(248,113,113,0.4)",
                color: "var(--danger)",
              } : {}),
            }}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
