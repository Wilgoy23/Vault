import { useState, useRef, useEffect } from "react";
import { RefreshCw, Wand2 } from "lucide-react";
import { generatePassword, GenOptions, DEFAULT_OPTIONS } from "../utils/passwordGen";

interface Props {
  value: string;
  onChange: (value: string) => void;
}

export default function PasswordInput({ value, onChange }: Props) {
  const [open, setOpen] = useState(false);
  const [opts, setOpts] = useState<GenOptions>({ ...DEFAULT_OPTIONS });
  const [preview, setPreview] = useState("");
  const popoverRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (popoverRef.current && !popoverRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const refreshPreview = (o: GenOptions) => setPreview(generatePassword(o));

  const openGenerator = () => { refreshPreview(opts); setOpen(true); };

  const applyPassword = () => { onChange(preview); setOpen(false); };

  const toggleOpt = (key: keyof Omit<GenOptions, "length">) => {
    const next = { ...opts, [key]: !opts[key] };
    const active = [next.upper, next.numbers, next.symbols, true];
    if (!active.some(Boolean)) return;
    setOpts(next);
    refreshPreview(next);
  };

  const setLength = (len: number) => {
    const next = { ...opts, length: len };
    setOpts(next);
    refreshPreview(next);
  };

  const strength = (() => {
    const score = [opts.upper, opts.numbers, opts.symbols].filter(Boolean).length;
    if (opts.length >= 20 && score === 3) return { label: "Strong", color: "#4ade80" };
    if (opts.length >= 14 && score >= 2)  return { label: "Good",   color: "#facc15" };
    return { label: "Weak", color: "#f87171" };
  })();

  return (
    <div style={{ position: "relative" }}>
      <div style={{ display: "flex", gap: "6px" }}>
        <input
          type="text"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          style={{ flex: 1 }}
          placeholder="Password"
        />
        <button
          type="button"
          className="btn-icon"
          onClick={openGenerator}
          title="Generate password"
          style={{ width: "36px", height: "36px", flexShrink: 0 }}
        >
          <Wand2 size={14} strokeWidth={2} />
        </button>
      </div>

      {open && (
        <div
          ref={popoverRef}
          style={{
            position: "absolute", top: "calc(100% + 6px)", right: 0, zIndex: 200,
            background: "rgba(8,18,40,0.88)",
            backdropFilter: "blur(20px) saturate(200%)",
            WebkitBackdropFilter: "blur(20px) saturate(200%)",
            border: "1px solid rgba(255,255,255,0.12)",
            borderRadius: "var(--radius-lg)", padding: "16px", width: "300px",
            boxShadow: "0 12px 40px rgba(0,0,0,0.6), 0 0 40px rgba(30,80,200,0.10)",
          }}
        >
          {/* Preview */}
          <div style={{
            fontFamily: "monospace", fontSize: "13px",
            background: "rgba(255,255,255,0.05)",
            border: "1px solid rgba(255,255,255,0.10)",
            borderRadius: "var(--radius)", padding: "8px 10px", marginBottom: "12px",
            wordBreak: "break-all", lineHeight: 1.5,
          }}>
            {preview}
          </div>

          {/* Strength + refresh */}
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "12px" }}>
            <span style={{ fontSize: "12px", color: strength.color, fontWeight: 600 }}>
              {strength.label}
            </span>
            <button
              type="button"
              className="btn-icon"
              onClick={() => refreshPreview(opts)}
              title="Regenerate"
              style={{ width: "28px", height: "28px" }}
            >
              <RefreshCw size={12} strokeWidth={2} />
            </button>
          </div>

          {/* Length slider */}
          <div style={{ marginBottom: "12px" }}>
            <div style={{ display: "flex", justifyContent: "space-between", fontSize: "12px", color: "var(--muted)", marginBottom: "4px" }}>
              <span>Length</span>
              <span style={{ color: "var(--text)", fontWeight: 600 }}>{opts.length}</span>
            </div>
            <input
              type="range" min={8} max={64} value={opts.length}
              onChange={(e) => setLength(Number(e.target.value))}
              style={{ width: "100%", accentColor: "var(--accent)" }}
            />
          </div>

          {/* Character set toggles */}
          <div style={{ display: "flex", gap: "6px", marginBottom: "14px", flexWrap: "wrap" }}>
            {([ ["upper", "A–Z"], ["numbers", "0–9"], ["symbols", "!@#"] ] as const).map(([key, label]) => (
              <button
                key={key}
                type="button"
                onClick={() => toggleOpt(key)}
                style={{
                  padding: "4px 10px", fontSize: "12px", borderRadius: "var(--radius)",
                  border: `1px solid ${opts[key] ? "var(--accent)" : "var(--border)"}`,
                  background: opts[key] ? "rgba(77,157,224,0.12)" : "transparent",
                  color: opts[key] ? "var(--accent)" : "var(--muted)",
                  cursor: "pointer", transition: "all 0.15s",
                }}
              >
                {label}
              </button>
            ))}
          </div>

          <button type="button" className="btn-primary" onClick={applyPassword} style={{ width: "100%", fontSize: "13px" }}>
            Use this password
          </button>
        </div>
      )}
    </div>
  );
}
