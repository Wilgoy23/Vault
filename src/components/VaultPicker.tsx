import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown, Plus, X, FolderSync } from "lucide-react";
import { VaultList, switchVault, addVault, removeVault } from "../api";

interface Props {
  list: VaultList;
  /** Reports errors the picker can't show inside its own menu */
  onError: (message: string) => void;
  /** Runs before the open vault is locked, e.g. to write a pending delete */
  beforeChange?: () => Promise<void>;
  /** "lock": centred above the lock form; "titlebar": inline, menu to the left */
  placement?: "lock" | "titlebar";
}

/** Switches to another vault on this device, or adds one. The backend locks
 *  and emits "vault-switched"; App remounts the lock screen for the newly
 *  active vault. */
export default function VaultPicker({ list, onError, beforeChange, placement = "lock" }: Props) {
  const [open, setOpen] = useState(false);
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState("");
  const [addError, setAddError] = useState("");
  const [busy, setBusy] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  const active = list.vaults.find((v) => v.id === list.active);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) close();
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") close(); };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const close = () => {
    setOpen(false);
    setAdding(false);
    setName("");
    setAddError("");
  };

  const handleSwitch = async (id: string) => {
    close();
    if (id === list.active) return;
    try {
      await beforeChange?.();
      await switchVault(id);
    }
    catch (err: any) { onError(err?.toString() ?? "Could not switch vaults."); }
  };

  const handleAdd = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setAddError("");
    try {
      await beforeChange?.();
      await addVault(name);
      close();
    } catch (err: any) {
      setAddError(err?.toString() ?? "Could not add the vault.");
    } finally {
      setBusy(false);
    }
  };

  // Only vaults never given a password can go from here; the rest need
  // unlocking first, from Settings
  const handleDiscard = async (id: string) => {
    try { await removeVault(id); }
    catch (err: any) { onError(err?.toString() ?? "Could not remove the vault."); }
  };

  const inTitlebar = placement === "titlebar";

  return (
    <div
      ref={rootRef}
      style={inTitlebar
        ? { position: "relative", display: "flex", minWidth: 0 }
        : { position: "relative", display: "flex", justifyContent: "center", marginBottom: "18px" }}
    >
      <button
        type="button"
        className="btn-ghost"
        onClick={() => (open ? close() : setOpen(true))}
        aria-haspopup="menu"
        aria-expanded={open}
        title="Switch vault"
        style={{
          display: "flex", alignItems: "center", gap: "6px",
          padding: inTitlebar ? "4px 8px 4px 10px" : "6px 10px 6px 12px",
          fontSize: inTitlebar ? "12.5px" : "13px", fontWeight: 500,
          maxWidth: inTitlebar ? "200px" : "100%",
        }}
      >
        <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {active?.name ?? "Vault"}
        </span>
        <ChevronDown
          size={14} strokeWidth={2}
          style={{ flexShrink: 0, transform: open ? "rotate(180deg)" : "none", transition: "transform 0.15s" }}
        />
      </button>

      {open && (
        <div
          role="menu"
          style={{
            position: "absolute", top: "calc(100% + 6px)",
            ...(inTitlebar ? { left: 0 } : { left: "50%", transform: "translateX(-50%)" }),
            zIndex: 50, width: "min(280px, calc(100vw - 48px))",
            background: "var(--select-bg)", border: "1px solid var(--border)",
            borderRadius: "var(--r-lg)", padding: "4px",
            boxShadow: "0 8px 32px rgba(0,0,0,0.5)",
          }}
        >
          {list.vaults.map((v) => (
            <div key={v.id} style={{ display: "flex", alignItems: "center" }}>
              <button
                type="button"
                role="menuitemradio"
                aria-checked={v.id === list.active}
                onClick={() => handleSwitch(v.id)}
                style={itemStyle}
                onMouseEnter={hoverOn}
                onMouseLeave={hoverOff}
              >
                <span style={{ width: "14px", flexShrink: 0, display: "flex" }}>
                  {v.id === list.active && <Check size={14} strokeWidth={2.5} color="var(--accent)" />}
                </span>
                <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                  {v.name}
                </span>
                {v.synced && <FolderSync size={13} strokeWidth={2} color="var(--muted)" aria-label="Synced" />}
                {!v.exists && <span style={{ fontSize: "11px", color: "var(--muted)" }}>Not set up</span>}
              </button>
              {!v.exists && list.vaults.length > 1 && (
                <button
                  type="button"
                  className="btn-icon"
                  onClick={() => handleDiscard(v.id)}
                  title="Remove"
                  aria-label={`Remove ${v.name}`}
                  style={{ padding: "5px", flexShrink: 0 }}
                >
                  <X size={13} strokeWidth={2} />
                </button>
              )}
            </div>
          ))}

          <div style={{ borderTop: "1px solid var(--border)", margin: "4px 0" }} />

          {adding ? (
            <form onSubmit={handleAdd} style={{ display: "flex", flexDirection: "column", gap: "8px", padding: "6px" }}>
              <input
                placeholder="Name, e.g. Work"
                value={name}
                onChange={(e) => { setName(e.target.value); setAddError(""); }}
                maxLength={40}
                autoFocus
              />
              {addError && <p className="error" style={{ margin: 0, fontSize: "12px" }}>{addError}</p>}
              <button type="submit" className="btn-primary" disabled={busy || !name.trim()} style={{ fontSize: "13px", padding: "7px" }}>
                {busy ? "Adding…" : "Add vault"}
              </button>
            </form>
          ) : (
            <button
              type="button"
              role="menuitem"
              onClick={() => setAdding(true)}
              style={itemStyle}
              onMouseEnter={hoverOn}
              onMouseLeave={hoverOff}
            >
              <Plus size={14} strokeWidth={2} />
              Add a vault…
            </button>
          )}
        </div>
      )}
    </div>
  );
}

const itemStyle: React.CSSProperties = {
  display: "flex", alignItems: "center", gap: "8px",
  flex: 1, minWidth: 0, width: "100%", textAlign: "left",
  padding: "8px 10px", background: "transparent", border: "none",
  color: "var(--text)", fontSize: "13px", borderRadius: "var(--r-sm)", cursor: "pointer",
};

const hoverOn = (e: React.MouseEvent<HTMLButtonElement>) => { e.currentTarget.style.background = "var(--surface-hi)"; };
const hoverOff = (e: React.MouseEvent<HTMLButtonElement>) => { e.currentTarget.style.background = "transparent"; };
