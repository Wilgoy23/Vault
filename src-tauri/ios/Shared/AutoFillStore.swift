// Shared by the Vault app and its AutoFill extension.
//
// The app publishes a snapshot of the logins (built in src/autofill.rs),
// encrypted under a random key that is replaced on every publish. The
// ciphertext lives in the App Group container; the key lives in the Keychain,
// in an access group both processes share, behind Face ID or the passcode.
// The extension never sees vault.enc or the master key.

import AuthenticationServices
import CryptoKit
import Foundation
import LocalAuthentication
import Security

struct AutoFillSnapshot: Codable {
    var entries: [AutoFillEntry]
}

struct AutoFillEntry: Codable {
    var id: String
    var name: String
    var login: String
    var password: String
    var url: String?
    var totpSecret: String?

    enum CodingKeys: String, CodingKey {
        case id, name, login, password, url
        case totpSecret = "totp_secret"
    }

    /// Lowercased host without "www.", or nil when the entry has no usable URL.
    var host: String? { url.flatMap(AutoFillEntry.host(of:)) }

    static func host(of raw: String) -> String? {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        // Entries are often saved as a bare "github.com"
        let withScheme = trimmed.contains("://") ? trimmed : "https://" + trimmed
        guard let host = URL(string: withScheme)?.host?.lowercased() else { return nil }
        return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
    }

    /// True when this entry belongs to any of the sites iOS is asking about.
    /// Subdomains match either way round (login.example.com ~ example.com).
    func matches(_ services: [ASCredentialServiceIdentifier]) -> Bool {
        guard let mine = host else { return false }
        return services.contains { service in
            guard let theirs = AutoFillEntry.host(of: service.identifier) else { return false }
            return theirs == mine || theirs.hasSuffix("." + mine) || mine.hasSuffix("." + theirs)
        }
    }
}

enum AutoFillStore {
    enum Failure: Error {
        case noContainer
        case notEnabled
        case cancelled
        case noPasscode
        case keychain(OSStatus)
        case corrupt
    }

    /// Must match the entitlements in src-tauri/ios/Config.
    private static let defaultGroup = "group.com.willg.vault"
    private static let keyService = "com.willg.vault.autofill"
    private static let keyAccount = "snapshot-key"
    private static let snapshotName = "autofill.enc"

    /// AltStore and SideStore rename app groups per Apple ID when they
    /// re-sign, and list the real names under ALTAppGroups. The extension's
    /// own Info.plist may not carry the key, so the containing app's is
    /// checked too.
    static let appGroup: String = {
        let appBundleURL = Bundle.main.bundleURL.pathExtension == "appex"
            ? Bundle.main.bundleURL.deletingLastPathComponent().deletingLastPathComponent()
            : Bundle.main.bundleURL
        for bundle in [Bundle.main, Bundle(url: appBundleURL)].compactMap({ $0 }) {
            if let groups = bundle.object(forInfoDictionaryKey: "ALTAppGroups") as? [String],
               let group = groups.first(where: { $0.hasPrefix(defaultGroup) }) {
                return group
            }
        }
        return defaultGroup
    }()

    static var containerURL: URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: appGroup)
    }

    private static var snapshotURL: URL? {
        containerURL?.appendingPathComponent(snapshotName)
    }

    static var isEnabled: Bool {
        guard let url = snapshotURL else { return false }
        return FileManager.default.fileExists(atPath: url.path)
    }

    private static var keyQuery: [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: keyService,
            kSecAttrAccount as String: keyAccount,
            // An app group name doubles as a keychain access group
            kSecAttrAccessGroup as String: appGroup,
        ]
    }

    // MARK: App side

    /// Encrypts `plaintext` (snapshot JSON) under a fresh key and stores both.
    static func publish(_ plaintext: Data) throws {
        guard let url = snapshotURL else { throw Failure.noContainer }

        let key = SymmetricKey(size: .bits256)
        guard let sealed = try AES.GCM.seal(plaintext, using: key).combined else {
            throw Failure.corrupt
        }
        // Unreadable while the phone is locked, which is also when AutoFill can't run
        try sealed.write(to: url, options: [.atomic, .completeFileProtection])

        var error: Unmanaged<CFError>?
        guard let access = SecAccessControlCreateWithFlags(
            nil,
            kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly,
            .userPresence,
            &error
        ) else { throw Failure.keychain(errSecParam) }

        SecItemDelete(keyQuery as CFDictionary)
        var add = keyQuery
        add[kSecAttrAccessControl as String] = access
        add[kSecValueData as String] = key.withUnsafeBytes { Data($0) }
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else {
            try? FileManager.default.removeItem(at: url)
            // WhenPasscodeSet items can't be created on a phone with no passcode
            throw status == errSecAuthFailed ? Failure.noPasscode : Failure.keychain(status)
        }
    }

    static func disable() {
        SecItemDelete(keyQuery as CFDictionary)
        if let url = snapshotURL {
            try? FileManager.default.removeItem(at: url)
        }
    }

    // MARK: Extension side

    /// Reads the key (which shows the Face ID / passcode prompt) and decrypts
    /// the snapshot. Blocks, so it runs off the main thread; `completion` is
    /// called on the main thread.
    static func load(reason: String, completion: @escaping (Result<AutoFillSnapshot, Error>) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            let result = Result { try loadSync(reason: reason) }
            DispatchQueue.main.async { completion(result) }
        }
    }

    private static func loadSync(reason: String) throws -> AutoFillSnapshot {
        guard let url = snapshotURL else { throw Failure.noContainer }
        guard FileManager.default.fileExists(atPath: url.path) else { throw Failure.notEnabled }

        let context = LAContext()
        context.localizedReason = reason
        var query = keyQuery
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        query[kSecUseAuthenticationContext as String] = context

        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        switch status {
        case errSecSuccess: break
        case errSecUserCanceled: throw Failure.cancelled
        case errSecItemNotFound: throw Failure.notEnabled
        default: throw Failure.keychain(status)
        }
        guard let keyData = item as? Data, keyData.count == 32 else { throw Failure.corrupt }

        do {
            let box = try AES.GCM.SealedBox(combined: try Data(contentsOf: url))
            let plaintext = try AES.GCM.open(box, using: SymmetricKey(data: keyData))
            return try JSONDecoder().decode(AutoFillSnapshot.self, from: plaintext)
        } catch {
            // Most likely the app republished between our file read and key read
            throw Failure.corrupt
        }
    }
}
