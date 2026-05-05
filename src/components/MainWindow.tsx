import { useEffect, useState } from "react";
import { listEntries, lock } from "../api";
import { Entry } from "../types";
import EntryList from "./EntryList";
import EntryDetail from "./EntryDetail";
import AddEntryModal from "./AddEntryModal";

const TIMEOUT_OPTIONS = [
  { label: "Lock after 1 min",  ms: 1 * 60 * 1000 },
  { label: "Lock after 5 min",  ms: 5 * 60 * 1000 },
  { label: "Lock after 15 min", ms: 15 * 60 * 1000 },
  { label: "Lock after 30 min", ms: 30 * 60 * 1000 },
  { label: "Lock after 1 hour", ms: 60 * 60 * 1000 },
  { label: "No auto-lock",      ms: 0 },
];

interface Props {
  onLocked: () => void;
  timeoutMs: number;
  onTimeoutChange: (ms: number) => void;
}

export default function MainWindow({ onLocked, timeoutMs, onTimeoutChange }: Props) {
  const [entries, setEntries] = useState<Entry[]>([]);
  const [selected, setSelected] = useState<Entry | null>(null);
  const [search, setSearch] = useState("");
  const [showAdd, setShowAdd] = useState(false);

  useEffect(() => { listEntries().then(setEntries); }, []);

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

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100vh" }}>
      {/* Titlebar */}
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        padding: "0 14px", height: "46px",
        borderBottom: "1px solid var(--border)", background: "var(--bg2)", flexShrink: 0,
      }}>
        {/* Logo + name */}
        <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
          <div style={{
            width: "26px", height: "26px", borderRadius: "7px",
            background: "var(--accent-dim)", border: "1px solid rgba(124,106,247,0.25)",
            display: "flex", alignItems: "center", justifyContent: "center",
          }}>
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="var(--accent)"
              strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <rect x="3" y="11" width="18" height="11" rx="2"/>
              <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
            </svg>
          </div>
          <span style={{ fontWeight: 600, fontSize: "14px", letterSpacing: "-0.01em" }}>Vault</span>
        </div>

        {/* Actions */}
        <div style={{ display: "flex", gap: "8px", alignItems: "center" }}>
          <select
            value={timeoutMs}
            onChange={(e) => onTimeoutChange(Number(e.target.value))}
            title="Auto-lock timeout"
          >
            {TIMEOUT_OPTIONS.map((o) => (
              <option key={o.ms} value={o.ms}>{o.label}</option>
            ))}
          </select>
          <button className="btn-primary" style={{ padding: "6px 13px", fontSize: "13px" }} onClick={() => setShowAdd(true)}>
            + Add
          </button>
          <button className="btn-ghost" style={{ padding: "6px 13px", fontSize: "13px" }} onClick={handleLock}>
            Lock
          </button>
        </div>
      </div>

      {/* Body */}
      <div style={{ display: "flex", flex: 1, overflow: "hidden" }}>
        <EntryList
          entries={entries}
          selectedId={selected?.id ?? null}
          onSelect={setSelected}
          search={search}
          onSearchChange={setSearch}
        />
        <div style={{ flex: 1, overflow: "hidden", background: "var(--bg)" }}>
          {selected
            ? <EntryDetail entry={selected} onUpdated={handleUpdated} onDeleted={handleDeleted} />
            : (
              <div style={{
                display: "flex", flexDirection: "column", alignItems: "center",
                justifyContent: "center", height: "100%", gap: "10px",
                color: "var(--muted)", userSelect: "none",
              }}>
                <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                  strokeWidth="1.25" strokeLinecap="round" strokeLinejoin="round" style={{ opacity: 0.4 }}>
                  <rect x="3" y="11" width="18" height="11" rx="2"/>
                  <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
                </svg>
                <span style={{ fontSize: "13px" }}>
                  {entries.length === 0 ? "Add an entry to get started" : "Select an entry to view it"}
                </span>
              </div>
            )
          }
        </div>
      </div>

      {showAdd && <AddEntryModal onAdded={handleAdded} onClose={() => setShowAdd(false)} />}
    </div>
  );
}
