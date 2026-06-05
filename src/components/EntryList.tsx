import { useRef, useState } from "react";
import { Entry, Folder } from "../types";

interface Props {
  entries: Entry[];
  folders: Folder[];
  activeFolder: string | null;
  onFolderChange: (id: string | null) => void;
  onFolderAdded: (name: string) => Promise<Folder>;
  onFolderRenamed: (id: string, name: string) => Promise<void>;
  onFolderDeleted: (id: string) => Promise<void>;
  selectedId: string | null;
  onSelect: (entry: Entry) => void;
  search: string;
  onSearchChange: (v: string) => void;
}

export default function EntryList({
  entries, folders, activeFolder, onFolderChange,
  onFolderAdded, onFolderRenamed, onFolderDeleted,
  selectedId, onSelect, search, onSearchChange,
}: Props) {
  const [newFolderMode, setNewFolderMode] = useState(false);
  const [newFolderName, setNewFolderName] = useState("");
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [hoveredTab, setHoveredTab] = useState<string | null>(null);
  const newInputRef = useRef<HTMLInputElement>(null);
  const renameInputRef = useRef<HTMLInputElement>(null);

  const filtered = entries.filter((e) => {
    const matchesSearch =
      e.name.toLowerCase().includes(search.toLowerCase()) ||
      e.email.toLowerCase().includes(search.toLowerCase());
    const matchesFolder =
      activeFolder === null ? true : e.folder_id === activeFolder;
    return matchesSearch && matchesFolder;
  });

  const startNewFolder = () => {
    setNewFolderMode(true);
    setNewFolderName("");
    setTimeout(() => newInputRef.current?.focus(), 0);
  };

  const commitNewFolder = async () => {
    const name = newFolderName.trim();
    if (name) {
      const folder = await onFolderAdded(name);
      onFolderChange(folder.id);
    }
    setNewFolderMode(false);
    setNewFolderName("");
  };

  const startRename = (folder: Folder, e: React.MouseEvent) => {
    e.stopPropagation();
    setRenamingId(folder.id);
    setRenameValue(folder.name);
    setTimeout(() => renameInputRef.current?.focus(), 0);
  };

  const commitRename = async () => {
    if (!renamingId) return;
    const name = renameValue.trim();
    if (name) await onFolderRenamed(renamingId, name);
    setRenamingId(null);
  };

  const handleDeleteFolder = async (folder: Folder, e: React.MouseEvent) => {
    e.stopPropagation();
    if (!window.confirm(`Delete folder "${folder.name}"? Entries will be moved to All.`)) return;
    await onFolderDeleted(folder.id);
  };

  const tabBase: React.CSSProperties = {
    position: "relative",
    display: "flex", alignItems: "center", gap: "4px",
    padding: "5px 10px",
    fontSize: "12px", fontWeight: 500,
    cursor: "pointer", whiteSpace: "nowrap", flexShrink: 0,
    borderRadius: "6px 6px 0 0",
    border: "1px solid transparent",
    borderBottom: "none",
    transition: "background 0.1s, color 0.1s",
    userSelect: "none",
  };

  const tabActive: React.CSSProperties = {
    background: "rgba(77,157,224,0.12)",
    borderColor: "var(--border)",
    color: "var(--accent)",
  };

  const tabInactive: React.CSSProperties = {
    background: "transparent",
    color: "var(--muted)",
  };

  return (
    <div className="glass" style={{
      width: "240px", flexShrink: 0,
      borderTop: "none", borderBottom: "none", borderLeft: "none",
      display: "flex", flexDirection: "column", height: "100%",
    }}>
      {/* Folder tab bar */}
      <div style={{
        display: "flex", alignItems: "flex-end",
        padding: "8px 8px 0",
        gap: "2px",
        overflowX: "auto",
        scrollbarWidth: "none",
        borderBottom: "1px solid var(--border)",
        flexShrink: 0,
      }}>
        {/* All tab */}
        <div
          style={{ ...tabBase, ...(activeFolder === null ? tabActive : tabInactive) }}
          onClick={() => onFolderChange(null)}
          onMouseEnter={() => setHoveredTab("all")}
          onMouseLeave={() => setHoveredTab(null)}
        >
          All
        </div>

        {/* Folder tabs */}
        {folders.map((folder) => (
          <div
            key={folder.id}
            style={{ ...tabBase, ...(activeFolder === folder.id ? tabActive : tabInactive) }}
            onClick={() => onFolderChange(folder.id)}
            onDoubleClick={(e) => startRename(folder, e)}
            onMouseEnter={() => setHoveredTab(folder.id)}
            onMouseLeave={() => setHoveredTab(null)}
          >
            {renamingId === folder.id ? (
              <input
                ref={renameInputRef}
                value={renameValue}
                onChange={(e) => setRenameValue(e.target.value)}
                onBlur={commitRename}
                onKeyDown={(e) => {
                  if (e.key === "Enter") commitRename();
                  if (e.key === "Escape") setRenamingId(null);
                  e.stopPropagation();
                }}
                onClick={(e) => e.stopPropagation()}
                style={{
                  width: "72px", fontSize: "12px", padding: "1px 4px",
                  background: "rgba(255,255,255,0.08)",
                  border: "1px solid var(--accent)",
                  borderRadius: "3px", color: "inherit",
                }}
              />
            ) : (
              <>
                <span style={{ maxWidth: "80px", overflow: "hidden", textOverflow: "ellipsis" }}>
                  {folder.name}
                </span>
                {(hoveredTab === folder.id || activeFolder === folder.id) && (
                  <span
                    onClick={(e) => handleDeleteFolder(folder, e)}
                    style={{
                      fontSize: "11px", lineHeight: 1, opacity: 0.6,
                      padding: "1px 2px", borderRadius: "3px",
                      cursor: "pointer",
                    }}
                    title="Delete folder"
                  >
                    ×
                  </span>
                )}
              </>
            )}
          </div>
        ))}

        {/* New folder input */}
        {newFolderMode && (
          <div style={{ ...tabBase, background: "rgba(255,255,255,0.04)", border: "1px solid var(--border)", borderBottom: "none" }}>
            <input
              ref={newInputRef}
              value={newFolderName}
              onChange={(e) => setNewFolderName(e.target.value)}
              onBlur={commitNewFolder}
              onKeyDown={(e) => {
                if (e.key === "Enter") commitNewFolder();
                if (e.key === "Escape") { setNewFolderMode(false); }
              }}
              placeholder="Name…"
              style={{
                width: "72px", fontSize: "12px", padding: "1px 4px",
                background: "transparent",
                border: "1px solid var(--accent)",
                borderRadius: "3px", color: "var(--text)",
              }}
            />
          </div>
        )}

        {/* Add folder button */}
        {!newFolderMode && (
          <div
            style={{ ...tabBase, ...tabInactive, color: "var(--muted)", fontSize: "16px", padding: "3px 8px" }}
            onClick={startNewFolder}
            title="New folder"
          >
            +
          </div>
        )}
      </div>

      {/* Search */}
      <div style={{ padding: "12px", borderBottom: "1px solid var(--border)" }}>
        <input
          placeholder="Search…"
          value={search}
          onChange={(e) => onSearchChange(e.target.value)}
        />
      </div>

      {/* Entry list */}
      <div style={{ overflowY: "auto", flex: 1 }}>
        {filtered.length === 0 && (
          <p style={{ padding: "20px", color: "var(--muted)", fontSize: "13px" }}>
            No entries found.
          </p>
        )}
        {filtered.map((entry) => (
          <div
            key={entry.id}
            onClick={() => onSelect(entry)}
            style={{
              padding: "12px 16px", cursor: "pointer",
              borderBottom: "1px solid var(--border)",
              background: selectedId === entry.id ? "rgba(77,157,224,0.12)" : "transparent",
              borderLeft: selectedId === entry.id ? "2px solid var(--accent)" : "2px solid transparent",
              transition: "background 0.1s",
            }}
          >
            <div style={{ fontWeight: 500, marginBottom: "2px" }}>{entry.name}</div>
            <div style={{ color: "var(--muted)", fontSize: "12px" }}>{entry.email}</div>
          </div>
        ))}
      </div>
    </div>
  );
}
