import { useState, useRef, useEffect } from "react";
import { createPortal } from "react-dom";
import { RefreshCw, Wand2 } from "lucide-react";
import {
  generatePassword, GenOptions, DEFAULT_OPTIONS,
  generatePassphrase, PassphraseOptions, DEFAULT_PASSPHRASE,
  passphraseEntropyBits, SEPARATORS,
} from "../utils/passwordGen";

interface Props {
  value: string;
  onChange: (value: string) => void;
}

// Approx. rendered height of the generator popover; used to decide
// whether it should open below or flip above the input.
const POPOVER_HEIGHT = 300;
const POPOVER_WIDTH = 300;

export default function PasswordInput({ value, onChange }: Props) {
  const [open, setOpen] = useState(false);
  const [mode, setMode] = useState<"password" | "passphrase">("password");
  const [opts, setOpts] = useState<GenOptions>({ ...DEFAULT_OPTIONS });
  const [phraseOpts, setPhraseOpts] = useState<PassphraseOptions>({ ...DEFAULT_PASSPHRASE });
  const [preview, setPreview] = useState("");
  const [coords, setCoords] = useState<{ top: number; left: number }>({ top: 0, left: 0 });
  const anchorRef = useRef<HTMLDivElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);

  // Position the (portaled, fixed) popover relative to the input row,
  // flipping above the input when there isn't room below. Rendering in a
  // portal keeps it from being clipped by scrollable/overflow ancestors.
  const reposition = () => {
    const el = anchorRef.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const spaceBelow = window.innerHeight - rect.bottom;
    const top = spaceBelow >= POPOVER_HEIGHT + 6 || spaceBelow >= rect.top
      ? rect.bottom + 6
      : rect.top - POPOVER_HEIGHT - 6;
    const left = Math.max(8, rect.right - POPOVER_WIDTH);
    setCoords({ top, left });
  };

  useEffect(() => {
    if (!open) return;
    reposition();
    const handler = (e: MouseEvent) => {
      const target = e.target as Node;
      if (
        popoverRef.current && !popoverRef.current.contains(target) &&
        anchorRef.current && !anchorRef.current.contains(target)
      ) setOpen(false);
    };
    document.addEventListener("mousedown", handler);
    // Reposition while scrolling any ancestor (capture) or resizing.
    window.addEventListener("scroll", reposition, true);
    window.addEventListener("resize", reposition);
    return () => {
      document.removeEventListener("mousedown", handler);
      window.removeEventListener("scroll", reposition, true);
      window.removeEventListener("resize", reposition);
    };
  }, [open]);

  const refreshPreview = (o: GenOptions) => setPreview(generatePassword(o));
  const refreshPhrase = (o: PassphraseOptions) => setPreview(generatePassphrase(o));

  const regenerate = (next: "password" | "passphrase" = mode) =>
    next === "password" ? refreshPreview(opts) : refreshPhrase(phraseOpts);

  const switchMode = (next: "password" | "passphrase") => {
    setMode(next);
    regenerate(next);
  };

  const openGenerator = () => { refreshPreview(opts); setMode("password"); setOpen(true); };

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

  const setPhrase = (patch: Partial<PassphraseOptions>) => {
    const next = { ...phraseOpts, ...patch };
    setPhraseOpts(next);
    refreshPhrase(next);
  };

  const strength = (() => {
    const score = [opts.upper, opts.numbers, opts.symbols].filter(Boolean).length;
    if (opts.length >= 20 && score === 3) return { label: "Strong", color: "var(--success-fg)" };
    if (opts.length >= 14 && score >= 2)  return { label: "Good",   color: "var(--warning)" };
    return { label: "Weak", color: "var(--danger)" };
  })();

  return (
    <div style={{ position: "relative" }}>
      <div ref={anchorRef} style={{ display: "flex", gap: "6px" }}>
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

      {open && createPortal(
        <div
          ref={popoverRef}
          className="float"
          style={{
            position: "fixed", top: coords.top, left: coords.left, zIndex: 1000,
            borderRadius: "var(--radius-lg)", padding: "16px", width: "300px",
          }}
        >
          {/* Mode */}
          <div style={{ display: "flex", gap: "4px", marginBottom: "12px" }}>
            {([["password", "Password"], ["passphrase", "Passphrase"]] as const).map(([key, label]) => (
              <button
                key={key}
                type="button"
                onClick={() => switchMode(key)}
                className={mode === key ? "chip on" : "chip"}
                style={{
                  flex: 1, padding: "5px 0", fontSize: "12px", fontWeight: 600,
                  borderRadius: "var(--radius)",
                  border: `1px solid ${mode === key ? "var(--accent)" : "var(--border)"}`,
                  background: mode === key ? "var(--accent-tint)" : "transparent",
                  color: mode === key ? "var(--accent)" : "var(--muted)",
                  cursor: "pointer", transition: "all 0.15s",
                }}
              >
                {label}
              </button>
            ))}
          </div>

          {/* Preview */}
          <div className="well" style={{
            fontFamily: "monospace", fontSize: "13px",
            borderRadius: "var(--radius)", padding: "8px 10px", marginBottom: "12px",
            wordBreak: "break-all", lineHeight: 1.5,
          }}>
            {preview}
          </div>

          {/* Strength + refresh */}
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "12px" }}>
            {mode === "password" ? (
              <span style={{ fontSize: "12px", color: strength.color, fontWeight: 600 }}>
                {strength.label}
              </span>
            ) : (
              <span style={{ fontSize: "12px", color: "var(--muted)" }}>
                ~{Math.round(passphraseEntropyBits(phraseOpts))} bits of entropy
              </span>
            )}
            <button
              type="button"
              className="btn-icon"
              onClick={() => regenerate()}
              title="Regenerate"
              style={{ width: "28px", height: "28px" }}
            >
              <RefreshCw size={12} strokeWidth={2} />
            </button>
          </div>

          {mode === "password" ? (
            <>
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
                  className={opts[key] ? "chip on" : "chip"}
                  style={{
                    padding: "4px 10px", fontSize: "12px", borderRadius: "var(--radius)",
                    border: `1px solid ${opts[key] ? "var(--accent)" : "var(--border)"}`,
                    background: opts[key] ? "var(--accent-tint)" : "transparent",
                    color: opts[key] ? "var(--accent)" : "var(--muted)",
                    cursor: "pointer", transition: "all 0.15s",
                  }}
                >
                  {label}
                </button>
              ))}
            </div>
            </>
          ) : (
            <>
              {/* Word count */}
              <div style={{ marginBottom: "12px" }}>
                <div style={{ display: "flex", justifyContent: "space-between", fontSize: "12px", color: "var(--muted)", marginBottom: "4px" }}>
                  <span>Words</span>
                  <span style={{ color: "var(--text)", fontWeight: 600 }}>{phraseOpts.words}</span>
                </div>
                <input
                  type="range" min={3} max={10} value={phraseOpts.words}
                  onChange={(e) => setPhrase({ words: Number(e.target.value) })}
                  style={{ width: "100%", accentColor: "var(--accent)" }}
                />
              </div>

              {/* Separator */}
              <div style={{ marginBottom: "12px" }}>
                <div style={{ fontSize: "12px", color: "var(--muted)", marginBottom: "4px" }}>Separator</div>
                <div style={{ display: "flex", gap: "6px" }}>
                  {SEPARATORS.map((sep) => (
                    <button
                      key={sep}
                      type="button"
                      onClick={() => setPhrase({ separator: sep })}
                      title={sep === " " ? "Space" : `"${sep}"`}
                      className={phraseOpts.separator === sep ? "chip on" : "chip"}
                      style={{
                        width: "34px", padding: "4px 0", fontSize: "12px",
                        fontFamily: "var(--mono)", borderRadius: "var(--radius)",
                        border: `1px solid ${phraseOpts.separator === sep ? "var(--accent)" : "var(--border)"}`,
                        background: phraseOpts.separator === sep ? "var(--accent-tint)" : "transparent",
                        color: phraseOpts.separator === sep ? "var(--accent)" : "var(--muted)",
                        cursor: "pointer", transition: "all 0.15s",
                      }}
                    >
                      {sep === " " ? "␣" : sep}
                    </button>
                  ))}
                </div>
              </div>

              {/* Extras */}
              <div style={{ display: "flex", gap: "6px", marginBottom: "14px", flexWrap: "wrap" }}>
                {([["capitalize", "Capitalise"], ["number", "Add digit"]] as const).map(([key, label]) => (
                  <button
                    key={key}
                    type="button"
                    onClick={() => setPhrase({ [key]: !phraseOpts[key] })}
                    className={phraseOpts[key] ? "chip on" : "chip"}
                    style={{
                      padding: "4px 10px", fontSize: "12px", borderRadius: "var(--radius)",
                      border: `1px solid ${phraseOpts[key] ? "var(--accent)" : "var(--border)"}`,
                      background: phraseOpts[key] ? "var(--accent-tint)" : "transparent",
                      color: phraseOpts[key] ? "var(--accent)" : "var(--muted)",
                      cursor: "pointer", transition: "all 0.15s",
                    }}
                  >
                    {label}
                  </button>
                ))}
              </div>
            </>
          )}

          <button type="button" className="btn-primary" onClick={applyPassword} style={{ width: "100%", fontSize: "13px" }}>
            {mode === "password" ? "Use this password" : "Use this passphrase"}
          </button>
        </div>,
        document.body
      )}
    </div>
  );
}
