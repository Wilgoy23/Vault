// C entry points for src-tauri/src/autofill.rs. Compiled into the app target
// only; the Rust static library links against these symbols.

import AuthenticationServices
import Foundation

@_cdecl("vault_autofill_is_supported")
public func vaultAutofillIsSupported() -> Bool {
    AutoFillStore.containerURL != nil
}

@_cdecl("vault_autofill_is_enabled")
public func vaultAutofillIsEnabled() -> Bool {
    AutoFillStore.isEnabled
}

/// 0 ok, 1 no App Group, 2 no device passcode, 3 anything else.
@_cdecl("vault_autofill_publish")
public func vaultAutofillPublish(_ json: UnsafePointer<UInt8>, _ length: Int) -> Int32 {
    let data = Data(bytes: json, count: length)
    do {
        let snapshot = try JSONDecoder().decode(AutoFillSnapshot.self, from: data)
        try AutoFillStore.publish(data)
        replaceIdentities(with: snapshot)
        return 0
    } catch AutoFillStore.Failure.noContainer {
        return 1
    } catch AutoFillStore.Failure.noPasscode {
        return 2
    } catch {
        NSLog("Vault AutoFill: publish failed: %@", String(describing: error))
        return 3
    }
}

@_cdecl("vault_autofill_disable")
public func vaultAutofillDisable() {
    AutoFillStore.disable()
    ASCredentialIdentityStore.shared.removeAllCredentialIdentities(nil)
}

/// Tells iOS which logins exist, so it can suggest them above the keyboard
/// without opening the extension. Usernames and domains only, never secrets.
/// Entries without a URL are left out here but still listed in the extension.
private func replaceIdentities(with snapshot: AutoFillSnapshot) {
    let store = ASCredentialIdentityStore.shared
    store.getState { state in
        // Until Vault is switched on in Settings > AutoFill & Passwords, iOS
        // rejects updates; the next publish after that fills it in.
        guard state.isEnabled else { return }

        var passwords: [ASPasswordCredentialIdentity] = []
        for entry in snapshot.entries {
            guard let host = entry.host, !entry.login.isEmpty else { continue }
            passwords.append(ASPasswordCredentialIdentity(
                serviceIdentifier: ASCredentialServiceIdentifier(identifier: host, type: .domain),
                user: entry.login,
                recordIdentifier: entry.id
            ))
        }

        let done: (Bool, Error?) -> Void = { _, error in
            if let error = error {
                NSLog("Vault AutoFill: identity update failed: %@", String(describing: error))
            }
        }

        if #available(iOS 18.0, *) {
            var identities: [ASCredentialIdentity] = passwords
            for entry in snapshot.entries {
                guard let host = entry.host, entry.totpSecret?.isEmpty == false else { continue }
                identities.append(ASOneTimeCodeCredentialIdentity(
                    serviceIdentifier: ASCredentialServiceIdentifier(identifier: host, type: .domain),
                    label: entry.login.isEmpty ? entry.name : entry.login,
                    recordIdentifier: entry.id
                ))
            }
            store.replaceCredentialIdentities(identities, completion: done)
        } else {
            store.replaceCredentialIdentities(with: passwords, completion: done)
        }
    }
}
