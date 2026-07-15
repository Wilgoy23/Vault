function base32Decode(input: string): Uint8Array {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
  const clean = input.toUpperCase().replace(/[^A-Z2-7]/g, "");
  let bits = 0, value = 0;
  const output: number[] = [];
  for (const char of clean) {
    const idx = alphabet.indexOf(char);
    if (idx < 0) continue;
    value = (value << 5) | idx;
    bits += 5;
    if (bits >= 8) {
      output.push((value >>> (bits - 8)) & 0xff);
      bits -= 8;
    }
  }
  return new Uint8Array(output);
}

export async function generateTOTP(secret: string): Promise<string> {
  const key = base32Decode(secret);
  if (key.length === 0) return "------";

  const counter = Math.floor(Date.now() / 1000 / 30);
  const counterBytes = new Uint8Array(8);
  let c = counter;
  for (let i = 7; i >= 0; i--) {
    counterBytes[i] = c & 0xff;
    c = Math.floor(c / 256);
  }

  const cryptoKey = await crypto.subtle.importKey(
    "raw", key, { name: "HMAC", hash: "SHA-1" }, false, ["sign"]
  );
  const sig = await crypto.subtle.sign("HMAC", cryptoKey, counterBytes);
  const bytes = new Uint8Array(sig);

  const offset = bytes[19] & 0xf;
  const code =
    ((bytes[offset]     & 0x7f) << 24) |
    ((bytes[offset + 1] & 0xff) << 16) |
    ((bytes[offset + 2] & 0xff) << 8)  |
     (bytes[offset + 3] & 0xff);
  return String(code % 1_000_000).padStart(6, "0");
}

export function totpSecondsLeft(): number {
  return 30 - (Math.floor(Date.now() / 1000) % 30);
}

const BASE32_RE = /^[A-Z2-7]+$/;

/** Uppercases and strips spaces, dashes, and padding — the cosmetic
 * variations services use when displaying a Base32 secret. */
export function cleanBase32(input: string): string {
  return input.toUpperCase().replace(/[\s-]/g, "").replace(/=+$/, "");
}

/** True when the input reads as a Base32 secret after cleaning. Checks
 * the alphabet only (not length), so it flags typos like 0/1/8 without
 * nagging while a secret is still being typed. */
export function looksLikeBase32(input: string): boolean {
  const clean = cleanBase32(input);
  return clean.length > 0 && BASE32_RE.test(clean);
}

export interface OtpauthInfo {
  secret: string;
  issuer?: string;
  account?: string;
}

/** Parses an otpauth://totp/... URI (what QR codes and "can't scan?"
 * links contain) into its secret plus issuer/account labels.
 * Returns null when the input isn't an otpauth URI or has no valid secret. */
export function parseOtpauth(raw: string): OtpauthInfo | null {
  const trimmed = raw.trim();
  if (!/^otpauth:\/\//i.test(trimmed)) return null;

  let secret: string | null = null;
  let issuer: string | undefined;
  let account: string | undefined;
  try {
    const url = new URL(trimmed);
    secret = url.searchParams.get("secret");
    issuer = url.searchParams.get("issuer")?.trim() || undefined;
    // Label is "Issuer:account" or just "account"
    const label = decodeURIComponent(url.pathname.replace(/^\/+/, "")).trim();
    if (label) {
      const sep = label.indexOf(":");
      if (sep >= 0) {
        issuer = issuer || label.slice(0, sep).trim() || undefined;
        account = label.slice(sep + 1).trim() || undefined;
      } else {
        account = label;
      }
    }
  } catch {
    // Malformed URI — still salvage a secret= param if one is present
    const m = trimmed.match(/[?&]secret=([^&\s]+)/i);
    secret = m ? decodeURIComponent(m[1]) : null;
  }

  if (!secret) return null;
  const clean = cleanBase32(secret);
  if (!clean || !BASE32_RE.test(clean)) return null;
  return { secret: clean, issuer, account };
}
