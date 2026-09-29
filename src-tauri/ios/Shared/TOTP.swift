// RFC 6238 codes, matching src/utils/totp.ts: SHA-1, 6 digits, 30 seconds.

import CryptoKit
import Foundation

enum TOTP {
    static func code(secret: String, at date: Date = Date()) -> String? {
        let key = base32Decode(secret)
        guard !key.isEmpty else { return nil }

        var counter = UInt64(date.timeIntervalSince1970 / 30).bigEndian
        let message = Data(bytes: &counter, count: MemoryLayout<UInt64>.size)
        let mac = Array(HMAC<Insecure.SHA1>.authenticationCode(for: message, using: SymmetricKey(data: key)))

        let offset = Int(mac[19] & 0x0f)
        let value = (UInt32(mac[offset] & 0x7f) << 24)
            | (UInt32(mac[offset + 1]) << 16)
            | (UInt32(mac[offset + 2]) << 8)
            | UInt32(mac[offset + 3])
        return String(format: "%06u", value % 1_000_000)
    }

    /// Like the frontend's decoder, skips anything outside the alphabet
    /// (spaces, dashes, padding).
    private static func base32Decode(_ input: String) -> Data {
        let alphabet = Array("ABCDEFGHIJKLMNOPQRSTUVWXYZ234567")
        var bits = 0
        var value: UInt32 = 0
        var output = Data()
        for char in input.uppercased() {
            guard let index = alphabet.firstIndex(of: char) else { continue }
            value = (value << 5) | UInt32(index)
            bits += 5
            if bits >= 8 {
                output.append(UInt8((value >> UInt32(bits - 8)) & 0xff))
                bits -= 8
            }
        }
        return output
    }
}
