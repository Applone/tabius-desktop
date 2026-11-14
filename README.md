# Tabius Desktop

A desktop SSH client built with Rust, Tauri 2, React, and xterm.js.

- Saved hosts and groups, SSH jump hosts, and optional Mosh support.
- Managed SSH keys, local/remote/SOCKS forwarding, and verified host fingerprints.
- Snippets, Bash/Zsh history import, and OpenSSH configuration import.
- Terminal workspace with draggable tabs and a collapsible snippets, history, and themes panel.
- Two-pane SFTP browser with local/remote navigation and file transfers.
- Configurable shortcuts, themes, and reconnect.
- Encrypted local storage and optional Tabby-Web-compatible configuration sync.

## Build and run

Install Rust through rustup (the toolchain is pinned in `rust-toolchain.toml`),
Bun 1.4.2+, and the native dependencies for your platform. On Arch/CachyOS:

```sh
sudo pacman -S --needed base-devel rust bun openssh webkit2gtk-4.1 gtk3 libayatana-appindicator librsvg
```

Run from the repository root:

```sh
cargo run --locked
cargo build --release --locked
```

Cargo installs frontend dependencies from the frozen Bun lockfile, checks
TypeScript, and embeds the built UI. No frontend development server is required.
The release executable is `target/release/tabius-desktop` (`.exe` on Windows).
OpenSSH 8.5+ is required, including Windows OpenSSH Client. Mosh requires a local
client and remote server.

## Releases

| Platform                            | Packages                       |
| ----------------------------------- | ------------------------------ |
| Linux x64                           | TAR.GZ, DEB, RPM, Arch package |
| macOS 13+ (Apple Silicon and Intel) | ZIP, DMG                       |
| Windows x64                         | ZIP, NSIS installer            |

Linux requires glibc 2.35+, GTK 3, WebKitGTK 4.1, and OpenSSH. Release installation
checks cover Debian 12, Ubuntu 22.04, Fedora 44, and Arch Linux. Portable archives
also need these system dependencies. macOS bundles are ad-hoc signed without
notarization; Windows installers are unsigned and offer WebView2 installation.

Tag pushes publish releases after checks pass. Tags must match the versions in
`Cargo.toml`, `package.json`, and `tauri.conf.json`. See the
[release workflow](.github/workflows/release.yml) and
[packaging tool](scripts/release.py) for the complete process.

## Usage

Double-click a host to connect, or focus it and press Enter/Space. Drag tabs to
reorder sessions. The plus button opens a host picker searchable by name,
address, or group. Deleting a group keeps its hosts in Ungrouped.

The top bar provides **Library**, **SFTP**, and your terminal tabs. Library contains
hosts, keychain, forwarding, snippets, known hosts, and settings; its navigation
is hidden during terminal and SFTP work. Toggle Session tools at the top right
for snippets, imported shell history, and terminal themes. Run sends the selected
command to the active terminal for immediate execution.

Terminal fonts use installed system fonts. The **JetBrains Mono** selection also
recognizes **JetBrainsMono Nerd Font Mono** and **JetBrainsMono Nerd Font**. If a
selected font is unavailable, the terminal and Settings preview use the system
monospace font.

Open **SFTP** to browse this computer on the left and choose a saved host on the
right. SFTP uses OpenSSH key or agent authentication (password and interactive
key-passphrase prompts are not supported; unlock encrypted keys in your SSH
agent first). Unknown fingerprints use the same verification dialog as SSH.
Double-click folders or enter a path, select a file, then Upload or Download.
Transfers never replace existing files. Downloads publish only after completion;
a failed upload may leave a partial remote file, which can be deleted before
retrying. Folder transfers are not recursive. Both panes support new folders,
renaming, deletion of files and empty folders, filtering, and hidden files.
The SFTP connection stays open when switching tabs; use its disconnect button
when finished.

Import hosts and managed keys through **Hosts → Import SSH config**. Review the
preview and warnings before importing; existing hosts and original key files
are preserved. Unsupported options are reported, and hosts requiring
`ProxyCommand` are skipped.

Host chaining copies a saved host's endpoint and preceding hops; select it again
after changing that host. Jump hosts use the SSH agent or default identity files;
managed key assignments apply to the destination. The SSH proxy field accepts
an SSH hop. New server keys open a fingerprint dialog: abort login, continue once,
or trust and continue to save the key in Known Hosts. Temporary trust expires when
the connection ends; reconnects ask again. Jump hosts use the same verification.
Changed keys block connection and cannot be approved through this dialog.

| Action                     | Default shortcut              |
| -------------------------- | ----------------------------- |
| New / close session tab    | Ctrl+Shift+T / Ctrl+Shift+W   |
| Next / previous tab        | Ctrl+Tab / Ctrl+Shift+Tab     |
| Snippets / host library    | Ctrl+Shift+S / Ctrl+Shift+L   |
| Session 1–8 / last session | Ctrl+Shift+1–8 / Ctrl+Shift+9 |

Use Cmd instead of Ctrl for the Shift combinations on macOS, except previous
tab. Change bindings in **Settings → Keyboard shortcuts**; Escape cancels,
Backspace/Delete clears, and Reset shortcuts restores defaults. Shortcuts apply
inside terminals while ordinary forms and dialogs retain their keyboard input.

## Local storage

On first launch, choose a password to encrypt hosts, keys, snippets, history,
settings, and trust records, including existing plaintext data. Automatic unlock
stores the password in Linux Secret Service (GNOME Keyring or compatible
KWallet), Apple Keychain, or Windows Credential Manager. If unavailable, choose
manual unlock on each launch or explicitly continue without encryption.

Storage uses XChaCha20-Poly1305 with a random key wrapped using Argon2id and
HKDF-SHA256. Back up the entire application data directory, including
`local-storage.json`; on Linux it is normally
`~/.local/share/dev.tabius.desktop`. Keep the password: there is no recovery if
both it and the system-secret entry are lost. An unavailable saved password
leaves the workspace locked until manually unlocked.

SSH passwords, key passphrases, and sync credentials are not saved. OpenSSH
operations temporarily decrypt required keys and trust files into private
directories, removed on completion; a crash can leave copies behind. Encryption
protects persistent app files, not an unlocked session or previous plaintext
backups and snapshots. Terminal input is not recorded.

## Configuration sync

In **Settings → Configuration sync**, enter the HTTPS server URL and token,
select a configuration, choose encryption, enter its vault password, and
synchronize. Sync includes hosts, private keys, and fingerprint references;
groups, shortcuts, snippets, forwarding, history, and terminal settings stay local.
Fingerprint references still require explicit trust before connecting.

Modern encryption uses Argon2id, HKDF-SHA256, and XChaCha20-Poly1305. Legacy Tabby
compatibility is available, but existing Tabby clients cannot read modern vaults.
Conflicting edits block sync. Servers without conditional writes can still race
with concurrent updates. See the [sync protocol and encryption specification](docs/sync-encryption.md).

**Settings → Migrate from Tabby-Web** previews and copies a legacy configuration
to a selected destination using modern encryption. Migration replaces that
destination and preserves the source. The [Tabius sync server](https://github.com/Applone/tabius-sync)
stores encrypted configurations without receiving vault passwords.

## Development checks

```sh
bun install --frozen-lockfile
bun run check
bun run test
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
```

Rust code and tests are in `src/`; the UI is in `frontend/`. Packaging and CI
checks are defined in [.github/workflows/build.yml](.github/workflows/build.yml).
The optional native secret-store test requires an unlocked system credential store:

```sh
cargo test --locked --lib local::tests::native_secret_store_roundtrip -- --ignored --exact
```
