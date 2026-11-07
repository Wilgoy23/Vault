import { useEffect, useState } from "react";
import { ShieldCheck, Lock, Eye, EyeOff, Check, Fingerprint, FolderSync, Upload, FileSpreadsheet, X, ChevronDown } from "lucide-react";
import { createVault, unlock, vaultExists, quickUnlockArmed, quickUnlock, syncStatus, openSyncedVault, importVault, stageCsvImport, clearStagedCsvImport, applyStagedCsvImport, SyncStatus } from "../api";
import MasterPasswordMeter from "./MasterPasswordMeter";

interface Props {
  /** Shown above the form, e.g. why the vault was locked */
  notice?: string;
  onUnlocked: () => void;
}

export default function LockScreen({ notice, onUnlocked }: Props) {
  const [isNew, setIsNew] = useState<boolean | null>(null);
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [showPw, setShowPw] = useState(false);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [helloArmed, setHelloArmed] = useState(false);
  const [helloBusy, setHelloBusy] = useState(false);
  const [sync, setSync] = useState<SyncStatus | null>(null);
  // Logins in a CSV picked before the vault exists; imported on create
  const [stagedCsv, setStagedCsv] = useState<number | null>(null);
  // First-run extras (import, sync folder) stay tucked away until asked for
  const [showMore, setShowMore] = useState(false);

  useEffect(() => {
    vaultExists().then((exists) => setIsNew(!exists));
    syncStatus().then(setSync).catch(() => {});
    // False off Windows, and whenever this session has not unlocked yet
    quickUnlockArmed().then(setHelloArmed).catch(() => {});
  }, []);

  const handleHello = async () => {
    setError("");
    setHelloBusy(true);
    try {
      await quickUnlock();
      onUnlocked();
    } catch (err: any) {
      const message = err?.toString() ?? "";
      // Walking away from the prompt is not an error worth shouting about
      if (!message.includes("Cancelled")) {
        setError(message || "Windows Hello could not unlock the vault.");
      }
    } finally {
      setHelloBusy(false);
    }
  };

  // Second device: use the vault the first one put in a sync folder
  const handleOpenSynced = async () => {
    setError("");
    try {
      if (await openSyncedVault()) {
        setIsNew(false);
        syncStatus().then(setSync).catch(() => {});
      }
    } catch (err: any) {
      setError(err?.toString() ?? "Could not open that folder.");
    }
  };

  // First run: restore an existing .enc backup instead of creating a new
  // vault. The backup keeps its own master password, so switch to unlock.
  const handleImportBackup = async () => {
    setError("");
    try {
      if (await importVault()) {
        if (stagedCsv !== null) { clearStagedCsvImport().catch(() => {}); setStagedCsv(null); }
        setIsNew(false);
        setPassword("");
        setConfirm("");
      }
    } catch (err: any) {
      setError(err?.toString() ?? "Import failed.");
    }
  };

  const handleStageCsv = async () => {
    setError("");
    try {
      const count = await stageCsvImport();
      if (count !== null) setStagedCsv(count);
    } catch (err: any) {
      setError(err?.toString() ?? "Could not read that file.");
    }
  };

  const handleClearCsv = () => {
    clearStagedCsvImport().catch(() => {});
    setStagedCsv(null);
  };

  const confirmMismatch = isNew === true && confirm.length > 0 && confirm !== password;
  const confirmMatches = isNew === true && confirm.length > 0 && confirm === password;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    if (isNew && password !== confirm) { setError("Passwords don't match."); return; }
    if (password.length < 8) { setError("Password must be at least 8 characters."); return; }
    setLoading(true);
    try {
      if (isNew) { await createVault(password); } else { await unlock(password); }
      if (isNew && stagedCsv !== null) {
        // The vault exists now either way, so a failed import must not keep
        // the user on the create screen; they can retry from Settings.
        try {
          await applyStagedCsvImport();
        } catch (err: any) {
          window.alert(`Your vault was created, but the CSV import failed: ${err?.toString() ?? "unknown error"}\n\nYou can try again from Settings → Import CSV.`);
        }
      }
      onUnlocked();
    } catch (err: any) {
      setError(err?.toString() ?? "Something went wrong.");
    } finally {
      setLoading(false);
    }
  };

  if (isNew === null) return null;

  return (
    // margin:auto on the card centres it but, unlike align-items:center,
    // still lets it scroll from the top when it is taller than the screen
    <div style={{ display: "flex", height: "100%", overflowY: "auto", padding: "12px 0" }}>
      <div className="glass" style={{
        margin: "auto", flexShrink: 0,
        borderRadius: "var(--radius-lg)", padding: "40px", width: "min(360px, calc(100vw - 24px))",
        boxShadow: "0 8px 48px rgba(0,0,0,0.5), 0 0 80px rgba(30,80,200,0.12)",
      }}>
        {/* Icon + title */}
        <div style={{ display: "flex", flexDirection: "column", alignItems: "center", marginBottom: "28px", gap: "12px" }}>
          <div style={{
            width: "52px", height: "52px", borderRadius: "14px",
            background: "rgba(77,157,224,0.12)",
            border: "1px solid rgba(77,157,224,0.25)",
            display: "flex", alignItems: "center", justifyContent: "center",
            boxShadow: "0 0 24px rgba(77,157,224,0.15)",
          }}>
            {isNew
              ? <ShieldCheck size={26} color="var(--accent)" strokeWidth={1.75} />
              : <Lock size={24} color="var(--accent)" strokeWidth={1.75} />
            }
          </div>
          <div style={{ textAlign: "center" }}>
            <h1 style={{ fontSize: "19px", fontWeight: 600, marginBottom: "5px" }}>
              {isNew ? "Create your vault" : "Unlock your vault"}
            </h1>
            <p style={{ color: "var(--muted)", fontSize: "13px", lineHeight: 1.5 }}>
              {isNew
                ? "Choose a strong master password. It cannot be recovered if lost."
                : "Enter your master password to continue."}
            </p>
          </div>
        </div>

        {notice && (
          <p style={{ fontSize: "12.5px", color: "var(--danger)", lineHeight: 1.5, margin: "0 0 14px", textAlign: "center" }}>
            {notice}
          </p>
        )}
        {sync?.missing && (
          <p style={{ fontSize: "12.5px", color: "var(--danger)", lineHeight: 1.5, margin: "0 0 14px", textAlign: "center", wordBreak: "break-word" }}>
            Your sync folder isn't available: {sync.folder}. Make sure the drive is connected
            and your sync app is signed in, then restart Vault.
          </p>
        )}

        <form onSubmit={handleSubmit} style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
          <div style={{ position: "relative" }}>
            <input
              type={showPw ? "text" : "password"}
              placeholder="Master password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoFocus
              style={{ width: "100%", paddingRight: "38px" }}
            />
            <button
              type="button"
              className="btn-icon"
              onClick={() => setShowPw((s) => !s)}
              title={showPw ? "Hide password" : "Show password"}
              tabIndex={-1}
              style={{
                position: "absolute", right: "6px", top: "50%",
                transform: "translateY(-50%)", padding: "5px",
              }}
            >
              {showPw ? <EyeOff size={14} strokeWidth={2} /> : <Eye size={14} strokeWidth={2} />}
            </button>
          </div>

          {/* Strength meter — creation only, where the choice is being made */}
          {isNew && <MasterPasswordMeter password={password} />}

          {isNew && (
            <div style={{ position: "relative" }}>
              <input
                type={showPw ? "text" : "password"}
                placeholder="Confirm password"
                value={confirm}
                onChange={(e) => setConfirm(e.target.value)}
                style={{
                  width: "100%", paddingRight: "38px",
                  borderColor: confirmMismatch ? "var(--danger)" : undefined,
                }}
              />
              {confirmMatches && (
                <Check
                  size={14} strokeWidth={2.5}
                  style={{
                    position: "absolute", right: "12px", top: "50%",
                    transform: "translateY(-50%)", color: "var(--success)",
                    pointerEvents: "none",
                  }}
                />
              )}
            </div>
          )}
          {confirmMismatch && (
            <p className="error" style={{ margin: 0, fontSize: "12px" }}>Passwords don't match.</p>
          )}
          {error && <p className="error">{error}</p>}
          <button
            type="submit"
            className="btn-primary"
            disabled={loading || (isNew === true && (password.length < 8 || confirm !== password))}
            style={{ marginTop: "4px" }}
          >
            {loading ? "Please wait…" : isNew ? "Create vault" : "Unlock"}
          </button>
        </form>

        {!isNew && helloArmed && (
          <button
            type="button"
            className="btn-ghost"
            onClick={handleHello}
            disabled={helloBusy}
            style={{
              width: "100%", marginTop: "10px", padding: "9px 0",
              display: "flex", alignItems: "center", justifyContent: "center", gap: "7px",
              fontSize: "13px",
            }}
          >
            <Fingerprint size={15} strokeWidth={2} />
            {helloBusy ? "Waiting for Windows Hello…" : "Unlock with Windows Hello"}
          </button>
        )}

        {isNew && stagedCsv !== null && (
          <div style={{
            marginTop: "10px", padding: "8px 10px",
            display: "flex", alignItems: "center", gap: "8px",
            fontSize: "12.5px", color: "var(--fg-mid)",
            border: "1px solid rgba(77,157,224,0.25)", borderRadius: "var(--r-md)",
            background: "rgba(77,157,224,0.07)",
          }}>
            <FileSpreadsheet size={15} strokeWidth={2} color="var(--accent)" style={{ flexShrink: 0 }} />
            <span style={{ flex: 1 }}>
              {stagedCsv} login{stagedCsv === 1 ? "" : "s"} will be imported when you create your vault.
            </span>
            <button
              type="button"
              className="btn-icon"
              onClick={handleClearCsv}
              title="Don't import"
              aria-label="Don't import the CSV"
              style={{ padding: "3px", flexShrink: 0 }}
            >
              <X size={13} strokeWidth={2} />
            </button>
          </div>
        )}

        {isNew && (
          <>
            <button
              type="button"
              className="btn-icon"
              onClick={() => setShowMore((s) => !s)}
              aria-expanded={showMore}
              style={{
                width: "100%", marginTop: "14px", padding: "4px 0",
                display: "flex", alignItems: "center", justifyContent: "center", gap: "5px",
                fontSize: "12.5px", color: "var(--muted)",
              }}
            >
              More options
              <ChevronDown
                size={14} strokeWidth={2}
                style={{ transform: showMore ? "rotate(180deg)" : "none", transition: "transform 0.15s" }}
              />
            </button>

            {showMore && (
              <>
                <button
                  type="button"
                  className="btn-ghost"
                  onClick={handleImportBackup}
                  style={optionStyle}
                >
                  <Upload size={15} strokeWidth={2} />
                  Import a vault backup…
                </button>

                {stagedCsv === null && (
                  <button
                    type="button"
                    className="btn-ghost"
                    onClick={handleStageCsv}
                    style={optionStyle}
                  >
                    <FileSpreadsheet size={15} strokeWidth={2} />
                    Import passwords from CSV…
                  </button>
                )}

                {sync?.supported && (
                  <button
                    type="button"
                    className="btn-ghost"
                    onClick={handleOpenSynced}
                    style={optionStyle}
                  >
                    <FolderSync size={15} strokeWidth={2} />
                    Open a vault from a sync folder…
                  </button>
                )}
              </>
            )}
          </>
        )}
      </div>
    </div>
  );
}

const optionStyle: React.CSSProperties = {
  width: "100%", marginTop: "8px", padding: "9px 0",
  display: "flex", alignItems: "center", justifyContent: "center", gap: "7px",
  fontSize: "13px",
};
