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

/**
 * Cryptographically secure random integer in [0, max).
 * Uses rejection sampling so every value is equally likely
 * (a plain modulo would bias toward lower values).
 */
function randInt(max: number): number {
  const limit = Math.floor(0x100000000 / max) * max;
  const buf = new Uint32Array(1);
  do {
    crypto.getRandomValues(buf);
  } while (buf[0] >= limit);
  return buf[0] % max;
}

export function generatePassword(opts: GenOptions = DEFAULT_OPTIONS): string {
  let pool = LOWER;
  const required: string[] = [];

  if (opts.upper)   { pool += UPPER;  required.push(UPPER[randInt(UPPER.length)]); }
  if (opts.numbers) { pool += NUMS;   required.push(NUMS[randInt(NUMS.length)]); }
  if (opts.symbols) { pool += SYMS;   required.push(SYMS[randInt(SYMS.length)]); }

  const remaining = opts.length - required.length;
  const chars = Array.from({ length: Math.max(remaining, 0) }, () =>
    pool[randInt(pool.length)]
  );

  const all = [...required, ...chars];
  // Fisher-Yates shuffle
  for (let i = all.length - 1; i > 0; i--) {
    const j = randInt(i + 1);
    [all[i], all[j]] = [all[j], all[i]];
  }
  return all.join("");
}
