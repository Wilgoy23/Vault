import { EFF_SHORT_WORDLIST } from "./wordlist";

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

// ── Passphrases ──────────────────────────────────────────────────────────────

export interface PassphraseOptions {
  words: number;
  separator: string;
  capitalize: boolean;
  /** Append a digit to one randomly chosen word, for sites that demand one */
  number: boolean;
}

export const DEFAULT_PASSPHRASE: PassphraseOptions = {
  words: 5,
  separator: "-",
  capitalize: false,
  number: false,
};

export const SEPARATORS = ["-", ".", "_", " "] as const;

/** Entropy of the word choices alone, in bits. Capitalisation and the
 *  appended digit are left out: an attacker who knows the scheme gains
 *  little from them, so counting them would overstate the strength. */
export function passphraseEntropyBits(opts: PassphraseOptions): number {
  return opts.words * Math.log2(EFF_SHORT_WORDLIST.length);
}

export function generatePassphrase(opts: PassphraseOptions = DEFAULT_PASSPHRASE): string {
  const count = Math.max(1, Math.floor(opts.words));
  const words = Array.from({ length: count }, () => {
    const word = EFF_SHORT_WORDLIST[randInt(EFF_SHORT_WORDLIST.length)];
    return opts.capitalize ? word.charAt(0).toUpperCase() + word.slice(1) : word;
  });

  if (opts.number) {
    const at = randInt(words.length);
    words[at] += randInt(10);
  }

  return words.join(opts.separator);
}
