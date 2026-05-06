# Vault

A local, end-to-end encrypted password manager built with Tauri 2, React, and Rust.

![Platform](https://img.shields.io/badge/platform-Windows-blue)
![Version](https://img.shields.io/badge/version-1.0.0-green)
![License](https://img.shields.io/badge/license-MIT-blue)

---

## Features

- **AES-256-GCM encryption** — every vault is encrypted at rest with an authenticated cipher; nothing is ever stored in plaintext
- **Argon2id key derivation** — master password is stretched with Argon2id before use, making brute-force attacks expensive
- **Secure key wiping** — the decryption key is zeroed from memory on lock via the `zeroize` crate
- **System tray** — lives in the background; left-click or use the tray menu to open, lock, or quit
- **Quick-access overlay** — press `Ctrl+Shift+P` anywhere to open a floating search window; arrow keys to navigate, `Enter` to copy password, `Tab` to copy email
- **Auto-lock** — configurable inactivity timeout (1 min to 1 hour, or never)
- **Clipboard auto-clear** — copied secrets are wiped from the clipboard after 30 seconds
- **Autostart** — optional launch on system boot
- **Fully local** — no cloud, no sync, no telemetry; vault file lives at `%APPDATA%\vault\vault.enc`

---

## Screenshots

> Coming soon

---

## Security Model

| Layer | Implementation |
|---|---|
| Key derivation | Argon2id — 64 MB memory, 3 iterations |
| Encryption | AES-256-GCM (authenticated) |
| Salt | 32 random bytes, generated once per vault |
| Nonce | 12 random bytes, unique per save |
| Storage | `Base64(nonce ‖ ciphertext)` written to disk |
| Memory | Master key zeroed with `zeroize` on lock |

The vault file cannot be decrypted without the master password. There is no recovery mechanism — if you forget your master password, the data is unrecoverable.

---

## Tech Stack

**Frontend**
- React 19 + TypeScript 5.8
- Vite 7
- Glassmorphism UI — no external component library

**Backend (Rust)**
- Tauri 2
- `aes-gcm` — encryption
- `argon2` — key derivation
- `zeroize` — secure memory wiping
- `uuid` — entry identifiers
- `serde_json` — vault serialization

---

## Entry Fields

| Field | Required |
|---|---|
| Name | Yes |
| Password | Yes |
| Username | No |
| Email | No |
| URL | No |
| Notes | No |

---

## Installation

Download the latest installer from the [Releases](../../releases) page.

| File | Format |
|---|---|
| `Vault_1.0.0_x64-setup.exe` | NSIS installer (recommended) |
| `Vault_1.0.0_x64_en-US.msi` | MSI |

---

## Building from Source

**Prerequisites**
- [Node.js](https://nodejs.org) 18+
- [Rust](https://rustup.rs) stable
- [Tauri prerequisites](https://tauri.app/start/prerequisites/) — WebView2, MSVC build tools

```bash
git clone https://github.com/Wilgoy23/vault.git
cd vault
npm install
npm run tauri build
```

Installers are output to `src-tauri/target/release/bundle/`.

For development hot-reload:

```bash
npm run tauri dev
```

---

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+Shift+P` | Toggle quick-access overlay |
| `↑` / `↓` | Navigate overlay results |
| `Enter` | Copy password |
| `Tab` | Copy email |
| `Escape` | Close overlay | (Bugged - Ctrl+Shift+P to close)

---

## License

MIT
