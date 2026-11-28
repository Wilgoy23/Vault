/** Appearance is two independent choices:
 *  - a colour scheme (palette): the colours themselves, dark or light
 *  - a style: how surfaces are drawn (glass, neumorphic, clay, flat, brutalist)
 *
 *  A palette sets a handful of base colours on <html>; app.css derives every
 *  tint, border and shadow from them, and each style redefines the surface
 *  tokens under html[data-style=…]. Either can change without touching the
 *  other. */

export type Mode = "dark" | "light";

export interface Theme {
  id: string;
  name: string;
  mode: Mode;
  accent: string;
  bg: string;
  vars: Record<string, string>;
}

type Rgb = [number, number, number];

function parse(hex: string): Rgb {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** "r,g,b" for use inside rgba(var(--x-rgb), alpha) */
function rgb(hex: string): string {
  return parse(hex).join(",");
}

/** `share` of a blended with the rest of b, as CSS color-mix(in srgb) would.
 *  Done here rather than in CSS because color-mix() needs iOS 16.2. */
function mix(a: string, b: string, share: number): string {
  const x = parse(a), y = parse(b);
  const c = x.map((v, i) => Math.round(v * share + y[i] * (1 - share)));
  return "#" + c.map((v) => v.toString(16).padStart(2, "0")).join("");
}

const WHITE = "#FFFFFF";
const BLACK = "#000000";
/** The light mode shadow ink; matches --shade-rgb in app.css */
const LIGHT_SHADE = "#283250";

interface PaletteSpec {
  id: string;
  name: string;
  mode: Mode;
  bg: string;
  text: string;
  muted: string;
  accent: string;
  accentHover: string;
  /** Opaque control colour, for native <select> lists */
  selectBg: string;
  /** Background artwork behind glass surfaces */
  art: string;
  /** Optional overrides for the derived tones */
  mutedDim?: string;
  fgMid?: string;
}

function palette(p: PaletteSpec): Theme {
  const light = p.mode === "light";
  /** The background with n% of the text colour in it */
  const tone = (n: number) => mix(p.bg, p.text, 1 - n / 100);

  const neu = light ? p.bg : tone(12);
  const clay = light ? mix(p.bg, WHITE, 0.55) : tone(16);
  const clayHi = light ? mix(p.bg, WHITE, 0.35) : tone(24);

  const vars: Record<string, string> = {
    "--bg": p.bg,
    "--bg-rgb": rgb(p.bg),
    "--text": p.text,
    "--muted": p.muted,
    "--muted-dim": p.mutedDim ?? mix(p.muted, p.bg, 0.58),
    "--fg-mid": p.fgMid ?? mix(p.text, p.accent, 0.86),
    "--text-soft": mix(p.text, p.muted, 0.68),
    "--accent": p.accent,
    "--accent-hover": p.accentHover,
    "--accent-rgb": rgb(p.accent),
    "--accent-deep": mix(p.accent, BLACK, 0.6),
    "--accent-dark": mix(p.accent, BLACK, 0.7),
    "--select-solid": p.selectBg,
    "--art": p.art,

    // Solid surfaces a step or two off the background, for the Flat style
    "--tone-3": tone(3),
    "--tone-4": tone(4),
    "--tone-5": tone(5),
    "--tone-7": tone(7),
    "--tone-8": tone(8),

    // Neumorphism: the surface and its dark and light shadows
    "--neu": neu,
    "--neu-lo": light ? mix(p.bg, LIGHT_SHADE, 0.76) : mix(neu, BLACK, 0.48),
    "--neu-hi": light ? "rgba(255,255,255,0.9)" : mix(neu, WHITE, 0.9),

    // Claymorphism: the shape, a lighter raised one, and sunken fills
    "--clay": clay,
    "--clay-hi": clayHi,
    "--clay-well": mix(clay, p.bg, 0.8),
    "--clay-sunk": mix(clay, p.bg, 0.7),
    "--clay-hover": mix(clayHi, p.text, 0.9),
    "--clay-swatch": light ? mix(p.bg, WHITE, 0.45) : tone(22),
  };
  return { id: p.id, name: p.name, mode: p.mode, accent: p.accent, bg: p.bg, vars };
}

/** Soft colour fields behind a dark glass UI */
const darkArt = (a: string, b: string, c: string, from: string, mid: string, to: string) => `
  radial-gradient(ellipse 70% 60% at 15% 15%, ${a} 0%, transparent 60%),
  radial-gradient(ellipse 60% 50% at 85% 85%, ${b} 0%, transparent 60%),
  radial-gradient(ellipse 50% 40% at 65% 5%,  ${c} 0%, transparent 50%),
  linear-gradient(145deg, ${from} 0%, ${mid} 55%, ${to} 100%)`;

/** Pastel colour fields behind a light glass UI */
const lightArt = (a: string, b: string, from: string, to: string) => `
  radial-gradient(ellipse 70% 60% at 12% 10%, ${a} 0%, transparent 60%),
  radial-gradient(ellipse 60% 55% at 88% 90%, ${b} 0%, transparent 60%),
  linear-gradient(145deg, ${from} 0%, ${to} 100%)`;

export const THEMES: Theme[] = [
  // ── Dark ────────────────────────────────────────────────────────────────
  palette({
    id: "ocean", name: "Ocean", mode: "dark",
    bg: "#050D1A", text: "#E2EAF4", muted: "#6B90B8", mutedDim: "#3E5F7E", fgMid: "#C5D8EC",
    accent: "#4D9DE0", accentHover: "#69B0E8", selectBg: "#0D1F38",
    art: darkArt("rgba(30,80,200,0.35)", "rgba(15,50,140,0.30)", "rgba(60,110,220,0.18)", "#050D1A", "#081526", "#0B1E3D"),
  }),
  palette({
    id: "midnight", name: "Midnight", mode: "dark",
    bg: "#0A0A14", text: "#E8E0F8", muted: "#7B6BA8",
    accent: "#A78BFA", accentHover: "#C4A9FF", selectBg: "#161626",
    art: darkArt("rgba(100,50,220,0.30)", "rgba(70,30,160,0.25)", "rgba(130,80,240,0.15)", "#0A0A14", "#0E0B1E", "#130D2A"),
  }),
  palette({
    id: "forest", name: "Forest", mode: "dark",
    bg: "#060F0A", text: "#DDF5EC", muted: "#5A9070",
    accent: "#34D399", accentHover: "#50E3AC", selectBg: "#0E2018",
    art: darkArt("rgba(20,120,70,0.30)", "rgba(10,80,50,0.25)", "rgba(30,140,90,0.15)", "#060F0A", "#081407", "#0A1B0E"),
  }),
  palette({
    id: "crimson", name: "Crimson", mode: "dark",
    bg: "#0F0A0A", text: "#F8E8E8", muted: "#9B6B6B",
    accent: "#FB7185", accentHover: "#FF8C9E", selectBg: "#201414",
    art: darkArt("rgba(180,40,40,0.28)", "rgba(120,25,25,0.22)", "rgba(200,60,60,0.14)", "#0F0A0A", "#180B0B", "#200E0E"),
  }),
  palette({
    id: "slate", name: "Slate", mode: "dark",
    bg: "#0A0C0F", text: "#E0EDF5", muted: "#5B7A8A",
    accent: "#22D3EE", accentHover: "#38E8FF", selectBg: "#131820",
    art: darkArt("rgba(20,80,120,0.30)", "rgba(10,60,90,0.25)", "rgba(30,100,150,0.15)", "#0A0C0F", "#0C1018", "#0E1520"),
  }),
  palette({
    id: "graphite", name: "Graphite", mode: "dark",
    bg: "#121212", text: "#EDEDED", muted: "#8A8A8A",
    accent: "#F5A524", accentHover: "#FFB940", selectBg: "#1E1E1E",
    art: darkArt("rgba(245,165,36,0.12)", "rgba(120,120,120,0.14)", "rgba(245,165,36,0.06)", "#121212", "#161616", "#1B1B1B"),
  }),

  // ── Light ───────────────────────────────────────────────────────────────
  palette({
    id: "cloud", name: "Cloud", mode: "light",
    bg: "#E6ECF3", text: "#1B2A3D", muted: "#5B6F87", mutedDim: "#8496AB", fgMid: "#2C3E55",
    accent: "#2F7BD8", accentHover: "#1F68C2", selectBg: "#F4F7FB",
    art: lightArt("rgba(120,170,240,0.45)", "rgba(170,200,250,0.50)", "#E9EFF7", "#DCE5F0"),
  }),
  palette({
    id: "lavender", name: "Lavender", mode: "light",
    bg: "#ECE8F6", text: "#2A2340", muted: "#6D6390", mutedDim: "#9A92B8", fgMid: "#3A3157",
    accent: "#7C5CE0", accentHover: "#6847CF", selectBg: "#F7F5FC",
    art: lightArt("rgba(180,150,250,0.45)", "rgba(240,170,220,0.40)", "#F0ECF9", "#E3DDF2"),
  }),
  palette({
    id: "sage", name: "Sage", mode: "light",
    bg: "#E5EEE8", text: "#1D2E24", muted: "#587263", mutedDim: "#8AA195", fgMid: "#2B4034",
    accent: "#2E9E6B", accentHover: "#228757", selectBg: "#F3F8F5",
    art: lightArt("rgba(130,210,160,0.45)", "rgba(200,230,170,0.45)", "#EAF2EC", "#DCE8E0"),
  }),
  palette({
    id: "peach", name: "Peach", mode: "light",
    bg: "#F5E9E4", text: "#3A2420", muted: "#8A665E", mutedDim: "#B5958D", fgMid: "#4F332D",
    accent: "#E0566B", accentHover: "#CB4157", selectBg: "#FBF4F1",
    art: lightArt("rgba(250,170,150,0.45)", "rgba(250,210,160,0.45)", "#F8EEE9", "#F0E0D9"),
  }),
  palette({
    id: "paper", name: "Paper", mode: "light",
    bg: "#EFEDE8", text: "#24231F", muted: "#6F6C63", mutedDim: "#A09C91", fgMid: "#38362F",
    accent: "#127E93", accentHover: "#0D6B7E", selectBg: "#F8F7F4",
    art: lightArt("rgba(120,200,210,0.30)", "rgba(230,210,170,0.40)", "#F3F1EC", "#E6E3DC"),
  }),
];

export const DEFAULT_THEME_ID = "ocean";

export interface UiStyle {
  id: string;
  name: string;
  description: string;
}

export const STYLES: UiStyle[] = [
  { id: "glass", name: "Glass", description: "Frosted, translucent panels over soft colour" },
  { id: "neu", name: "Neumorphism", description: "Soft shapes pressed out of and into the surface" },
  { id: "clay", name: "Claymorphism", description: "Puffy, rounded shapes with inner highlights" },
  { id: "flat", name: "Flat", description: "Solid surfaces and thin borders, no effects" },
  { id: "brutal", name: "Brutalism", description: "Hard edges, thick outlines and offset shadows" },
];

export const DEFAULT_STYLE_ID = "glass";

export const THEME_KEY = "vault_theme";
export const STYLE_KEY = "vault_style";

/** The saved choices, for windows (like the overlay) that only read them. */
export function savedAppearance(): { themeId: string; styleId: string } {
  let themeId = DEFAULT_THEME_ID;
  let styleId = DEFAULT_STYLE_ID;
  try {
    themeId = localStorage.getItem(THEME_KEY) ?? DEFAULT_THEME_ID;
    styleId = localStorage.getItem(STYLE_KEY) ?? DEFAULT_STYLE_ID;
  } catch { /* storage unavailable: defaults */ }
  return { themeId, styleId };
}

export function applyTheme(themeId: string) {
  const theme = THEMES.find((t) => t.id === themeId) ?? THEMES[0];
  const root = document.documentElement;
  // Every palette sets the same keys, so this replaces the previous one fully
  for (const [key, value] of Object.entries(theme.vars)) {
    root.style.setProperty(key, value);
  }
  root.dataset.mode = theme.mode;
  root.style.colorScheme = theme.mode;
}

export function applyStyle(styleId: string) {
  const style = STYLES.find((s) => s.id === styleId) ?? STYLES[0];
  document.documentElement.dataset.style = style.id;
}
