import { useEffect, useState } from "react";
import { listEntries, lock } from "../api";
import { Entry } from "../types";
import EntryList from "./EntryList";
import EntryDetail from "./EntryDetail";
import AddEntryModal from "./AddEntryModal";

interface Props {
  onLocked: () => void;
}

export default function MainWindow({ onLocked }: Props) {
  const [entries, setEntries] = useState<Entry[]>([]);
  const [selected, setSelected] = useState<Entry | null>(null);
  const [search, setSearch] = useState("");
  const [showAdd, setShowAdd] = useState(false);

  useEffect(() => {
    listEntries().then(setEntries);
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

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100vh" }}>
      {/* Titlebar */}
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        padding: "0 16px", height: "48px",
        borderBottom: "1px solid var(--border)", background: "var(--bg2)",
        flexShrink: 0,
      }}>
        <span style={{ fontWeight: 600, fontSize: "15px" }}>Vault</span>
        <div style={{ display: "flex", gap: "8px" }}>
          <button className="btn-primary" style={{ padding: "6px 14px", fontSize: "13px" }} onClick={() => setShowAdd(true)}>
            + Add entry
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
          selectedId={selected?.id ?? null}
          onSelect={setSelected}
          search={search}
          onSearchChange={setSearch}
        />
        <div style={{ flex: 1, overflow: "hidden" }}>
          {selected
            ? <EntryDetail entry={selected} onUpdated={handleUpdated} onDeleted={handleDeleted} />
            : (
              <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100%", color: "var(--muted)" }}>
                {entries.length === 0 ? "No entries yet — add one to get started." : "Select an entry to view it."}
              </div>
            )
          }
        </div>
      </div>

      {showAdd && <AddEntryModal onAdded={handleAdded} onClose={() => setShowAdd(false)} />}
    </div>
  );
}