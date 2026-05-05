import { useState, useRef, useEffect } from "react";
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

  // Close popover on outside click
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (popoverRef.current && !popoverRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const refreshPreview = (o: GenOptions) => setPreview(generatePassword(o));

  const openGenerator = () => {
    refreshPreview(opts);
    setOpen(true);
  };

  const applyPassword = () => {
    onChange(preview);
    setOpen(false);
  };

  const toggleOpt = (key: keyof Omit<GenOptions, "length">) => {
    const next = { ...opts, [key]: !opts[key] };
    // Always keep at least one character class active
    const active = [next.upper, next.numbers, next.symbols, true]; // lower is always on
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
          className="btn-ghost"
          onClick={openGenerator}
          style={{ padding: "0 12px", fontSize: "13px", whiteSpace: "nowrap" }}
        >
          Generate
        </button>
      </div>

      {open && (
        <div
          ref={popoverRef}
          style={{
            position: "absolute",
            top: "calc(100% + 6px)",
            right: 0,
            zIndex: 200,
            background: "var(--bg2)",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius-lg)",
            padding: "16px",
            width: "300px",
            boxShadow: "0 8px 24px rgba(0,0,0,0.4)",
          }}
        >
          {/* Preview */}
          <div style={{
            fontFamily: "monospace",
            fontSize: "13px",
            background: "var(--bg)",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            padding: "8px 10px",
            marginBottom: "12px",
            wordBreak: "break-all",
            color: "var(--fg)",
            lineHeight: 1.5,
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
              className="btn-ghost"
              onClick={() => refreshPreview(opts)}
              style={{ padding: "2px 10px", fontSize: "12px" }}
            >
              Regenerate
            </button>
          </div>

          {/* Length slider */}
          <div style={{ marginBottom: "12px" }}>
            <div style={{ display: "flex", justifyContent: "space-between", fontSize: "12px", color: "var(--muted)", marginBottom: "4px" }}>
              <span>Length</span>
              <span style={{ color: "var(--fg)", fontWeight: 600 }}>{opts.length}</span>
            </div>
            <input
              type="range"
              min={8}
              max={64}
              value={opts.length}
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
                  padding: "4px 10px",
                  fontSize: "12px",
                  borderRadius: "var(--radius)",
                  border: `1px solid ${opts[key] ? "var(--accent)" : "var(--border)"}`,
                  background: opts[key] ? "rgba(99,102,241,0.15)" : "transparent",
                  color: opts[key] ? "var(--accent)" : "var(--muted)",
                  cursor: "pointer",
                  transition: "all 0.15s",
                }}
              >
                {label}
              </button>
            ))}
          </div>

          {/* Apply button */}
          <button
            type="button"
            className="btn-primary"
            onClick={applyPassword}
            style={{ width: "100%", fontSize: "13px" }}
          >
            Use this password
          </button>
        </div>
      )}
    </div>
  );
}
