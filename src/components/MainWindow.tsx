import { useEffect, useState } from "react";
import { listEntries, listFolders, lock, isAutostartEnabled, enableAutostart, disableAutostart, exportVault, importVault, addFolder, renameFolder, deleteFolder } from "../api";
import { Entry, Folder } from "../types";
import EntryList from "./EntryList";
import EntryDetail from "./EntryDetail";
import AddEntryModal from "./AddEntryModal";

const TIMEOUT_OPTIONS = [
  { label: "1 min",  ms: 1 * 60 * 1000 },
  { label: "5 min",  ms: 5 * 60 * 1000 },
  { label: "15 min", ms: 15 * 60 * 1000 },
  { label: "30 min", ms: 30 * 60 * 1000 },
  { label: "1 hour", ms: 60 * 60 * 1000 },
  { label: "Never",  ms: 0 },
];

interface Props {
  onLocked: () => void;
  timeoutMs: number;
  onTimeoutChange: (ms: number) => void;
}

export default function MainWindow({ onLocked, timeoutMs, onTimeoutChange }: Props) {
  const [entries, setEntries] = useState<Entry[]>([]);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [selected, setSelected] = useState<Entry | null>(null);
  const [search, setSearch] = useState("");
  const [activeFolder, setActiveFolder] = useState<string | null>(null);
  const [showAdd, setShowAdd] = useState(false);
  const [autostart, setAutostart] = useState(false);

  useEffect(() => {
    listEntries().then(setEntries);
    listFolders().then(setFolders);
    isAutostartEnabled().then(setAutostart).catch(() => {});
  }, []);

  const handleAutostartToggle = async () => {
    try {
      if (autostart) {
        await disableAutostart();
        setAutostart(false);
      } else {
        await enableAutostart();
        setAutostart(true);
      }
    } catch (e) {
      console.error("Autostart toggle failed:", e);
    }
  };

  const handleLock = async () => {
    await lock();
    onLocked();
  };

  const handleExport = async () => {
    try {
      await exportVault();
    } catch (e) {
      console.error("Export failed:", e);
    }
  };

  const handleImport = async () => {
    if (!window.confirm("Importing a vault will replace your current vault and lock the app. Continue?")) return;
    try {
      await importVault();
      onLocked();
    } catch (e) {
      console.error("Import failed:", e);
      window.alert(`Import failed: ${e}`);
    }
  };

  const handleAdded = (entry: Entry) => {
    setEntries((prev) => [...prev, entry]);
    setSelected(entry);
    setShowAdd(false);
  };

  const handleUpdated = (updated: Entry) => {
    setEntries((prev) => prev.map((e) => (e.id === updated.id ? updated : e)));
    setSelected(updated);
  };

  const handleDeleted = (id: string) => {
    setEntries((prev) => prev.filter((e) => e.id !== id));
    setSelected(null);
  };

  const handleFolderAdded = async (name: string) => {
    const folder = await addFolder(name);
    setFolders((prev) => [...prev, folder]);
    return folder;
  };

  const handleFolderRenamed = async (id: string, name: string) => {
    await renameFolder(id, name);
    setFolders((prev) => prev.map((f) => (f.id === id ? { ...f, name } : f)));
  };

  const handleFolderDeleted = async (id: string) => {
    await deleteFolder(id);
    setFolders((prev) => prev.filter((f) => f.id !== id));
    // Unassign entries that were in this folder
    setEntries((prev) => prev.map((e) => e.folder_id === id ? { ...e, folder_id: undefined } : e));
    if (activeFolder === id) setActiveFolder(null);
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100vh" }}>
      {/* Titlebar */}
      <div className="glass" style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        padding: "0 16px", height: "48px",
        borderLeft: "none", borderRight: "none", borderTop: "none",
        flexShrink: 0,
      }}>
        <span style={{ fontWeight: 600, fontSize: "15px" }}>Vault</span>
        <div style={{ display: "flex", gap: "8px", alignItems: "center" }}>
          <select
            value={timeoutMs}
            onChange={(e) => onTimeoutChange(Number(e.target.value))}
            style={{
              background: "rgba(255,255,255,0.06)", color: "var(--muted)",
              border: "1px solid var(--border)",
              borderRadius: "var(--radius)", padding: "5px 8px", fontSize: "12px", cursor: "pointer",
            }}
            title="Auto-lock after"
          >
            {TIMEOUT_OPTIONS.map((o) => (
              <option key={o.ms} value={o.ms}>{o.label === "Never" ? "No auto-lock" : `Lock after ${o.label}`}</option>
            ))}
          </select>
          <button className="btn-primary" style={{ padding: "6px 14px", fontSize: "13px" }} onClick={() => setShowAdd(true)}>
            + Add entry
          </button>
          <button
            className="btn-ghost"
            style={{ padding: "6px 14px", fontSize: "13px" }}
            onClick={handleAutostartToggle}
            title="Launch Vault on system startup"
          >
            {autostart ? "Autostart: On" : "Autostart: Off"}
          </button>
          <button className="btn-ghost" style={{ padding: "6px 14px", fontSize: "13px" }} onClick={handleExport} title="Save an encrypted backup of your vault">
            Export
          </button>
          <button className="btn-ghost" style={{ padding: "6px 14px", fontSize: "13px" }} onClick={handleImport} title="Restore vault from an encrypted backup">
            Import
          </button>
          <button className="btn-ghost" style={{ padding: "6px 14px", fontSize: "13px" }} onClick={handleLock}>
            Lock
          </button>
        </div>
      </div>

      {/* Body */}
      <div style={{ display: "flex", flex: 1, overflow: "hidden" }}>
        <EntryList
          entries={entries}
          folders={folders}
          activeFolder={activeFolder}
          onFolderChange={setActiveFolder}
          onFolderAdded={handleFolderAdded}
          onFolderRenamed={handleFolderRenamed}
          onFolderDeleted={handleFolderDeleted}
          selectedId={selected?.id ?? null}
          onSelect={setSelected}
          search={search}
          onSearchChange={setSearch}
        />
        <div style={{ flex: 1, overflow: "hidden", display: "flex", flexDirection: "column" }}>
          {selected
            ? <EntryDetail entry={selected} folders={folders} onUpdated={handleUpdated} onDeleted={handleDeleted} />
            : (
              <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100%", color: "var(--muted)" }}>
                {entries.length === 0 ? "No entries yet — add one to get started." : "Select an entry to view it."}
              </div>
            )
          }
        </div>
      </div>

      {showAdd && (
        <AddEntryModal
          folders={folders}
          defaultFolderId={activeFolder ?? undefined}
          onAdded={handleAdded}
          onClose={() => setShowAdd(false)}
        />
      )}
    </div>
  );
}
