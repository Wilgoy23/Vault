import { useEffect, useState } from "react";
import { Plus, Lock, Settings } from "lucide-react";
import { listEntries, listFolders, lock, isAutostartEnabled, addFolder, renameFolder, deleteFolder } from "../api";
import { Entry, Folder } from "../types";
import EntryList from "./EntryList";
import EntryDetail from "./EntryDetail";
import AddEntryModal from "./AddEntryModal";
import SettingsModal from "./SettingsModal";

interface Props {
  onLocked: () => void;
  timeoutMs: number;
  onTimeoutChange: (ms: number) => void;
  themeId: string;
  onThemeChange: (id: string) => void;
}

export default function MainWindow({ onLocked, timeoutMs, onTimeoutChange, themeId, onThemeChange }: Props) {
  const [entries, setEntries] = useState<Entry[]>([]);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [selected, setSelected] = useState<Entry | null>(null);
  const [search, setSearch] = useState("");
  const [activeFolder, setActiveFolder] = useState<string | null>(null);
  const [showAdd, setShowAdd] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [autostart, setAutostart] = useState(false);

  useEffect(() => {
    listEntries().then(setEntries);
    listFolders().then(setFolders);
    isAutostartEnabled().then(setAutostart).catch(() => {});
  }, []);

  const handleLock = async () => { await lock(); onLocked(); };

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
    setEntries((prev) => prev.map((e) => e.folder_id === id ? { ...e, folder_id: undefined } : e));
    if (activeFolder === id) setActiveFolder(null);
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100vh" }}>
      {/* Titlebar */}
      <div className="glass" style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        padding: "0 14px", height: "48px",
        borderLeft: "none", borderRight: "none", borderTop: "none",
        flexShrink: 0,
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
          <div className="wordmark-logo">
            <svg width="13" height="14" viewBox="0 0 13 14" fill="none">
              <path d="M6.5 1.2L1.5 3.2V7C1.5 9.8 3.7 12.2 6.5 12.8C9.3 12.2 11.5 9.8 11.5 7V3.2L6.5 1.2Z" fill="white" fillOpacity="0.95"/>
              <path d="M4.5 7L6 8.5L9 5.5" stroke="#1A5FA8" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/>
            </svg>
          </div>
          <span style={{ fontWeight: 700, fontSize: "14px", letterSpacing: "-0.025em", color: "var(--text)" }}>
            Vault
          </span>
        </div>
        <div style={{ display: "flex", gap: "6px", alignItems: "center" }}>
          <button
            className="btn-primary"
            style={{ padding: "6px 12px", fontSize: "13px", display: "flex", alignItems: "center", gap: "5px" }}
            onClick={() => setShowAdd(true)}
          >
            <Plus size={14} strokeWidth={2.5} />
            Add entry
          </button>
          <button
            className="btn-icon"
            onClick={handleLock}
            title="Lock vault"
          >
            <Lock size={15} strokeWidth={2} />
          </button>
          <button
            className="btn-icon"
            onClick={() => setShowSettings(true)}
            title="Settings"
          >
            <Settings size={15} strokeWidth={2} />
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
              <div style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", height: "100%", gap: "10px" }}>
                <div style={{ color: "var(--muted)", opacity: 0.25, fontSize: "48px" }}>🔐</div>
                <p style={{ color: "var(--muted)", fontSize: "13px" }}>
                  {entries.length === 0 ? "No entries yet — add one to get started." : "Select an entry to view it."}
                </p>
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

      {showSettings && (
        <SettingsModal
          themeId={themeId}
          onThemeChange={onThemeChange}
          timeoutMs={timeoutMs}
          onTimeoutChange={onTimeoutChange}
          autostart={autostart}
          onAutostartChange={setAutostart}
          onImported={() => { setShowSettings(false); onLocked(); }}
          onClose={() => setShowSettings(false)}
        />
      )}
    </div>
  );
}
