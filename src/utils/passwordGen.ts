export interface GenOptions {
  length: number;
  upper: boolean;
  numbers: boolean;
  symbols: boolean;
}

export const DEFAULT_OPTIONS: GenOptions = {
  length: 20,
  upper: true,
  numbers: true,
  symbols: true,
};

const LOWER = "abcdefghijklmnopqrstuvwxyz";
const UPPER = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const NUMS  = "0123456789";
const SYMS  = "!@#$%^&*()-_=+[]{}|;:,.<>?";

export function generatePassword(opts: GenOptions = DEFAULT_OPTIONS): string {
  let pool = LOWER;
  const required: string[] = [];

  if (opts.upper)   { pool += UPPER;  required.push(UPPER[Math.floor(Math.random() * UPPER.length)]); }
  if (opts.numbers) { pool += NUMS;   required.push(NUMS[Math.floor(Math.random() * NUMS.length)]); }
  if (opts.symbols) { pool += SYMS;   required.push(SYMS[Math.floor(Math.random() * SYMS.length)]); }

  const remaining = opts.length - required.length;
  const chars = Array.from({ length: Math.max(remaining, 0) }, () =>
    pool[Math.floor(Math.random() * pool.length)]
  );

  const all = [...required, ...chars];
  // Fisher-Yates shuffle
  for (let i = all.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [all[i], all[j]] = [all[j], all[i]];
  }
  return all.join("");
}
