import { Entry } from "../types";

interface Props {
  entries: Entry[];
  selectedId: string | null;
  onSelect: (entry: Entry) => void;
  search: string;
  onSearchChange: (v: string) => void;
}

const AVATAR_COLORS = ["#7c6af7", "#e05593", "#05b8cc", "#e8a825", "#4caf82", "#e07a55", "#5b9cf7"];

function avatarColor(name: string) {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) & 0xffffffff;
  return AVATAR_COLORS[Math.abs(h) % AVATAR_COLORS.length];
}

export default function EntryList({ entries, selectedId, onSelect, search, onSearchChange }: Props) {
  const filtered = entries.filter((e) =>
    e.name.toLowerCase().includes(search.toLowerCase()) ||
    e.email.toLowerCase().includes(search.toLowerCase())
  );

  return (
    <div style={{
      width: "256px", flexShrink: 0, borderRight: "1px solid var(--border)",
      display: "flex", flexDirection: "column", height: "100%", background: "var(--bg2)",
    }}>
      {/* Search */}
      <div style={{ padding: "10px", borderBottom: "1px solid var(--border)" }}>
        <div style={{ position: "relative" }}>
          <svg
            width="14" height="14" viewBox="0 0 24 24" fill="none"
            stroke="var(--muted)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"
            style={{ position: "absolute", left: "10px", top: "50%", transform: "translateY(-50%)", pointerEvents: "none" }}
          >
            <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
          </svg>
          <input
            placeholder="Search…"
            value={search}
            onChange={(e) => onSearchChange(e.target.value)}
            style={{ paddingLeft: "30px" }}
          />
        </div>
      </div>

      {/* List */}
      <div style={{ overflowY: "auto", flex: 1 }}>
        {filtered.length === 0 && (
          <p style={{ padding: "20px 16px", color: "var(--muted)", fontSize: "13px" }}>
            {search ? `No results for "${search}"` : "No entries yet."}
          </p>
        )}
        {filtered.map((entry) => {
          const selected = selectedId === entry.id;
          const color = avatarColor(entry.name);
          return (
            <div
              key={entry.id}
              onClick={() => onSelect(entry)}
              style={{
                display: "flex", alignItems: "center", gap: "10px",
                padding: "10px 12px", cursor: "pointer",
                borderBottom: "1px solid var(--border)",
                background: selected ? "var(--bg3)" : "transparent",
                borderLeft: `2px solid ${selected ? "var(--accent)" : "transparent"}`,
                transition: "background 0.12s, border-color 0.12s",
              }}
              onMouseEnter={(e) => { if (!selected) (e.currentTarget as HTMLDivElement).style.background = "var(--bg3)"; }}
              onMouseLeave={(e) => { if (!selected) (e.currentTarget as HTMLDivElement).style.background = "transparent"; }}
            >
              {/* Avatar */}
              <div style={{
                width: "32px", height: "32px", borderRadius: "8px", flexShrink: 0,
                background: `${color}22`, border: `1px solid ${color}44`,
                display: "flex", alignItems: "center", justifyContent: "center",
                fontSize: "13px", fontWeight: 600, color,
              }}>
                {entry.name.charAt(0).toUpperCase()}
              </div>
              <div style={{ minWidth: 0 }}>
                <div style={{
                  fontWeight: 500, fontSize: "13px", whiteSpace: "nowrap",
                  overflow: "hidden", textOverflow: "ellipsis",
                }}>
                  {entry.name}
                </div>
                <div style={{
                  color: "var(--muted)", fontSize: "12px", whiteSpace: "nowrap",
                  overflow: "hidden", textOverflow: "ellipsis",
                }}>
                  {entry.username || entry.email}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
