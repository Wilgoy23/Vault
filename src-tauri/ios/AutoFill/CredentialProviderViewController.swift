// The AutoFill extension's entry point. iOS calls one of the prepare/provide
// methods below depending on how the user got here:
//
// - tapped a suggestion above the keyboard: provideCredentialWithoutUserInteraction,
//   which always asks for UI (the key sits behind Face ID), then
//   prepareInterfaceToProvideCredential fills that entry straight after unlocking.
// - tapped the key icon / "Passwords": prepareCredentialList, which shows a
//   searchable list with this site's logins first.
// - a one-time code field (iOS 18+): the same two paths, filling a TOTP code.

import AuthenticationServices
import UIKit

final class CredentialProviderViewController: ASCredentialProviderViewController {
    private var mode = EntryListController.Mode.passwords
    private var services: [ASCredentialServiceIdentifier] = []
    private let list = EntryListController()

    override func viewDidLoad() {
        super.viewDidLoad()
        let nav = UINavigationController(rootViewController: list)
        addChild(nav)
        nav.view.frame = view.bounds
        nav.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(nav.view)
        nav.didMove(toParent: self)

        list.onCancel = { [weak self] in self?.cancel(.userCanceled) }
        list.onPick = { [weak self] entry in self?.fill(entry) }
        list.onRetry = { [weak self] in self?.showList() }
    }

    // MARK: List

    override func prepareCredentialList(for serviceIdentifiers: [ASCredentialServiceIdentifier]) {
        services = serviceIdentifiers
        mode = .passwords
        showList()
    }

    @available(iOS 18.0, *)
    override func prepareOneTimeCodeCredentialList(for serviceIdentifiers: [ASCredentialServiceIdentifier]) {
        services = serviceIdentifiers
        mode = .oneTimeCodes
        showList()
    }

    private func showList() {
        list.showStatus("Unlocking…", canRetry: false)
        AutoFillStore.load(reason: "Unlock Vault to fill in") { [weak self] result in
            guard let self = self else { return }
            switch result {
            case .success(let snapshot):
                var entries = snapshot.entries
                if self.mode == .oneTimeCodes {
                    entries = entries.filter { $0.totpSecret?.isEmpty == false }
                }
                self.list.show(entries: entries, services: self.services, mode: self.mode)
            case .failure(let error):
                self.report(error)
            }
        }
    }

    // MARK: Suggestion tapped

    override func provideCredentialWithoutUserInteraction(for credentialIdentity: ASPasswordCredentialIdentity) {
        cancel(.userInteractionRequired)
    }

    @available(iOS 17.0, *)
    override func provideCredentialWithoutUserInteraction(for credentialRequest: ASCredentialRequest) {
        cancel(.userInteractionRequired)
    }

    override func prepareInterfaceToProvideCredential(for credentialIdentity: ASPasswordCredentialIdentity) {
        fillRecord(credentialIdentity.recordIdentifier, mode: .passwords)
    }

    @available(iOS 17.0, *)
    override func prepareInterfaceToProvideCredential(for credentialRequest: ASCredentialRequest) {
        var mode = EntryListController.Mode.passwords
        if #available(iOS 18.0, *), credentialRequest.type == .oneTimeCode {
            mode = .oneTimeCodes
        }
        fillRecord(credentialRequest.credentialIdentity.recordIdentifier, mode: mode)
    }

    private func fillRecord(_ id: String?, mode: EntryListController.Mode) {
        self.mode = mode
        list.showStatus("Unlocking…", canRetry: false)
        AutoFillStore.load(reason: "Unlock Vault to fill in") { [weak self] result in
            guard let self = self else { return }
            switch result {
            case .success(let snapshot):
                if let entry = snapshot.entries.first(where: { $0.id == id }) {
                    self.fill(entry)
                } else {
                    // Deleted since iOS last heard from the app
                    self.cancel(.credentialIdentityNotFound)
                }
            case .failure(let error):
                self.report(error)
            }
        }
    }

    // MARK: Finishing

    private func fill(_ entry: AutoFillEntry) {
        switch mode {
        case .passwords:
            let credential = ASPasswordCredential(user: entry.login, password: entry.password)
            extensionContext.completeRequest(withSelectedCredential: credential, completionHandler: nil)
        case .oneTimeCodes:
            if #available(iOS 18.0, *), let code = entry.totpSecret.flatMap({ TOTP.code(secret: $0) }) {
                extensionContext.completeOneTimeCodeRequest(
                    using: ASOneTimeCodeCredential(code: code),
                    completionHandler: nil
                )
            } else {
                cancel(.failed)
            }
        }
    }

    private func report(_ error: Error) {
        switch error {
        case AutoFillStore.Failure.cancelled:
            cancel(.userCanceled)
        case AutoFillStore.Failure.notEnabled:
            list.showStatus("Open Vault and turn on Password AutoFill in Settings.", canRetry: false)
        case AutoFillStore.Failure.corrupt:
            list.showStatus("Vault's AutoFill data is out of date. Open Vault to refresh it.", canRetry: true)
        default:
            list.showStatus("Couldn't unlock Vault.", canRetry: true)
        }
    }

    private func cancel(_ code: ASExtensionError.Code) {
        extensionContext.cancelRequest(withError: ASExtensionError(code))
    }
}
