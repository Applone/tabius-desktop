# Tabius Desktop

A Rust application built with Tauri 2. Rust owns workspace defaults, validation and edits, shell-history import, SSH sessions and reconnects, key management, known-host fingerprints, and Tabby-Web synchronization, including YAML, vault encryption and three-way merging. React/TypeScript and xterm.js handle rendering, forms, terminal display and input, and native-command bindings.

## Build and run

Run Cargo from the repository root:

```sh
cargo build --locked
cargo run --locked
cargo build --release --locked
```

Executables are written to `target/debug/tabius-desktop` and `target/release/tabius-desktop`.

The project uses Rust edition 2024 and pins Rust 1.99.1 in
`rust-toolchain.toml`. Install Rust through rustup and Bun 1.4.2 or later.

Cargo's `src/build.rs` installs frontend dependencies from the frozen Bun lockfile, checks TypeScript, bundles the UI with Vite, and embeds the result in the native executable. No separate frontend build command or running development server is needed. Both debug and release builds use embedded assets. An input/output fingerprint avoids rebuilding unchanged UI assets and detects missing or modified `dist/` files.

Bun is still an internal frontend tool, so it must be installed alongside Rust. On Arch/CachyOS, install the native prerequisites with:

```sh
sudo pacman -S --needed base-devel rust bun openssh webkit2gtk-4.1 gtk3 libayatana-appindicator librsvg
```

Linux is the primary development host. Native CI builds cover Linux x64,
macOS 13+ on Apple Silicon and Intel, and Windows x64. The terminal backend uses
Unix PTYs or Windows ConPTY. Windows requires the OpenSSH Client optional
feature; the NSIS installer offers WebView2 installation when needed. Mosh
connections require `mosh` locally and `mosh-server` remotely; Mosh is optional
and is not bundled.

## Layout

```text
Cargo.toml / Cargo.lock   Rust package and locked dependencies
src/                     Rust application logic and tests; build.rs builds the UI
frontend/                React components, terminal UI, and styles
src/fixtures/            Independent legacy and modern vault fixtures
docs/sync-encryption.md  Sync server contract and version 2 encryption format
assets/icons/            Application icon and its SVG source
tauri.conf.json         Native window, assets, and bundle configuration
package.json / bun.lock  Internal frontend tooling and locked dependencies
rust-toolchain.toml      Pinned Rust toolchain and components
.github/                Debug/release workflows and Dependabot configuration
packaging/              nFPM configuration, desktop entry, and pinned tools
scripts/release.py       Native bundles, archives, and checksum verification
```

Tauri core/plugin capabilities are not granted: the frontend currently uses only custom Rust commands and IPC channels. A capabilities directory is not required for these APIs; see [Tauri’s capability documentation](https://v2.tauri.app/security/capabilities/).

Rust tests live beside their code in `src/`, including native PTY and OpenSSH
storage checks. There is no browser or UI end-to-end test tooling.

## Checks and unit tests

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
bun run check
uv tool run ruff==0.17.2 check scripts
uv tool run ruff==0.17.2 format --check scripts
bash -n scripts/test-linux-package.sh
shellcheck scripts/test-linux-package.sh
actionlint
desktop-file-validate packaging/linux/dev.tabius.desktop.desktop
```

The frontend check runs strict TypeScript checking and Biome lint/format checks. Application regression tests, including the migrated workspace and sync suites, run with Cargo. Dependencies are prepared by `cargo build`, or can be installed independently with `bun install --frozen-lockfile`. `bun run dev` serves UI assets for frontend development. Workspace operations require the Rust desktop backend; there is no separate browser workspace or localStorage persistence.

Builds require the locked dependencies to be cached or the package registries to be reachable. With cached dependencies, Cargo's `--offline` option also works. Generated `target/`, `dist/`, `gen/`, and `node_modules/` directories are excluded from version control.

## CI and distribution

The workflows follow the ParakeetX Debug/Release structure. Branch pushes,
pull requests, and manual Debug runs check and build all four targets. Each
job uploads a portable archive and its SHA-256 checksum, retained for 14 days.
Tag pushes run Release builds and publish only after the version, archive
smoke checks, complete asset set, checksums, and Linux installation checks pass.
Tags must be `0.1.0` or `v0.1.0` for the current version; update `Cargo.toml`,
`package.json`, and `tauri.conf.json` together before the next release.

| System              | Release assets                                       |
| ------------------- | ---------------------------------------------------- |
| Linux x64           | Portable TAR.GZ and nFPM DEB, RPM, and Arch packages |
| macOS Apple Silicon | Application ZIP and DMG                              |
| macOS Intel         | Application ZIP and DMG                              |
| Windows x64         | Portable ZIP and Tauri NSIS EXE installer            |

Linux builds run in Ubuntu 22.04 for a glibc 2.35 baseline. Packages use the
system GTK 3 and WebKitGTK 4.1 libraries. The installation matrix covers Debian
12, Ubuntu 22.04, Fedora 44, and Arch Linux. AlmaLinux 9 from the reference
workflow is not a supported runtime because its default WebKitGTK ABI differs
from [Tauri's WebKitGTK 4.1 requirement](https://v2.tauri.app/start/prerequisites/).
Portable Linux archives also require those system libraries and OpenSSH.

macOS bundles use ad-hoc signatures; Apple Developer signing and notarization
are not configured. Windows installers are unsigned. Archive checks execute
`--version` and `--help`; macOS also verifies the bundle signature and DMG.
Linux release jobs install and uninstall each package in an isolated container
and check that user data survives removal. Native Windows and macOS installers
and live SSH connections still require testing on those systems.

To package an existing native build locally, install Python 3.11+ and uv, then
run from the repository root. On Linux, `install-tools` downloads the pinned
nFPM release and verifies its published SHA-256 checksum:

```sh
uv run --no-project python scripts/release.py install-tools
export PATH="${TMPDIR:-/tmp}/tabius-tools:$PATH"
cargo build --release --locked
uv run --no-project python scripts/release.py package \
  --target x86_64-unknown-linux-gnu --build-dir target/release \
  --installers --verify
```

Use `--configuration Debug` without `--installers` for debug archives. CI builds
use explicit target directories; macOS and Windows bundles should likewise be
built with `cargo build --target <triple>` before invoking the script. These
bundles use the Tauri CLI installed by the frozen Bun dependencies. Icons for
Windows and macOS are generated from `assets/icons/icon.svg` with `bun run
tauri icon`; the PNG and SVG remain the Linux icon sources. Generated release
files are written to the ignored `artifacts/` directory.

Dependabot follows ParakeetX's weekly grouped updates, five-PR limit, reviewer,
assignee, and commit prefixes, without dependency ignores. Cargo, Bun, and
GitHub Actions are monitored; the reference's Python-worker entry is replaced
by Bun because the packaging script uses only Python's standard library. The
repository labels are `dependencies`, `rust`, `javascript`, and `ci`.

## Features

- **Hosts:** SSH agent, managed keys or interactive password authentication; port, username, agent forwarding, startup snippet, jump hosts, SSH proxy hop, environment variables, UTF-8 locale, and Mosh. Tags and groups are omitted.
- **Keychain:** generate Ed25519 or RSA-4096 keys with optional passphrase encryption, import OpenSSH/PEM keys, copy public keys, and delete keys. Passphrases are entered through a PTY rather than process arguments. Encrypted manual imports need the matching public key.
- **Port forwarding:** local, remote, and dynamic SOCKS rules. Listeners bind to loopback; remote listeners also depend on the server's GatewayPorts policy. Closing a tunnel tab stops it.
- **Snippets and history:** save scripts, run them in an open session, or use them at startup. Explicitly import Bash/Zsh history and turn a command into a snippet. Terminal input is not recorded. Selecting a session under Run in session executes the script.
- **Known hosts:** inspect/import/remove verified OpenSSH identities. New keys require confirmation and changed keys block connections.
- **Settings:** reconnect, bell, Option as Meta, TERM selection, local fonts, themes, ANSI colors, OS detection, and post-quantum key-exchange compatibility.

Host chaining and the proxy field accept SSH hops, not HTTP/SOCKS proxy commands. Environment values currently exclude whitespace and quotes, and require server AcceptEnv support. Startup snippets and OS detection assume a POSIX-compatible remote shell. Auto reconnect retries SSH exit 255 every three seconds, including authentication errors; close the tab to stop it.

## Import OpenSSH configuration

Choose **Hosts → Import SSH config**, then **Read configuration** to preview
`~/.ssh/config`, or enter another configuration path. Review the hosts, key count,
and warnings, then choose **Import**. Existing hosts are preserved; repeated
imports skip their aliases, including imported hosts you have renamed.

The importer reads explicit `Host` aliases, `HostName`, `User`, `Port`,
`ForwardAgent`, `IdentityFile`, and `ProxyJump`. It applies wildcard and negated
host patterns, first-value defaults, quoted values, comments, and `Include`
globs in lexical order. Relative includes resolve under `~/.ssh`. Jump aliases
are expanded into concrete SSH endpoints and cyclic chains are rejected.
`Match` blocks are skipped without running commands; unsupported options appear
in the preview. Hosts requiring `ProxyCommand` are skipped. System-wide SSH
configuration is not imported.

Referenced private keys, or available default identities when no `IdentityFile`
is specified, are copied into the managed Keychain and deduplicated by public
fingerprint. The originals remain untouched. Encrypted OpenSSH keys retain their
encryption and do not require a password to migrate. Other encrypted formats
require their matching `.pub` file. Keys are eligible for the existing encrypted
configuration sync; private material stays out of workspace metadata and the
webview. If multiple keys apply, all are migrated and the first is assigned to
the host. Jump-host authentication uses the SSH agent or default identity files;
managed key assignments apply to the destination host.

## Configuration sync

In **Settings → Configuration sync**, enter an HTTPS server URL and synchronization token, load configurations, select one, choose encryption, and enter its vault password. Then choose **Synchronize**. Tokens and vault passwords stay in memory; only the destination, encryption mode, last-sync date, and merge baselines are persisted.

The protocol matches the iOS project: Bearer-authenticated `GET /api/1/configs`, `GET /api/1/configs/{id}`, and `PATCH /api/1/configs/{id}`. Redirects are rejected. Three-way merging propagates deletions and independent edits, and blocks conflicting edits before upload. A second read checks for concurrent changes; If-Match is sent when an ETag is available. Servers without conditional writes can still race between that read and PATCH.

Sync includes host names/endpoints/users/key assignments, private keys, and fingerprint references. It preserves unrelated profiles, templates, options, vault files, and password secrets. Key baselines contain hashes rather than private-key material.

**Modern encryption (recommended)** is the default for new connections: Argon2id v19 (64 MiB, three passes, four lanes), HKDF-SHA256 with a vault-specific context, and XChaCha20-Poly1305 authenticated encryption. It encrypts the whole configuration and creates a vault if needed. Selecting modern mode upgrades plaintext or legacy vaults on the next successful sync. Version 2 requires compatible clients; existing Tabby clients cannot decrypt it. Modern vaults cannot be downgraded through compatibility mode, and a previously modern destination returning legacy or plaintext content blocks sync.

**Tabby compatibility (legacy, not recommended)** retains [Tabby's version 1 format](https://github.com/Eugeny/tabby/wiki/Vault-Encryption): AES-256-CBC with PBKDF2-HMAC-SHA512, 100,000 iterations, an eight-byte salt, and a 16-byte IV. Existing saved connections without a mode retain compatibility behavior. Both whole-configuration and secrets-only legacy vaults remain supported; private keys require an existing encrypted vault in this mode.

The [server contract and encryption specification](docs/sync-encryption.md) describe how a lightweight server can store opaque configuration content without holding encryption keys. Tests cover an independent libsodium-based version 2 fixture and the existing iOS version 1 compatibility fixture.

Advanced SSH options, snippets, forwarding rules, history, and terminal settings remain local. Imported jump-host references resolve to SSH chains; missing references and cycles block import. Unsupported proxy profiles block sync. Saved passwords remain in the remote vault and are entered interactively here.

Synced fingerprints are references, not automatically trusted keys. They appear in Known hosts and before the SSH prompt for comparison. Changes or deletions revoke matching previous local trust entries after successful upload; accepting a new key still requires confirmation.

## Storage and transport

Rust runs the installed OpenSSH client in a controlling PTY and streams bytes to xterm.js over Tauri IPC. Session resizes reach the PTY; closing sessions or exiting the app terminates Unix process groups or the Windows process tree. The main connection ignores the user's SSH config so UI preferences are deterministic. Without a selected managed key, OpenSSH can use its agent and default identity paths. Jump hosts may use the system trust file.

Application data is stored in Tauri's data directory for `dev.tabius.desktop`, normally `~/.local/share/dev.tabius.desktop` on Linux:

- `workspace.json`: hosts, preferences, public key metadata, scripts, imported history, and sync state; atomically written with mode `0600`.
- `keys/<uuid>`: private keys with mode `0600`, inside a `0700` directory. Encrypted keys retain their encryption; this is not an encrypted application vault.
- `known_hosts`: application-specific OpenSSH trust entries.

On Windows, the application directory and keys directory use a protected DACL
granting full access only to the current user's SID; new files inherit that
restriction. Unix files continue to use `0600` and directories `0700`.

Passwords and session passphrases are not saved. Scripts, environment variables, and imported history are plaintext workspace data; review history for secrets. The webview does not persist a separate browser workspace.

Live SSH sessions, a live Tabby-Web account, release packaging, and non-Linux platforms require separate runtime validation; unit tests do not establish those claims.
