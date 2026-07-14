export function passwordStrength(pw: string): number {
  if (!pw) return 0;
  let score = 0;
  if (pw.length >= 8)  score++;
  if (pw.length >= 12) score++;
  if (/[A-Z]/.test(pw) && /[0-9]/.test(pw)) score++;
  if (/[^A-Za-z0-9]/.test(pw) && pw.length >= 14) score++;
  return Math.min(4, Math.max(1, score));
}

export const STRENGTH_LABELS = ["", "Weak", "Fair", "Good", "Strong"];

export interface MasterFeedback {
  /** 0 = empty, 1–4 = Weak…Strong */
  score: number;
  label: string;
  hint: string | null;
}

/**
 * Stricter scoring for the master password — it protects everything else,
 * so the thresholds are higher than the per-entry heuristic and it comes
 * with an actionable hint for the next improvement.
 */
export function masterPasswordFeedback(pw: string): MasterFeedback {
  if (!pw) return { score: 0, label: "", hint: null };
  if (pw.length < 8) {
    return { score: 1, label: "Too short", hint: "Use at least 8 characters." };
  }

  const variety =
    (/[a-z]/.test(pw) ? 1 : 0) +
    (/[A-Z]/.test(pw) ? 1 : 0) +
    (/[0-9]/.test(pw) ? 1 : 0) +
    (/[^A-Za-z0-9]/.test(pw) ? 1 : 0);

  let score: number;
  if ((pw.length >= 16 && variety >= 3) || pw.length >= 20) score = 4;
  else if ((pw.length >= 12 && variety >= 3) || (pw.length >= 16 && variety >= 2)) score = 3;
  else if (pw.length >= 10 && variety >= 2) score = 2;
  else score = 1;

  // "aaaaaaaaaaaaaaaa" is long but trivial to guess
  if (new Set(pw).size <= 4) score = Math.min(score, 1);

  const hint =
    score === 4 ? null :
    pw.length < 12 ? "Longer is stronger — aim for 12+ characters." :
    variety < 3 ? "Mix in upper case, numbers, or symbols." :
    "A few more characters would make this strong.";

  return { score, label: STRENGTH_LABELS[score], hint };
}
