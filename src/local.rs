//! Local encryption is independent of the remote sync vault.
use crate::{Result, storage, vault};
use base64::{Engine, engine::general_purpose::STANDARD};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::Manager;
use zeroize::{Zeroize, Zeroizing};

const CONFIG: &str = "local-storage.json";
const MAGIC: &[u8] = b"tabius-local-v1\0";
const LIMIT: u64 = 16 * 1024 * 1024;

#[derive(Default)]
pub struct LocalStorage(pub Mutex<Option<Store>>, Mutex<Option<fs::File>>);

pub struct Store {
    pub directory: PathBuf,
    key: Option<Zeroizing<[u8; 32]>>,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    System,
    Manual,
    Plain,
}

#[derive(Deserialize, Serialize)]
struct Config {
    version: u8,
    mode: Mode,
    directory: String,
    wrapped_key: Option<Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    state: &'static str,
    keyring_unavailable: bool,
}
impl Status {
    fn new(state: &'static str) -> Self {
        Self {
            state,
            keyring_unavailable: false,
        }
    }
}

fn root(app: &tauri::AppHandle) -> Result<PathBuf> {
    let root = app.path().app_data_dir().map_err(|e| e.to_string())?;
    storage::directory(&root)?;
    let state = app.state::<LocalStorage>();
    let mut lock = state.1.lock().map_err(|e| e.to_string())?;
    if lock.is_none() {
        let file = storage::options()
            .create(true)
            .truncate(false)
            .open(root.join(".storage.lock"))
            .map_err(|e| e.to_string())?;
        file.try_lock().map_err(|_| {
            "Another Tabius instance is using this local storage. Close it and retry.".to_string()
        })?;
        *lock = Some(file);
    }
    Ok(root)
}

fn entry(root: &Path) -> Result<keyring::Entry> {
    let account = vault::hex(&Sha256::digest(root.to_string_lossy().as_bytes()));
    keyring::Entry::new("dev.tabius.desktop.local-storage", &account)
        .map_err(|_| "System secret storage is unavailable.".into())
}

trait PasswordStore {
    fn get(&self) -> Result<Zeroizing<String>>;
    fn set(&self, password: &str) -> Result<()>;
    fn clear(&self);
}
struct SystemSecrets<'a>(&'a Path);
impl PasswordStore for SystemSecrets<'_> {
    fn get(&self) -> Result<Zeroizing<String>> {
        entry(self.0)?
            .get_password()
            .map(Zeroizing::new)
            .map_err(|_| "System secret storage is unavailable.".into())
    }
    fn set(&self, password: &str) -> Result<()> {
        entry(self.0)?
            .set_password(password)
            .map_err(|_| "System secret storage is unavailable.".into())
    }
    fn clear(&self) {
        if let Ok(entry) = entry(self.0) {
            let _ = entry.delete_credential();
        }
    }
}

fn initialize(
    root: &Path,
    unlocked: &mut Option<Store>,
    secrets: &impl PasswordStore,
) -> Result<Status> {
    if unlocked.is_some() {
        return Ok(Status::new("ready"));
    }
    let Some(c) = config(root)? else {
        return Ok(Status::new("setup"));
    };
    if c.mode == Mode::Plain {
        *unlocked = Some(open(root, &c, "")?);
        return Ok(Status::new("ready"));
    }
    if c.mode == Mode::System {
        if let Ok(password) = secrets.get()
            && let Ok(store) = open(root, &c, &password)
        {
            cleanup_legacy(root)?;
            *unlocked = Some(store);
            return Ok(Status::new("ready"));
        }
        return Ok(Status {
            state: "locked",
            keyring_unavailable: true,
        });
    }
    Ok(Status::new("locked"))
}

fn setup(
    root: &Path,
    unlocked: &mut Option<Store>,
    mode: Mode,
    password: &str,
    secrets: &impl PasswordStore,
) -> Result<Status> {
    if config(root)?.is_some() {
        return Err("Local storage has already been configured.".into());
    }
    if mode != Mode::Plain && (password.is_empty() || password.len() > 1024) {
        return Err("Enter a password of 1–1024 bytes.".into());
    }
    if mode == Mode::System {
        let saved = secrets.set(password).and_then(|()| secrets.get());
        if !saved.is_ok_and(|check| check.as_str() == password) {
            secrets.clear();
            return Ok(Status {
                state: "setup",
                keyring_unavailable: true,
            });
        }
    }
    *unlocked = Some(configure(root, mode, password)?);
    Ok(Status::new("ready"))
}

fn config(root: &Path) -> Result<Option<Config>> {
    if !root.join(CONFIG).exists() {
        if legacy_files(root)?.is_empty() {
            for item in fs::read_dir(root).map_err(|e| e.to_string())? {
                let item = item.map_err(|e| e.to_string())?;
                if uuid::Uuid::parse_str(&item.file_name().to_string_lossy()).is_ok()
                    && ["workspace.json", "known_hosts", "keys"]
                        .iter()
                        .any(|name| item.path().join(name).exists())
                {
                    return Err("Encrypted local data exists, but its configuration is missing. Restore local-storage.json from your backup.".into());
                }
            }
        }
        return Ok(None);
    }
    let bytes = bounded_read(&root.join(CONFIG))?;
    let c: Config =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid local storage configuration.")?;
    if c.version != 1 || (c.directory != "." && uuid::Uuid::parse_str(&c.directory).is_err()) {
        return Err("Unsupported local storage configuration.".into());
    }
    if (c.mode == Mode::Plain) != c.wrapped_key.is_none() {
        return Err("Invalid local storage encryption configuration.".into());
    }
    if c.mode != Mode::Plain
        && (c.directory == "." || c.wrapped_key.as_ref().is_none_or(|key| key["version"] != 2))
    {
        return Err("Invalid local storage encryption configuration.".into());
    }
    Ok(Some(c))
}

fn bounded_read(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("Local storage file exceeds 16 MB.".into());
    }
    Ok(bytes)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path.parent().ok_or("Invalid storage path.")?;
    storage::directory(parent)?;
    let mut file = tempfile::Builder::new()
        .prefix(".tabius-")
        .tempfile_in(parent)
        .map_err(|e| e.to_string())?;
    // tempfile is private on Unix; the directory supplies its restricted Windows ACL.
    file.write_all(bytes)
        .and_then(|()| file.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.error.to_string())?;
    #[cfg(unix)]
    fs::File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(())
}

impl Store {
    fn stage_session(&self, scratch: &Path, key_id: &str) -> Result<String> {
        if !key_id.is_empty() {
            crate::ssh::valid_id(key_id)?;
            let key = self.read(&self.directory.join("keys").join(key_id))?;
            atomic_write(&scratch.join("keys").join(key_id), &key)?;
        }
        let path = self.directory.join("known_hosts");
        let known = if path.exists() {
            String::from_utf8(self.read(&path)?.to_vec())
                .map_err(|_| "Invalid known-host storage.")?
        } else {
            String::new()
        };
        atomic_write(&scratch.join("known_hosts"), known.as_bytes())?;
        Ok(known)
    }

    fn aad(&self, path: &Path) -> Result<Vec<u8>> {
        let relative = path
            .strip_prefix(&self.directory)
            .map_err(|_| "Invalid storage path.")?;
        if relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err("Invalid storage path.".into());
        }
        Ok([
            MAGIC,
            relative.to_string_lossy().replace('\\', "/").as_bytes(),
        ]
        .concat())
    }

    pub fn read(&self, path: &Path) -> Result<Zeroizing<Vec<u8>>> {
        let aad = self.aad(path)?;
        let bytes = Zeroizing::new(bounded_read(path)?);
        let Some(key) = &self.key else {
            return Ok(bytes);
        };
        let data = bytes
            .strip_prefix(MAGIC)
            .ok_or("Local storage is not encrypted or is damaged.")?;
        if data.len() < 24 + 16 {
            return Err("Truncated local storage file.".into());
        }
        XChaCha20Poly1305::new_from_slice(key.as_ref())
            .map_err(|_| "Invalid local storage key.")?
            .decrypt(
                &XNonce::try_from(&data[..24]).map_err(|_| "Invalid local nonce.")?,
                Payload {
                    msg: &data[24..],
                    aad: &aad,
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| {
                "Local storage authentication failed. The file is damaged or was replaced.".into()
            })
    }

    pub fn write(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        let aad = self.aad(path)?;
        if bytes.len() as u64 > LIMIT - 128 {
            return Err("Local storage file exceeds its size limit.".into());
        }
        let Some(key) = &self.key else {
            return atomic_write(path, bytes);
        };
        let mut nonce = [0; 24];
        getrandom::fill(&mut nonce).map_err(|_| "Cannot generate local storage randomness.")?;
        let encrypted = XChaCha20Poly1305::new_from_slice(key.as_ref())
            .map_err(|_| "Invalid local storage key.")?
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: bytes,
                    aad: &aad,
                },
            )
            .map_err(|_| "Cannot encrypt local storage.".to_string())?;
        atomic_write(path, &[MAGIC, &nonce, &encrypted].concat())
    }
}

fn open(root: &Path, c: &Config, password: &str) -> Result<Store> {
    let key = if let Some(wrapped) = &c.wrapped_key {
        let mut value = vault::decrypt(wrapped, password)?;
        let Value::String(encoded) = value["key"].take() else {
            return Err("Invalid local storage key.".into());
        };
        let encoded = Zeroizing::new(encoded);
        let decoded = Zeroizing::new(
            STANDARD
                .decode(encoded.as_bytes())
                .map_err(|_| "Invalid local storage key.")?,
        );
        Some(Zeroizing::new(
            decoded
                .as_slice()
                .try_into()
                .map_err(|_| "Invalid local storage key length.")?,
        ))
    } else {
        None
    };
    let directory = root.join(&c.directory);
    if !directory.is_dir() {
        return Err("Local storage directory is missing.".into());
    }
    Ok(Store { directory, key })
}

pub fn directory(app: &tauri::AppHandle) -> Result<PathBuf> {
    let state = app.state::<LocalStorage>();
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let store = guard.as_ref().ok_or("Local storage is locked.")?;
    storage::directory(&store.directory.join("keys"))?;
    Ok(store.directory.clone())
}
pub fn read(app: &tauri::AppHandle, path: &Path) -> Result<Zeroizing<Vec<u8>>> {
    app.state::<LocalStorage>()
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .ok_or("Local storage is locked.")?
        .read(path)
}
pub fn text(app: &tauri::AppHandle, path: &Path) -> Result<String> {
    String::from_utf8(read(app, path)?.to_vec())
        .map_err(|_| "Local storage is not valid UTF-8.".into())
}
pub fn write(app: &tauri::AppHandle, path: &Path, bytes: &[u8]) -> Result<()> {
    app.state::<LocalStorage>()
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .ok_or("Local storage is locked.")?
        .write(path, bytes)
}
pub fn stage_session(app: &tauri::AppHandle, scratch: &Path, key_id: &str) -> Result<String> {
    app.state::<LocalStorage>()
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .ok_or("Local storage is locked.")?
        .stage_session(scratch, key_id)
}

#[tauri::command]
pub async fn storage_status(app: tauri::AppHandle) -> Result<Status> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = root(&app)?;
        let state = app.state::<LocalStorage>();
        let mut guard = state.0.lock().map_err(|e| e.to_string())?;
        initialize(&root, &mut guard, &SystemSecrets(&root))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn unlock_storage(app: tauri::AppHandle, password: String) -> Result<()> {
    let password = Zeroizing::new(password);
    tauri::async_runtime::spawn_blocking(move || {
        let root = root(&app)?;
        let c = config(&root)?.ok_or("Set up local storage first.")?;
        let state = app.state::<LocalStorage>();
        let mut guard = state.0.lock().map_err(|e| e.to_string())?;
        if guard.is_some() {
            return Ok(());
        }
        let store = open(&root, &c, &password)?;
        if c.mode != Mode::Plain {
            cleanup_legacy(&root)?;
        }
        *guard = Some(store);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn legacy_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for name in ["workspace.json", "known_hosts"] {
        if root.join(name).exists() {
            files.push(PathBuf::from(name));
        }
    }
    if root.join("keys").is_dir() {
        for item in fs::read_dir(root.join("keys")).map_err(|e| e.to_string())? {
            let item = item.map_err(|e| e.to_string())?;
            if !item.file_type().map_err(|e| e.to_string())?.is_file() {
                return Err("Unexpected entry in the key directory.".into());
            }
            files.push(PathBuf::from("keys").join(item.file_name()));
        }
    }
    Ok(files)
}

fn configure(root: &Path, mode: Mode, password: &str) -> Result<Store> {
    if config(root)?.is_some() {
        return Err("Local storage has already been configured.".into());
    }
    if mode == Mode::Plain {
        let c = Config {
            version: 1,
            mode,
            directory: ".".into(),
            wrapped_key: None,
        };
        atomic_write(
            &root.join(CONFIG),
            &serde_json::to_vec(&c).map_err(|e| e.to_string())?,
        )?;
        return open(root, &c, "");
    }
    if password.is_empty() || password.len() > 1024 {
        return Err("Enter a password of 1–1024 bytes.".into());
    }
    let mut key = Zeroizing::new([0; 32]);
    getrandom::fill(key.as_mut()).map_err(|_| "Cannot generate local storage key.")?;
    let mut contents = json!({"key": STANDARD.encode(key.as_ref())});
    let wrapped_key = vault::encrypt(&contents, password, vault::Encryption::Modern);
    if let Value::String(encoded) = &mut contents["key"] {
        encoded.zeroize();
    }
    let wrapped_key = wrapped_key?;
    let c = Config {
        version: 1,
        mode,
        directory: uuid::Uuid::new_v4().to_string(),
        wrapped_key: Some(wrapped_key),
    };
    let store = Store {
        directory: root.join(&c.directory),
        key: Some(key),
    };
    storage::directory(&store.directory)?;
    let files = legacy_files(root)?;
    // Commit the configuration only after every encrypted copy is durable.
    for relative in &files {
        let bytes = Zeroizing::new(bounded_read(&root.join(relative))?);
        store.write(&store.directory.join(relative), &bytes)?;
        if store.read(&store.directory.join(relative))?.as_slice() != bytes.as_slice() {
            return Err("Local storage migration verification failed.".into());
        }
    }
    atomic_write(
        &root.join(CONFIG),
        &serde_json::to_vec(&c).map_err(|e| e.to_string())?,
    )?;
    cleanup_legacy(root)?;
    Ok(store)
}

fn cleanup_legacy(root: &Path) -> Result<()> {
    for relative in legacy_files(root)? {
        fs::remove_file(root.join(relative)).map_err(|_| "Encrypted storage was saved, but old plaintext files could not be removed. Fix file permissions and restart.".to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn configure_storage(
    app: tauri::AppHandle,
    mode: Mode,
    password: String,
) -> Result<Status> {
    let password = Zeroizing::new(password);
    tauri::async_runtime::spawn_blocking(move || {
        let root = root(&app)?;
        let state = app.state::<LocalStorage>();
        let mut guard = state.0.lock().map_err(|e| e.to_string())?;
        setup(&root, &mut guard, mode, &password, &SystemSecrets(&root))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// OpenSSH needs ordinary files; never place these copies in persistent app data.
pub fn scratch() -> Result<tempfile::TempDir> {
    let mut builder = tempfile::Builder::new();
    builder.prefix("tabius-session-");
    // Prefer the per-user runtime filesystem on Linux (normally tmpfs).
    #[cfg(target_os = "linux")]
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| {
            use std::os::unix::fs::PermissionsExt;
            fs::metadata(path).is_ok_and(|m| m.is_dir() && m.permissions().mode() & 0o077 == 0)
        });
    #[cfg(not(target_os = "linux"))]
    let runtime: Option<PathBuf> = None;
    let temp = runtime
        .and_then(|path| builder.tempdir_in(path).ok())
        .map_or_else(|| builder.tempdir(), Ok)
        .map_err(|e| e.to_string())?;
    storage::directory(temp.path())?;
    storage::directory(&temp.path().join("keys"))?;
    Ok(temp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Secrets {
        password: RefCell<Option<String>>,
        unavailable: bool,
        unreadable: bool,
    }
    impl PasswordStore for Secrets {
        fn get(&self) -> Result<Zeroizing<String>> {
            if self.unavailable || self.unreadable {
                return Err("Unavailable".into());
            }
            self.password
                .borrow()
                .clone()
                .map(Zeroizing::new)
                .ok_or("No entry".into())
        }
        fn set(&self, password: &str) -> Result<()> {
            if self.unavailable {
                return Err("Unavailable".into());
            }
            *self.password.borrow_mut() = Some(password.into());
            Ok(())
        }
        fn clear(&self) {
            *self.password.borrow_mut() = None;
        }
    }

    #[test]
    fn system_password_is_verified_and_unlocks_after_restart() {
        let root = tempfile::tempdir().unwrap();
        let secrets = Secrets::default();
        let mut state = None;
        assert_eq!(
            initialize(root.path(), &mut state, &secrets).unwrap().state,
            "setup"
        );
        assert_eq!(
            setup(
                root.path(),
                &mut state,
                Mode::System,
                "local-password",
                &secrets
            )
            .unwrap()
            .state,
            "ready"
        );
        let store = state.take().unwrap();
        store
            .write(
                &store.directory.join("workspace.json"),
                b"private workspace",
            )
            .unwrap();
        drop(store);
        assert_eq!(
            initialize(root.path(), &mut state, &secrets).unwrap().state,
            "ready"
        );
        let store = state.unwrap();
        assert_eq!(
            store
                .read(&store.directory.join("workspace.json"))
                .unwrap()
                .as_slice(),
            b"private workspace"
        );
        assert!(
            !fs::read_to_string(root.path().join(CONFIG))
                .unwrap()
                .contains("local-password")
        );
    }

    #[test]
    fn unavailable_or_unreadable_keyring_leaves_setup_and_existing_files_untouched() {
        for secrets in [
            Secrets {
                unavailable: true,
                ..Secrets::default()
            },
            Secrets {
                unreadable: true,
                ..Secrets::default()
            },
        ] {
            let root = tempfile::tempdir().unwrap();
            fs::write(root.path().join("workspace.json"), b"original").unwrap();
            let mut state = None;
            let result =
                setup(root.path(), &mut state, Mode::System, "password", &secrets).unwrap();
            assert_eq!(result.state, "setup");
            assert!(result.keyring_unavailable);
            assert!(state.is_none());
            assert!(secrets.password.borrow().is_none());
            assert!(!root.path().join(CONFIG).exists());
            assert_eq!(
                fs::read(root.path().join("workspace.json")).unwrap(),
                b"original"
            );
        }
    }

    #[test]
    fn lost_system_password_requires_manual_unlock_without_downgrading() {
        let root = tempfile::tempdir().unwrap();
        let mut state = None;
        setup(
            root.path(),
            &mut state,
            Mode::System,
            "password",
            &Secrets::default(),
        )
        .unwrap();
        state = None;
        let before = fs::read(root.path().join(CONFIG)).unwrap();
        let status = initialize(root.path(), &mut state, &Secrets::default()).unwrap();
        assert_eq!(status.state, "locked");
        assert!(status.keyring_unavailable);
        assert!(state.is_none());
        let c = config(root.path()).unwrap().unwrap();
        assert!(open(root.path(), &c, "wrong").is_err());
        assert!(open(root.path(), &c, "password").is_ok());
        assert!(
            setup(
                root.path(),
                &mut state,
                Mode::Plain,
                "",
                &Secrets::default()
            )
            .is_err()
        );
        assert_eq!(fs::read(root.path().join(CONFIG)).unwrap(), before);
    }

    #[test]
    fn manual_mode_prompts_after_each_restart_and_never_uses_keyring() {
        let root = tempfile::tempdir().unwrap();
        let secrets = Secrets {
            unavailable: true,
            ..Secrets::default()
        };
        let mut state = None;
        setup(root.path(), &mut state, Mode::Manual, "password", &secrets).unwrap();
        state = None;
        let status = initialize(root.path(), &mut state, &secrets).unwrap();
        assert_eq!(status.state, "locked");
        assert!(!status.keyring_unavailable);
        assert!(secrets.password.borrow().is_none());
    }

    #[test]
    fn explicit_plain_mode_persists_without_a_password_or_keyring() {
        let root = tempfile::tempdir().unwrap();
        let secrets = Secrets {
            unavailable: true,
            ..Secrets::default()
        };
        let mut state = None;
        setup(root.path(), &mut state, Mode::Plain, "", &secrets).unwrap();
        let store = state.take().unwrap();
        store
            .write(&store.directory.join("workspace.json"), b"plain")
            .unwrap();
        assert_eq!(
            fs::read(root.path().join("workspace.json")).unwrap(),
            b"plain"
        );
        assert_eq!(
            initialize(root.path(), &mut state, &secrets).unwrap().state,
            "ready"
        );
    }

    #[test]
    fn migration_encrypts_all_data_then_removes_the_originals() {
        let root = tempfile::tempdir().unwrap();
        storage::directory(&root.path().join("keys")).unwrap();
        let originals = [
            ("workspace.json", "workspace secret"),
            ("keys/key", "private key secret"),
            ("keys/key.pub", "public key"),
            ("known_hosts", "trusted host"),
        ];
        for (path, text) in originals {
            fs::write(root.path().join(path), text).unwrap();
        }
        let store = configure(root.path(), Mode::Manual, "password").unwrap();
        let c = config(root.path()).unwrap().unwrap();
        let reopened = open(root.path(), &c, "password").unwrap();
        for (path, text) in originals {
            assert!(!root.path().join(path).exists());
            let bytes = fs::read(store.directory.join(path)).unwrap();
            assert!(
                !bytes
                    .windows(text.len())
                    .any(|part| part == text.as_bytes())
            );
            assert_eq!(
                reopened
                    .read(&reopened.directory.join(path))
                    .unwrap()
                    .as_slice(),
                text.as_bytes()
            );
        }
        assert!(root.path().join(CONFIG).is_file());
    }

    #[test]
    fn failed_migration_keeps_originals_and_does_not_commit_configuration() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("workspace.json"), b"original workspace").unwrap();
        storage::directory(&root.path().join("keys/unexpected-directory")).unwrap();
        assert!(configure(root.path(), Mode::Manual, "password").is_err());
        assert!(!root.path().join(CONFIG).exists());
        assert_eq!(
            fs::read(root.path().join("workspace.json")).unwrap(),
            b"original workspace"
        );
    }

    #[test]
    fn encryption_randomizes_writes_and_rejects_tampering_plaintext_and_swapped_files() {
        let root = tempfile::tempdir().unwrap();
        let store = Store {
            directory: root.path().into(),
            key: Some(Zeroizing::new([42; 32])),
        };
        let path = root.path().join("workspace.json");
        store.write(&path, b"secret").unwrap();
        let first = fs::read(&path).unwrap();
        store.write(&path, b"secret").unwrap();
        assert_ne!(first, fs::read(&path).unwrap());
        let other = root.path().join("known_hosts");
        fs::write(&other, &first).unwrap();
        assert!(store.read(&other).is_err());
        for index in [0, MAGIC.len(), first.len() - 1] {
            let mut bad = first.clone();
            bad[index] ^= 1;
            fs::write(&path, bad).unwrap();
            assert!(store.read(&path).is_err());
        }
        for bytes in [&first[..MAGIC.len() + 10], b"plaintext"] {
            fs::write(&path, bytes).unwrap();
            assert!(store.read(&path).is_err());
        }
        assert!(
            store
                .write(&root.path().join("../escape"), b"secret")
                .is_err()
        );
    }

    #[test]
    fn malformed_configuration_is_never_treated_as_first_launch() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(CONFIG), b"broken").unwrap();
        assert!(initialize(root.path(), &mut None, &Secrets::default()).is_err());
        assert!(configure(root.path(), Mode::Plain, "").is_err());
        for directory in ["../escape", "/tmp", "keys"] {
            let c = Config {
                version: 1,
                mode: Mode::Plain,
                directory: directory.into(),
                wrapped_key: None,
            };
            fs::write(root.path().join(CONFIG), serde_json::to_vec(&c).unwrap()).unwrap();
            assert!(config(root.path()).is_err());
        }
    }

    #[test]
    fn missing_configuration_for_existing_encrypted_data_is_not_a_new_workspace() {
        let root = tempfile::tempdir().unwrap();
        let store = configure(root.path(), Mode::Manual, "password").unwrap();
        store
            .write(&store.directory.join("workspace.json"), b"saved data")
            .unwrap();
        fs::remove_file(root.path().join(CONFIG)).unwrap();
        assert!(initialize(root.path(), &mut None, &Secrets::default()).is_err());
        assert!(configure(root.path(), Mode::Plain, "").is_err());
        assert!(store.directory.join("workspace.json").exists());
    }

    #[test]
    fn restart_retries_plaintext_cleanup_after_migration_commit() {
        let root = tempfile::tempdir().unwrap();
        let secrets = Secrets::default();
        fs::write(root.path().join("workspace.json"), b"workspace").unwrap();
        let mut state = None;
        setup(root.path(), &mut state, Mode::System, "password", &secrets).unwrap();
        // Model interruption between configuration commit and original-file removal.
        fs::write(root.path().join("workspace.json"), b"workspace").unwrap();
        state = None;
        assert_eq!(
            initialize(root.path(), &mut state, &secrets).unwrap().state,
            "ready"
        );
        assert!(!root.path().join("workspace.json").exists());
        let store = state.unwrap();
        assert_eq!(
            store
                .read(&store.directory.join("workspace.json"))
                .unwrap()
                .as_slice(),
            b"workspace"
        );
    }

    #[test]
    fn empty_and_oversized_passwords_leave_setup_unmodified() {
        for password in [String::new(), "x".repeat(1025)] {
            let root = tempfile::tempdir().unwrap();
            for mode in [Mode::System, Mode::Manual] {
                let secrets = Secrets::default();
                assert!(setup(root.path(), &mut None, mode, &password, &secrets).is_err());
                assert!(secrets.password.borrow().is_none());
                assert!(!root.path().join(CONFIG).exists());
            }
        }
    }

    #[test]
    fn encrypted_keys_round_trip_through_native_openssh_and_scratch_is_removed() {
        let root = tempfile::tempdir().unwrap();
        let store = Store {
            directory: root.path().into(),
            key: Some(Zeroizing::new([17; 32])),
        };
        let scratch = scratch().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let source = scratch.path().join("keys").join(&id);
        crate::generate_private_key(&source, "local-encryption-test", "ed25519", "").unwrap();
        let bytes = Zeroizing::new(fs::read(&source).unwrap());
        let destination = store.directory.join("keys").join(&id);
        store.write(&destination, &bytes).unwrap();
        let output_dir = self::scratch().unwrap();
        let output_key = output_dir.path().join("keys").join(&id);
        store.stage_session(output_dir.path(), &id).unwrap();
        let output = std::process::Command::new(crate::ssh::program("ssh-keygen").unwrap())
            .args(["-y", "-P", "", "-f"])
            .arg(&output_key)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .starts_with("ssh-ed25519 ")
        );
        let scratch_path = output_dir.path().to_path_buf();
        drop(output_dir);
        assert!(!scratch_path.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(destination).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn session_refresh_applies_trust_revocations_and_rejects_deleted_keys() {
        let root = tempfile::tempdir().unwrap();
        let store = Store {
            directory: root.path().into(),
            key: Some(Zeroizing::new([9; 32])),
        };
        let scratch = scratch().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let key_path = store.directory.join("keys").join(&id);
        store.write(&key_path, b"key").unwrap();
        let known = store.directory.join("known_hosts");
        store.write(&known, b"initial trust").unwrap();
        assert_eq!(
            store.stage_session(scratch.path(), &id).unwrap(),
            "initial trust"
        );
        store.write(&known, b"updated trust").unwrap();
        assert_eq!(
            store.stage_session(scratch.path(), &id).unwrap(),
            "updated trust"
        );
        assert_eq!(
            fs::read_to_string(scratch.path().join("known_hosts")).unwrap(),
            "updated trust"
        );
        fs::remove_file(key_path).unwrap();
        assert!(store.stage_session(scratch.path(), &id).is_err());
        assert!(store.stage_session(scratch.path(), "../escape").is_err());
        assert!(store.stage_session(scratch.path(), "").is_ok());
    }

    #[test]
    #[ignore = "Requires an unlocked native secret store; uses a disposable credential entry."]
    fn native_secret_store_roundtrip() {
        let root = tempfile::tempdir().unwrap();
        let secrets = SystemSecrets(root.path());
        let result: Result<()> = (|| {
            let mut state = None;
            let password = Zeroizing::new(uuid::Uuid::new_v4().to_string());
            if setup(root.path(), &mut state, Mode::System, &password, &secrets)?.state != "ready" {
                return Err("The native secret store is unavailable.".into());
            }
            let store = state.take().ok_or("Storage was not unlocked.")?;
            store.write(
                &store.directory.join("workspace.json"),
                b"native store test",
            )?;
            if initialize(root.path(), &mut state, &secrets)?.state != "ready" {
                return Err("Automatic unlock through native secret storage failed.".into());
            }
            let store = state.take().ok_or("Storage was not unlocked.")?;
            if store
                .read(&store.directory.join("workspace.json"))?
                .as_slice()
                != b"native store test"
            {
                return Err("Native store test data did not round-trip.".into());
            }
            secrets.clear();
            if initialize(root.path(), &mut state, &secrets)?.state != "locked" {
                return Err("Missing native credentials did not keep storage locked.".into());
            }
            Ok(())
        })();
        secrets.clear();
        result.unwrap();
    }
}
