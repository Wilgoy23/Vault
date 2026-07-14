import { masterPasswordFeedback } from "../utils/password";

const BAR_COLORS = ["", "#F87171", "#FB923C", "#FBBF24", "#4ADE80"];

/** Live strength bars + hint for a master password being chosen.
 * Renders nothing while the field is empty. */
export default function MasterPasswordMeter({ password }: { password: string }) {
  if (!password) return null;
  const feedback = masterPasswordFeedback(password);
  return (
    <div>
      <div style={{ display: "flex", gap: "4px", marginBottom: "6px" }}>
        {[1, 2, 3, 4].map((i) => (
          <div key={i} style={{
            flex: 1, height: "3px", borderRadius: "2px",
            background: i <= feedback.score ? BAR_COLORS[feedback.score] : "rgba(255,255,255,0.10)",
            transition: "background 0.2s",
          }} />
        ))}
      </div>
      <div style={{ display: "flex", justifyContent: "space-between", gap: "8px" }}>
        <span style={{ fontSize: "11px", color: "var(--muted)", lineHeight: 1.4 }}>
          {feedback.hint ?? "Nice — this is a strong master password."}
        </span>
        <span style={{
          fontSize: "11px", fontWeight: 700, flexShrink: 0,
          color: BAR_COLORS[feedback.score] || "var(--muted)",
        }}>
          {feedback.label}
        </span>
      </div>
    </div>
  );
}
