export interface Theme {
  id: string;
  name: string;
  accent: string;
  bg: string;
  vars: Record<string, string>;
  bodyBackground: string;
}

export const THEMES: Theme[] = [
  {
    id: "ocean",
    name: "Ocean",
    accent: "#4D9DE0",
    bg: "#050D1A",
    vars: {
      "--bg": "#050D1A",
      "--bg2": "rgba(255,255,255,0.06)",
      "--bg3": "rgba(255,255,255,0.10)",
      "--border": "rgba(255,255,255,0.10)",
      "--text": "#E2EAF4",
      "--muted": "#6B90B8",
      "--accent": "#4D9DE0",
      "--accent-hover": "#69B0E8",
      "--accent-glow": "rgba(77,157,224,0.30)",
      "--glass-bg": "rgba(255,255,255,0.06)",
      "--glass-border": "rgba(255,255,255,0.10)",
      "--select-bg": "#0D1F38",
    },
    bodyBackground: `
      radial-gradient(ellipse 70% 60% at 15% 15%, rgba(30,80,200,0.35) 0%, transparent 60%),
      radial-gradient(ellipse 60% 50% at 85% 85%, rgba(15,50,140,0.30) 0%, transparent 60%),
      radial-gradient(ellipse 50% 40% at 65% 5%,  rgba(60,110,220,0.18) 0%, transparent 50%),
      linear-gradient(145deg, #050D1A 0%, #081526 55%, #0B1E3D 100%)
    `,
  },
  {
    id: "midnight",
    name: "Midnight",
    accent: "#A78BFA",
    bg: "#0A0A14",
    vars: {
      "--bg": "#0A0A14",
      "--bg2": "rgba(255,255,255,0.05)",
      "--bg3": "rgba(255,255,255,0.09)",
      "--border": "rgba(255,255,255,0.09)",
      "--text": "#E8E0F8",
      "--muted": "#7B6BA8",
      "--accent": "#A78BFA",
      "--accent-hover": "#C4A9FF",
      "--accent-glow": "rgba(167,139,250,0.30)",
      "--glass-bg": "rgba(255,255,255,0.05)",
      "--glass-border": "rgba(255,255,255,0.09)",
      "--select-bg": "#161626",
    },
    bodyBackground: `
      radial-gradient(ellipse 70% 60% at 15% 15%, rgba(100,50,220,0.30) 0%, transparent 60%),
      radial-gradient(ellipse 60% 50% at 85% 85%, rgba(70,30,160,0.25) 0%, transparent 60%),
      radial-gradient(ellipse 50% 40% at 65% 5%,  rgba(130,80,240,0.15) 0%, transparent 50%),
      linear-gradient(145deg, #0A0A14 0%, #0E0B1E 55%, #130D2A 100%)
    `,
  },
  {
    id: "forest",
    name: "Forest",
    accent: "#34D399",
    bg: "#060F0A",
    vars: {
      "--bg": "#060F0A",
      "--bg2": "rgba(255,255,255,0.05)",
      "--bg3": "rgba(255,255,255,0.09)",
      "--border": "rgba(255,255,255,0.09)",
      "--text": "#DDF5EC",
      "--muted": "#5A9070",
      "--accent": "#34D399",
      "--accent-hover": "#50E3AC",
      "--accent-glow": "rgba(52,211,153,0.28)",
      "--glass-bg": "rgba(255,255,255,0.05)",
      "--glass-border": "rgba(255,255,255,0.09)",
      "--select-bg": "#0E2018",
    },
    bodyBackground: `
      radial-gradient(ellipse 70% 60% at 15% 15%, rgba(20,120,70,0.30) 0%, transparent 60%),
      radial-gradient(ellipse 60% 50% at 85% 85%, rgba(10,80,50,0.25) 0%, transparent 60%),
      radial-gradient(ellipse 50% 40% at 65% 5%,  rgba(30,140,90,0.15) 0%, transparent 50%),
      linear-gradient(145deg, #060F0A 0%, #081407 55%, #0A1B0E 100%)
    `,
  },
  {
    id: "crimson",
    name: "Crimson",
    accent: "#FB7185",
    bg: "#0F0A0A",
    vars: {
      "--bg": "#0F0A0A",
      "--bg2": "rgba(255,255,255,0.05)",
      "--bg3": "rgba(255,255,255,0.09)",
      "--border": "rgba(255,255,255,0.09)",
      "--text": "#F8E8E8",
      "--muted": "#9B6B6B",
      "--accent": "#FB7185",
      "--accent-hover": "#FF8C9E",
      "--accent-glow": "rgba(251,113,133,0.30)",
      "--glass-bg": "rgba(255,255,255,0.05)",
      "--glass-border": "rgba(255,255,255,0.09)",
      "--select-bg": "#201414",
    },
    bodyBackground: `
      radial-gradient(ellipse 70% 60% at 15% 15%, rgba(180,40,40,0.28) 0%, transparent 60%),
      radial-gradient(ellipse 60% 50% at 85% 85%, rgba(120,25,25,0.22) 0%, transparent 60%),
      radial-gradient(ellipse 50% 40% at 65% 5%,  rgba(200,60,60,0.14) 0%, transparent 50%),
      linear-gradient(145deg, #0F0A0A 0%, #180B0B 55%, #200E0E 100%)
    `,
  },
  {
    id: "slate",
    name: "Slate",
    accent: "#22D3EE",
    bg: "#0A0C0F",
    vars: {
      "--bg": "#0A0C0F",
      "--bg2": "rgba(255,255,255,0.06)",
      "--bg3": "rgba(255,255,255,0.10)",
      "--border": "rgba(255,255,255,0.10)",
      "--text": "#E0EDF5",
      "--muted": "#5B7A8A",
      "--accent": "#22D3EE",
      "--accent-hover": "#38E8FF",
      "--accent-glow": "rgba(34,211,238,0.28)",
      "--glass-bg": "rgba(255,255,255,0.06)",
      "--glass-border": "rgba(255,255,255,0.10)",
      "--select-bg": "#131820",
    },
    bodyBackground: `
      radial-gradient(ellipse 70% 60% at 15% 15%, rgba(20,80,120,0.30) 0%, transparent 60%),
      radial-gradient(ellipse 60% 50% at 85% 85%, rgba(10,60,90,0.25) 0%, transparent 60%),
      radial-gradient(ellipse 50% 40% at 65% 5%,  rgba(30,100,150,0.15) 0%, transparent 50%),
      linear-gradient(145deg, #0A0C0F 0%, #0C1018 55%, #0E1520 100%)
    `,
  },
];

export const DEFAULT_THEME_ID = "ocean";

export function applyTheme(themeId: string) {
  const theme = THEMES.find((t) => t.id === themeId) ?? THEMES[0];
  const root = document.documentElement;
  for (const [key, value] of Object.entries(theme.vars)) {
    root.style.setProperty(key, value);
  }
  document.body.style.background = theme.bodyBackground;
  document.body.style.backgroundAttachment = "fixed";
}
