import { describe, it, expect } from "vitest";
import {
  generatePassword, DEFAULT_OPTIONS, type GenOptions,
  generatePassphrase, DEFAULT_PASSPHRASE, passphraseEntropyBits,
} from "./passwordGen";
import { EFF_SHORT_WORDLIST } from "./wordlist";

const HAS_UPPER = /[A-Z]/;
const HAS_NUMBER = /[0-9]/;
const HAS_SYMBOL = /[!@#$%^&*()\-_=+\[\]{}|;:,.<>?]/;
const ONLY_LOWER = /^[a-z]+$/;

describe("generatePassword", () => {
  it("returns the requested length", () => {
    expect(generatePassword({ ...DEFAULT_OPTIONS, length: 16 })).toHaveLength(16);
    expect(generatePassword({ ...DEFAULT_OPTIONS, length: 32 })).toHaveLength(32);
  });

  it("length is stable across multiple calls", () => {
    const lengths = Array.from({ length: 3 }, () => generatePassword(DEFAULT_OPTIONS).length);
    expect(new Set(lengths).size).toBe(1);
  });

  it("contains uppercase when upper is enabled", () => {
    expect(HAS_UPPER.test(generatePassword({ ...DEFAULT_OPTIONS, upper: true, length: 30 }))).toBe(true);
  });

  it("contains a digit when numbers is enabled", () => {
    expect(HAS_NUMBER.test(generatePassword({ ...DEFAULT_OPTIONS, numbers: true, length: 30 }))).toBe(true);
  });

  it("contains a symbol when symbols is enabled", () => {
    expect(HAS_SYMBOL.test(generatePassword({ ...DEFAULT_OPTIONS, symbols: true, length: 30 }))).toBe(true);
  });

  it("contains only lowercase when all flags are off", () => {
    const opts: GenOptions = { length: 20, upper: false, numbers: false, symbols: false };
    expect(ONLY_LOWER.test(generatePassword(opts))).toBe(true);
  });

  it("handles length 1 with all flags off", () => {
    const opts: GenOptions = { length: 1, upper: false, numbers: false, symbols: false };
    const result = generatePassword(opts);
    expect(result).toHaveLength(1);
    expect(ONLY_LOWER.test(result)).toBe(true);
  });

  it("uses defaults when called with no arguments", () => {
    expect(generatePassword()).toHaveLength(DEFAULT_OPTIONS.length);
  });
});

describe("generatePassphrase", () => {
  // "yo-yo" is in the list, so splitting on the default "-" separator would
  // occasionally see an extra word. Every test that counts words uses ".".
  const dotted = (o: Partial<typeof DEFAULT_PASSPHRASE> = {}) =>
    generatePassphrase({ ...DEFAULT_PASSPHRASE, separator: ".", ...o }).split(".");

  it("produces the requested number of words", () => {
    for (const words of [3, 5, 10]) {
      expect(dotted({ words })).toHaveLength(words);
    }
  });

  it("joins with the chosen separator", () => {
    for (const separator of ["-", ".", "_", " "]) {
      const phrase = generatePassphrase({ ...DEFAULT_PASSPHRASE, words: 4, separator });
      expect(phrase.split(separator).length).toBeGreaterThanOrEqual(4);
      expect(phrase.startsWith(separator)).toBe(false);
      expect(phrase.endsWith(separator)).toBe(false);
    }
  });

  it("draws every word from the wordlist", () => {
    for (const word of dotted({ words: 8 })) {
      expect(EFF_SHORT_WORDLIST).toContain(word);
    }
  });

  it("capitalises only the first letter of each word", () => {
    for (const word of dotted({ capitalize: true })) {
      expect(word).toMatch(/^[A-Z][a-z-]*$/);
    }
  });

  it("adds exactly one digit when asked", () => {
    const phrase = generatePassphrase({ ...DEFAULT_PASSPHRASE, number: true });
    expect(phrase.replace(/[^0-9]/g, "")).toHaveLength(1);
  });

  it("does not repeat itself", () => {
    const phrases = new Set(
      Array.from({ length: 50 }, () => generatePassphrase(DEFAULT_PASSPHRASE))
    );
    expect(phrases.size).toBe(50);
  });

  it("reports the entropy of the word choices", () => {
    // 1296 words is 10.34 bits each, so five words clear 50 bits
    expect(passphraseEntropyBits({ ...DEFAULT_PASSPHRASE, words: 5 })).toBeCloseTo(51.7, 1);
  });
});

describe("the bundled wordlist", () => {
  it("is the full EFF short list with no duplicates", () => {
    expect(EFF_SHORT_WORDLIST).toHaveLength(1296);
    expect(new Set(EFF_SHORT_WORDLIST).size).toBe(1296);
  });

  it("contains only short, typable words", () => {
    for (const word of EFF_SHORT_WORDLIST) {
      expect(word).toMatch(/^[a-z-]{3,6}$/);
    }
  });
});
