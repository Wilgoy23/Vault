// The searchable list shown when the user opens Vault from the key icon.
// Logins for the site being filled come first, under "Suggested".

import AuthenticationServices
import UIKit

final class EntryListController: UITableViewController, UISearchResultsUpdating {
    enum Mode { case passwords, oneTimeCodes }

    var onPick: ((AutoFillEntry) -> Void)?
    var onCancel: (() -> Void)?
    var onRetry: (() -> Void)?

    private var suggested: [AutoFillEntry] = []
    private var others: [AutoFillEntry] = []
    private var query = ""
    private let search = UISearchController(searchResultsController: nil)

    private var sections: [(title: String?, rows: [AutoFillEntry])] {
        let filter: (AutoFillEntry) -> Bool = { [query] entry in
            query.isEmpty
                || entry.name.localizedCaseInsensitiveContains(query)
                || entry.login.localizedCaseInsensitiveContains(query)
                || (entry.url ?? "").localizedCaseInsensitiveContains(query)
        }
        let s = suggested.filter(filter)
        let o = others.filter(filter)
        if s.isEmpty { return [(nil, o)] }
        return [("Suggested", s), ("All", o)]
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        title = "Vault"
        navigationItem.leftBarButtonItem = UIBarButtonItem(
            barButtonSystemItem: .cancel, target: self, action: #selector(cancelTapped))
        search.searchResultsUpdater = self
        search.obscuresBackgroundDuringPresentation = false
        navigationItem.searchController = search
        navigationItem.hidesSearchBarWhenScrolling = false
    }

    func show(entries: [AutoFillEntry], services: [ASCredentialServiceIdentifier], mode: Mode) {
        let byName = entries.sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
        suggested = byName.filter { $0.matches(services) }
        others = byName.filter { !$0.matches(services) }
        tableView.backgroundView = nil
        tableView.reloadData()
        if entries.isEmpty {
            showStatus(mode == .oneTimeCodes ? "No entries have a 2FA secret." : "Your vault is empty.", canRetry: false)
        }
    }

    func showStatus(_ message: String, canRetry: Bool) {
        suggested = []
        others = []
        tableView.reloadData()

        let label = UILabel()
        label.text = message
        label.textAlignment = .center
        label.numberOfLines = 0
        label.textColor = .secondaryLabel

        let stack = UIStackView(arrangedSubviews: [label])
        stack.axis = .vertical
        stack.spacing = 12
        if canRetry {
            let button = UIButton(type: .system)
            button.setTitle("Try Again", for: .normal)
            button.addTarget(self, action: #selector(retryTapped), for: .touchUpInside)
            stack.addArrangedSubview(button)
        }

        let container = UIView()
        container.addSubview(stack)
        stack.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            stack.centerYAnchor.constraint(equalTo: container.centerYAnchor),
            stack.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 32),
            stack.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -32),
        ])
        tableView.backgroundView = container
    }

    @objc private func cancelTapped() { onCancel?() }
    @objc private func retryTapped() { onRetry?() }

    func updateSearchResults(for searchController: UISearchController) {
        query = searchController.searchBar.text ?? ""
        tableView.reloadData()
    }

    // MARK: Table

    override func numberOfSections(in tableView: UITableView) -> Int {
        sections.count
    }

    override func tableView(_ tableView: UITableView, titleForHeaderInSection section: Int) -> String? {
        sections[section].title
    }

    override func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int {
        sections[section].rows.count
    }

    override func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let entry = sections[indexPath.section].rows[indexPath.row]
        // Subtitle style isn't available through register(_:), so build it here
        let cell = tableView.dequeueReusableCell(withIdentifier: "subtitle")
            ?? UITableViewCell(style: .subtitle, reuseIdentifier: "subtitle")
        cell.textLabel?.text = entry.name
        cell.detailTextLabel?.text = [entry.login, entry.host].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " · ")
        cell.detailTextLabel?.textColor = .secondaryLabel
        return cell
    }

    override func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) {
        onPick?(sections[indexPath.section].rows[indexPath.row])
    }
}
