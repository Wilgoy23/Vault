import { Entry } from "../types";

interface Props {
  entries: Entry[];
  selectedId: string | null;
  onSelect: (entry: Entry) => void;
  search: string;
  onSearchChange: (v: string) => void;
}

export default function EntryList({ entries, selectedId, onSelect, search, onSearchChange }: Props) {
  const filtered = entries.filter((e) =>
    e.name.toLowerCase().includes(search.toLowerCase()) ||
    e.email.toLowerCase().includes(search.toLowerCase())
  );

  return (
    <div style={{
      width: "240px", flexShrink: 0, borderRight: "1px solid var(--border)",
      display: "flex", flexDirection: "column", height: "100%",
    }}>
      <div style={{ padding: "12px", borderBottom: "1px solid var(--border)" }}>
        <input
          placeholder="Search…"
          value={search}
          onChange={(e) => onSearchChange(e.target.value)}
        />
      </div>
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
              background: selectedId === entry.id ? "var(--bg3)" : "transparent",
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