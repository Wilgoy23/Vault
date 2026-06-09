import { useRef, useState, RefObject } from "react";
import { Folder, FolderOpen, Plus } from "lucide-react";
import { Entry, Folder as FolderType } from "../types";

const AV_CLASSES = [
  "av-blue", "av-purple", "av-green", "av-red", "av-orange",
  "av-cyan", "av-pink", "av-yellow", "av-teal", "av-indigo",
];

function avatarClass(name: string): string {
  const code = (name.charCodeAt(0) || 0) + (name.charCodeAt(1) || 0);
  return AV_CLASSES[code % AV_CLASSES.length];
}

interface Props {
  entries: Entry[];
  folders: FolderType[];
  activeFolder: string | null;
  onFolderChange: (id: string | null) => void;
  onFolderAdded: (name: string) => Promise<FolderType>;
  onFolderRenamed: (id: string, name: string) => Promise<void>;
  onFolderDeleted: (id: string) => Promise<void>;
  selectedId: string | null;
  onSelect: (entry: Entry) => void;
  search: string;
  onSearchChange: (v: string) => void;
  searchInputRef?: RefObject<HTMLInputElement | null>;
}

export default function EntryList({
  entries, folders, activeFolder, onFolderChange,
  onFolderAdded, onFolderRenamed, onFolderDeleted,
  selectedId, onSelect, search, onSearchChange, searchInputRef,
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
    const matchesFolder = activeFolder === null ? true : e.folder_id === activeFolder;
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

  const startRename = (folder: FolderType, e: React.MouseEvent) => {
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

  const handleDeleteFolder = async (folder: FolderType, e: React.MouseEvent) => {
    e.stopPropagation();
    if (!window.confirm(`Delete folder "${folder.name}"? Entries will be moved to All.`)) return;
    await onFolderDeleted(folder.id);
  };

  return (
    <div style={{
      width: "240px", flexShrink: 0,
      display: "flex", flexDirection: "column", height: "100%",
      borderRight: "1px solid var(--border-dim)",
      background: "rgba(4,11,22,0.55)",
      backdropFilter: "blur(8px)",
      WebkitBackdropFilter: "blur(8px)",
    }}>
      {/* Folder tabs */}
      <div style={{
        display: "flex", alignItems: "flex-end",
        padding: "10px 10px 0", gap: "0",
        flexShrink: 0,
      }}>
        <TabBtn
          label="All"
          active={activeFolder === null}
          onClick={() => onFolderChange(null)}
        />
        {folders.map((folder) => {
          const isActive = activeFolder === folder.id;
          return (
            <div
              key={folder.id}
              onClick={() => onFolderChange(folder.id)}
              onDoubleClick={(e) => startRename(folder, e)}
              onMouseEnter={() => setHoveredTab(folder.id)}
              onMouseLeave={() => setHoveredTab(null)}
              style={{
                display: "flex", alignItems: "center", gap: "4px",
                fontSize: "11.5px", fontWeight: 500,
                color: isActive ? "var(--accent)" : "var(--muted-dim)",
                padding: "5px 9px 7px",
                cursor: "pointer",
                borderBottom: isActive ? "2px solid var(--accent)" : "2px solid transparent",
                borderRadius: "5px 5px 0 0",
                background: isActive ? "rgba(77,157,224,0.07)" : "transparent",
                letterSpacing: "0.01em",
                whiteSpace: "nowrap",
                transition: "color 0.12s, border-color 0.12s, background 0.12s",
                userSelect: "none",
              }}
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
                  style={{ width: "72px", fontSize: "11.5px", padding: "1px 4px", background: "rgba(255,255,255,0.08)", border: "1px solid var(--accent)", borderRadius: "3px", color: "inherit" }}
                />
              ) : (
                <>
                  {isActive
                    ? <FolderOpen size={10} strokeWidth={2} style={{ flexShrink: 0 }} />
                    : <Folder size={10} strokeWidth={2} style={{ flexShrink: 0 }} />
                  }
                  <span style={{ maxWidth: "64px", overflow: "hidden", textOverflow: "ellipsis" }}>
                    {folder.name}
                  </span>
                  {(hoveredTab === folder.id || isActive) && (
                    <span
                      onClick={(e) => handleDeleteFolder(folder, e)}
                      style={{ fontSize: "11px", lineHeight: 1, opacity: 0.5, padding: "1px 2px", cursor: "pointer" }}
                      title="Delete folder"
                    >×</span>
                  )}
                </>
              )}
            </div>
          );
        })}

        {newFolderMode && (
          <div style={{ display: "flex", alignItems: "center", padding: "4px 8px 6px", background: "rgba(255,255,255,0.04)", border: "1px solid var(--border-dim)", borderBottom: "none", borderRadius: "5px 5px 0 0" }}>
            <input
              ref={newInputRef}
              value={newFolderName}
              onChange={(e) => setNewFolderName(e.target.value)}
              onBlur={commitNewFolder}
              onKeyDown={(e) => {
                if (e.key === "Enter") commitNewFolder();
                if (e.key === "Escape") setNewFolderMode(false);
              }}
              placeholder="Name…"
              style={{ width: "72px", fontSize: "11.5px", padding: "1px 4px", background: "transparent", border: "1px solid var(--accent)", borderRadius: "3px", color: "var(--text)" }}
            />
          </div>
        )}

        {!newFolderMode && (
          <div
            onClick={startNewFolder}
            title="New folder"
            style={{
              display: "flex", alignItems: "center", justifyContent: "center",
              padding: "4px 7px 7px",
              color: "var(--muted-dim)", cursor: "pointer",
              borderBottom: "2px solid transparent",
              borderRadius: "5px 5px 0 0",
              transition: "color 0.12s",
              userSelect: "none",
            }}
          >
            <Plus size={11} strokeWidth={2.5} />
          </div>
        )}
      </div>

      {/* Tab rule */}
      <div style={{ height: "1px", margin: "0 10px", background: "var(--border-dim)", flexShrink: 0 }} />

      {/* Search */}
      <div style={{ padding: "9px 10px 5px", flexShrink: 0 }}>
        <div className="search-box">
          <svg className="search-icon" width="13" height="13" viewBox="0 0 13 13" fill="none">
            <circle cx="5.5" cy="5.5" r="3.75" stroke="currentColor" strokeWidth="1.3"/>
            <path d="M8.75 8.75L11.25 11.25" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/>
          </svg>
          <input
            ref={searchInputRef}
            placeholder="Search entries…"
            value={search}
            onChange={(e) => onSearchChange(e.target.value)}
          />
        </div>
      </div>

      {/* Entry list */}
      <div style={{ overflowY: "auto", flex: 1, padding: "4px 7px 8px", minHeight: 0 }}>
        {filtered.length === 0 && (
          <p style={{ padding: "16px 8px", color: "var(--muted-dim)", fontSize: "12.5px" }}>
            No entries found.
          </p>
        )}
        {filtered.map((entry) => {
          const isSelected = selectedId === entry.id;
          const avClass = avatarClass(entry.name);
          return (
            <div
              key={entry.id}
              onClick={() => onSelect(entry)}
              style={{
                display: "flex", alignItems: "center", gap: "10px",
                padding: "7px 8px 7px 9px",
                borderRadius: "var(--r-md)",
                borderLeft: isSelected ? "2px solid var(--accent)" : "2px solid transparent",
                background: isSelected ? "var(--accent-tint)" : "transparent",
                cursor: "pointer",
                transition: "background 0.11s, border-color 0.11s",
                marginBottom: "1px",
                userSelect: "none",
              }}
              onMouseEnter={(e) => { if (!isSelected) (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.035)"; }}
              onMouseLeave={(e) => { if (!isSelected) (e.currentTarget as HTMLElement).style.background = "transparent"; }}
            >
              <div className={`avatar ${avClass}`}>
                {entry.name.charAt(0).toUpperCase()}
              </div>
              <div style={{ minWidth: 0, flex: 1 }}>
                <div style={{
                  fontWeight: 500, fontSize: "13px",
                  color: isSelected ? "var(--fg-mid)" : "#A8C2DA",
                  letterSpacing: "-0.008em",
                  whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
                  lineHeight: 1.2,
                }}>
                  {entry.name}
                </div>
                <div style={{
                  color: "var(--muted-dim)", fontSize: "11px",
                  whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
                  marginTop: "2px", lineHeight: 1.2,
                }}>
                  {entry.email}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function TabBtn({ label, active, onClick }: { label: string; active: boolean; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      style={{
        background: active ? "rgba(77,157,224,0.07)" : "transparent",
        border: "none",
        borderBottom: active ? "2px solid var(--accent)" : "2px solid transparent",
        borderRadius: "5px 5px 0 0",
        color: active ? "var(--accent)" : "var(--muted-dim)",
        cursor: "pointer",
        fontSize: "11.5px",
        fontWeight: 500,
        letterSpacing: "0.01em",
        padding: "5px 9px 7px",
        transition: "color 0.12s, border-color 0.12s, background 0.12s",
        whiteSpace: "nowrap",
        userSelect: "none",
      }}
    >
      {label}
    </button>
  );
}
