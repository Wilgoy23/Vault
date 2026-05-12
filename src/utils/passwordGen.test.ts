import { describe, it, expect } from "vitest";
import { generatePassword, DEFAULT_OPTIONS, type GenOptions } from "./passwordGen";

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
