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
