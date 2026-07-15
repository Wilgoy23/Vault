import { useState } from "react";
import { parseOtpauth, looksLikeBase32 } from "../utils/totp";

interface Props {
  value: string;
  onChange: (secret: string) => void;
  /** Called with the issuer/account labels when an otpauth:// URI is
   * pasted, so the caller can prefill e.g. the entry name. */
  onMeta?: (meta: { issuer?: string; account?: string }) => void;
  placeholder?: string;
}

/** 2FA secret input that accepts either a raw Base32 secret or a full
 * otpauth://totp/... URI (pasted from a QR code / "can't scan?" link),
 * converting the latter to its secret on the spot. */
export default function TotpSecretInput({ value, onChange, onMeta, placeholder }: Props) {
  const [imported, setImported] = useState(false);

  const handleChange = (raw: string) => {
    const parsed = parseOtpauth(raw);
    if (parsed) {
      onChange(parsed.secret);
      if (parsed.issuer || parsed.account) {
        onMeta?.({ issuer: parsed.issuer, account: parsed.account });
      }
      setImported(true);
      return;
    }
    setImported(false);
    onChange(raw);
  };

  const invalid = value.trim() !== "" && !looksLikeBase32(value);

  return (
    <div style={{ flex: 1, minWidth: 0 }}>
      <input
        value={value}
        onChange={(e) => handleChange(e.target.value)}
        placeholder={placeholder ?? "Base32 secret or otpauth:// link"}
        autoComplete="off"
        spellCheck={false}
        style={{
          width: "100%", fontFamily: "var(--mono)", fontSize: "12.5px",
          letterSpacing: "0.05em",
          borderColor: invalid ? "var(--danger)" : undefined,
        }}
      />
      {imported && (
        <div style={{ fontSize: "11px", color: "var(--success)", marginTop: "4px" }}>
          Secret imported from otpauth link.
        </div>
      )}
      {invalid && (
        <div style={{ fontSize: "11px", color: "var(--danger)", marginTop: "4px" }}>
          Doesn't look like a Base32 secret or otpauth:// link.
        </div>
      )}
    </div>
  );
}
