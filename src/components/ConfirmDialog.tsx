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

/** In-app replacement for window.confirm — drawn in the current UI style
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
      className="scrim"
      style={{
        position: "fixed", inset: 0,
        padding: "env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left)",
        display: "flex", alignItems: "center", justifyContent: "center", zIndex: 300,
      }}
      onClick={(e) => { if (e.target === e.currentTarget) onCancel(); }}
    >
      <div className="panel modal" style={{
        borderRadius: "var(--radius-lg)", padding: "20px", width: "min(340px, calc(100vw - 24px))",
      }}>
        <div style={{ display: "flex", alignItems: "flex-start", gap: "12px", marginBottom: "16px" }}>
          <div className={danger ? "tint-danger" : "tint"} style={{
            width: "34px", height: "34px", borderRadius: "var(--r-md)", flexShrink: 0,
            display: "flex", alignItems: "center", justifyContent: "center",
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
            className={danger ? "btn-primary danger" : "btn-primary"}
            onClick={onConfirm}
            style={{ fontSize: "13px", padding: "7px 14px" }}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
