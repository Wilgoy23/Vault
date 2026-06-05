import { useEffect, useState } from "react";
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

  const handleLock = async () => {
    await lock();
    onLocked();
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
          <button
            className="btn-primary"
            style={{ padding: "6px 14px", fontSize: "13px" }}
            onClick={() => setShowAdd(true)}
          >
            + Add entry
          </button>
          <button
            className="btn-ghost"
            style={{ padding: "6px 14px", fontSize: "13px" }}
            onClick={handleLock}
          >
            Lock
          </button>
          <button
            className="btn-ghost"
            style={{ padding: "6px 10px", fontSize: "16px", lineHeight: 1 }}
            onClick={() => setShowSettings(true)}
            title="Settings"
          >
            ⚙
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
