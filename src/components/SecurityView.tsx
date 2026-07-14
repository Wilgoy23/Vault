import { useMemo } from "react";
import { KeyRound, ShieldAlert, ShieldCheck, History, Copy } from "lucide-react";
import { Entry } from "../types";
import { passwordStrength, STRENGTH_LABELS } from "../utils/password";

const AV_CLASSES = [
  "av-blue", "av-purple", "av-green", "av-red", "av-orange",
  "av-cyan", "av-pink", "av-yellow", "av-teal", "av-indigo",
];
function avatarClass(name: string) {
  const code = (name.charCodeAt(0) || 0) + (name.charCodeAt(1) || 0);
  return AV_CLASSES[code % AV_CLASSES.length];
}

const YEAR_SECS = 365 * 86400;

interface Audit {
  /** Groups of entries sharing the same password, largest first */
  reused: Entry[][];
  /** Entries whose password scores Weak or Fair */
  weak: { entry: Entry; score: number }[];
  /** Entries not updated in over a year, oldest first */
  stale: Entry[];
  issueCount: number;
}

function runAudit(entries: Entry[]): Audit {
  const byPassword = new Map<string, Entry[]>();
  for (const e of entries) {
    if (!e.password) continue;
    const group = byPassword.get(e.password);
    if (group) group.push(e);
    else byPassword.set(e.password, [e]);
  }
  const reused = [...byPassword.values()]
    .filter((g) => g.length >= 2)
    .sort((a, b) => b.length - a.length);

  const weak = entries
    .map((entry) => ({ entry, score: passwordStrength(entry.password) }))
    .filter(({ score }) => score <= 2)
    .sort((a, b) => a.score - b.score);

  const cutoff = Date.now() / 1000 - YEAR_SECS;
  const stale = entries
    .filter((e) => e.updated_at < cutoff)
    .sort((a, b) => a.updated_at - b.updated_at);

  const issueCount =
    reused.reduce((n, g) => n + g.length, 0) + weak.length + stale.length;

  return { reused, weak, stale, issueCount };
}

function ageLabel(updatedAt: number): string {
  const days = Math.floor((Date.now() / 1000 - updatedAt) / 86400);
  if (days < 365 * 2) return `${Math.floor(days / 30)} months ago`;
  return `${Math.floor(days / 365)} years ago`;
}

function EntryRow({ entry, detail, detailColor, onSelect }: {
  entry: Entry;
  detail: string;
  detailColor: string;
  onSelect: (entry: Entry) => void;
}) {
  return (
    <div
      onClick={() => onSelect(entry)}
      style={{
        display: "flex", alignItems: "center", gap: "10px",
        padding: "8px 12px", cursor: "pointer",
        borderBottom: "1px solid rgba(255,255,255,0.045)",
        transition: "background 0.11s",
      }}
      onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.035)"; }}
      onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; }}
    >
      <div className={`avatar ${avatarClass(entry.name)}`}
        style={{ width: "26px", height: "26px", borderRadius: "7px", fontSize: "11px", flexShrink: 0 }}>
        {entry.name.charAt(0).toUpperCase()}
      </div>
      <div style={{ minWidth: 0, flex: 1 }}>
        <div style={{ fontWeight: 500, fontSize: "13px", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
          {entry.name}
        </div>
        <div style={{ color: "var(--muted-dim)", fontSize: "11px", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
          {entry.email}
        </div>
      </div>
      <span style={{ fontSize: "11px", fontWeight: 600, color: detailColor, flexShrink: 0, whiteSpace: "nowrap" }}>
        {detail}
      </span>
    </div>
  );
}

function Section({ icon, title, count, tone, children, empty }: {
  icon: React.ReactNode;
  title: string;
  count: number;
  tone: string;
  children?: React.ReactNode;
  empty: string;
}) {
  return (
    <div style={{
      background: "rgba(255,255,255,0.03)", borderRadius: "var(--r-md)",
      border: "1px solid var(--border-dim)", overflow: "hidden", marginBottom: "14px",
    }}>
      <div style={{
        display: "flex", alignItems: "center", gap: "8px",
        padding: "10px 12px", borderBottom: count > 0 ? "1px solid var(--border-dim)" : "none",
      }}>
        <span style={{ color: count > 0 ? tone : "var(--success)", display: "flex" }}>{icon}</span>
        <span style={{ fontSize: "12.5px", fontWeight: 600, flex: 1 }}>{title}</span>
        <span style={{
          fontSize: "11px", fontWeight: 700, padding: "1px 8px", borderRadius: "9px",
          background: count > 0 ? "rgba(255,255,255,0.07)" : "rgba(34,197,94,0.12)",
          color: count > 0 ? tone : "var(--success)",
        }}>
          {count > 0 ? count : "✓"}
        </span>
      </div>
      {count > 0
        ? children
        : <div style={{ padding: "9px 12px", fontSize: "11.5px", color: "var(--muted-dim)" }}>{empty}</div>
      }
    </div>
  );
}

export default function SecurityView({ entries, onSelect }: {
  entries: Entry[];
  onSelect: (entry: Entry) => void;
}) {
  const audit = useMemo(() => runAudit(entries), [entries]);

  return (
    <div style={{ flex: 1, overflowY: "auto", padding: "22px 26px" }}>
      {/* Header */}
      <div style={{ display: "flex", alignItems: "center", gap: "12px", marginBottom: "18px" }}>
        <div style={{
          width: "40px", height: "40px", borderRadius: "10px",
          display: "flex", alignItems: "center", justifyContent: "center",
          background: audit.issueCount > 0 ? "rgba(245,158,11,0.12)" : "rgba(34,197,94,0.12)",
          color: audit.issueCount > 0 ? "var(--warning, #F59E0B)" : "var(--success)",
        }}>
          {audit.issueCount > 0 ? <ShieldAlert size={20} strokeWidth={2} /> : <ShieldCheck size={20} strokeWidth={2} />}
        </div>
        <div>
          <div style={{ fontSize: "18px", fontWeight: 700, letterSpacing: "-0.02em", lineHeight: 1.15 }}>
            Password health
          </div>
          <div style={{ fontSize: "12px", color: "var(--muted)" }}>
            {entries.length === 0
              ? "No entries to audit yet."
              : audit.issueCount === 0
                ? "Everything looks good."
                : `${audit.issueCount} issue${audit.issueCount === 1 ? "" : "s"} across ${entries.length} entries.`}
          </div>
        </div>
      </div>

      {/* Reused passwords, grouped */}
      <Section
        icon={<Copy size={14} strokeWidth={2} />}
        title="Reused passwords"
        count={audit.reused.reduce((n, g) => n + g.length, 0)}
        tone="var(--danger)"
        empty="No two entries share a password."
      >
        {audit.reused.map((group, gi) => (
          <div key={gi} style={{ borderBottom: gi < audit.reused.length - 1 ? "1px solid var(--border-dim)" : "none" }}>
            <div style={{
              padding: "6px 12px 2px", fontSize: "10px", fontWeight: 700,
              textTransform: "uppercase", letterSpacing: "0.08em", color: "var(--muted-dim)",
            }}>
              Same password · {group.length} entries
            </div>
            {group.map((entry) => (
              <EntryRow key={entry.id} entry={entry} onSelect={onSelect}
                detail={`shared ×${group.length}`} detailColor="var(--danger)" />
            ))}
          </div>
        ))}
      </Section>

      {/* Weak passwords */}
      <Section
        icon={<KeyRound size={14} strokeWidth={2} />}
        title="Weak passwords"
        count={audit.weak.length}
        tone="var(--warning, #F59E0B)"
        empty="Every password scores Good or Strong."
      >
        {audit.weak.map(({ entry, score }) => (
          <EntryRow key={entry.id} entry={entry} onSelect={onSelect}
            detail={STRENGTH_LABELS[score]}
            detailColor={score <= 1 ? "var(--danger)" : "var(--warning, #F59E0B)"} />
        ))}
      </Section>

      {/* Stale passwords */}
      <Section
        icon={<History size={14} strokeWidth={2} />}
        title="Not updated in over a year"
        count={audit.stale.length}
        tone="var(--warning, #F59E0B)"
        empty="Every password was touched within the last year."
      >
        {audit.stale.map((entry) => (
          <EntryRow key={entry.id} entry={entry} onSelect={onSelect}
            detail={ageLabel(entry.updated_at)} detailColor="var(--muted)" />
        ))}
      </Section>
    </div>
  );
}
