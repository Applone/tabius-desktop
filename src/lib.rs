mod local;
mod migration;
mod pty;
mod ssh;
mod ssh_config;
mod storage;
mod sync;
mod tabby;
mod vault;
mod workspace;

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{Manager, State, ipc::Channel};
use uuid::Uuid;

type Result<T> = std::result::Result<T, String>;
struct Session {
    master: pty::Terminal,
    pid: u32,
    cancelled: Arc<AtomicBool>,
    rows: u16,
    cols: u16,
}
#[derive(Default, Clone)]
struct Sessions(Arc<Mutex<HashMap<String, Session>>>);
static TRUST: Mutex<()> = Mutex::new(());
fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf> {
    local::directory(app)
}
fn private_write(path: &Path, content: &[u8]) -> Result<()> {
    let mut file = storage::options()
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(content)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn load_workspace(app: tauri::AppHandle) -> Result<Value> {
    let path = data_dir(&app)?.join("workspace.json");
    let mut value = if path.exists() {
        serde_json::from_slice(&local::read(&app, &path)?).map_err(|e| e.to_string())?
    } else {
        workspace::empty()
    };
    let mut settings = workspace::settings();
    if let Some(saved) = value["settings"].as_object() {
        settings.as_object_mut().unwrap().extend(saved.clone());
    }
    value["settings"] = settings;
    if value.get("groups").is_none() {
        value["groups"] = serde_json::json!([]);
    }
    Ok(value)
}
#[tauri::command]
fn save_workspace(app: tauri::AppHandle, workspace: Value) -> Result<()> {
    for key in ["hosts", "keys", "snippets", "forwards", "history"] {
        if !workspace[key].is_array() {
            return Err("Invalid workspace.".into());
        }
    }
    if !workspace["settings"].is_object() {
        return Err("Invalid settings.".into());
    }
    if workspace
        .get("groups")
        .is_some_and(|groups| !groups.is_array())
    {
        return Err("Invalid host groups.".into());
    }
    let bytes = serde_json::to_vec_pretty(&workspace).map_err(|e| e.to_string())?;
    if bytes.len() > 5 * 1024 * 1024 {
        return Err("Workspace exceeds 5 MB.".into());
    }
    let dir = data_dir(&app)?;
    local::write(&app, &dir.join("workspace.json"), &bytes)
}
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Output {
    Data { data: Vec<u8> },
    Exit { code: i32 },
    Status { status: String },
}
#[allow(clippy::too_many_arguments)]
#[tauri::command]
fn start_session(
    app: tauri::AppHandle,
    sessions: State<'_, Sessions>,
    id: String,
    host: ssh::Host,
    settings: ssh::Settings,
    forward: Option<ssh::Forward>,
    rows: u16,
    cols: u16,
    on_output: Channel<Output>,
) -> Result<String> {
    ssh::valid_id(&id)?;
    let scratch = local::scratch()?;
    let session_directory = scratch.path();
    let mut initial_known = local::stage_session(&app, session_directory, &host.key_id)?;
    let arguments = ssh::args(&host, &settings, forward.as_ref(), session_directory)?;
    let mosh = host.mosh && forward.is_none();
    let ssh_program = ssh::program("ssh")?;
    let mut cmd = Command::new(if mosh {
        Path::new("mosh")
    } else {
        &ssh_program
    });
    if mosh {
        // Mosh's --ssh option is shell parsed. Quote every argument and keep the destination separate.
        let position = arguments
            .iter()
            .position(|a| a == "--")
            .ok_or("Missing destination.")?;
        let ssh_command = std::iter::once(ssh_program.to_string_lossy().replace('\\', "/"))
            .chain(
                arguments[..position]
                    .iter()
                    .filter(|a| a.as_str() != "-tt")
                    .cloned(),
            )
            .map(|a| format!("'{}'", a.replace('\'', "'\\''")))
            .collect::<Vec<_>>()
            .join(" ");
        cmd.arg(format!("--ssh={ssh_command}"));
        cmd.arg("--");
        cmd.arg(&arguments[position + 1]);
        if let Some(remote_command) = arguments.get(position + 2) {
            cmd.args(["sh", "-lc", remote_command]);
        }
    } else {
        cmd.args(&arguments);
    }
    cmd.env("TERM", &settings.emulator)
        .env_remove("SSH_ASKPASS")
        .env("SSH_ASKPASS_REQUIRE", "never");
    let mut map = sessions.0.lock().map_err(|e| e.to_string())?;
    if map.contains_key(&id) {
        return Err("Session already exists.".into());
    }
    let process = pty::spawn(&mut cmd, rows, cols)?;
    let mut pid = process.child.id();
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut reader = process.master.try_clone().map_err(|e| e.to_string())?;
    map.insert(
        id.clone(),
        Session {
            master: process.master,
            pid,
            cancelled: cancelled.clone(),
            rows,
            cols,
        },
    );
    drop(map);
    let shared = sessions.inner().clone();
    let thread_id = id.clone();
    let mut child = process.child;
    std::thread::spawn(move || {
        let session_files = scratch;
        loop {
            let mut buffer = [0; 16384];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Err(error) =
                            merge_session_trust(&app, session_files.path(), &mut initial_known)
                        {
                            let _ = on_output.send(Output::Status { status: error });
                        }
                        if on_output
                            .send(Output::Data {
                                data: buffer[..n].to_vec(),
                            })
                            .is_err()
                        {
                            cancelled.store(true, Ordering::Release);
                            pty::stop(pid);
                            break;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            let code = child
                .wait()
                .ok()
                .map(|s| s.exit_code() as i32)
                .unwrap_or(-1);
            if let Err(error) = merge_session_trust(&app, session_files.path(), &mut initial_known)
            {
                let _ = on_output.send(Output::Status { status: error });
            }
            let reconnect = code == 255
                && load_workspace(app.clone())
                    .is_ok_and(|w| w["settings"]["autoReconnect"] == true);
            if !reconnect || cancelled.load(Ordering::Acquire) {
                if let Ok(mut map) = shared.0.lock()
                    && map
                        .get(&thread_id)
                        .is_some_and(|session| session.pid == pid)
                {
                    map.remove(&thread_id);
                }
                let _ = on_output.send(Output::Exit { code });
                break;
            }
            if let Ok(mut map) = shared.0.lock() {
                if let Some(session) = map.get_mut(&thread_id) {
                    session.pid = 0;
                } else {
                    return;
                }
            } else {
                return;
            }
            let _ = on_output.send(Output::Status {
                status: "Reconnecting".into(),
            });
            if !wait_for_retry(&cancelled, std::time::Duration::from_secs(3)) {
                return;
            }
            let Ok(mut map) = shared.0.lock() else {
                return;
            };
            let Some(session) = map.get_mut(&thread_id) else {
                return;
            };
            if cancelled.load(Ordering::Acquire) {
                return;
            }
            let (rows, cols) = (session.rows, session.cols);
            // Re-read trust and keys so reconnect cannot reuse revoked credentials.
            let attempt = local::stage_session(&app, session_files.path(), &host.key_id)
                .and_then(|known| {
                    initial_known = known;
                    pty::spawn(&mut cmd, rows, cols)
                })
                .and_then(|process| {
                    let reader = process.master.try_clone().map_err(|e| e.to_string())?;
                    Ok((process, reader))
                });
            match attempt {
                Ok((process, next_reader)) => {
                    pid = process.child.id();
                    child = process.child;
                    reader = next_reader;
                    session.pid = pid;
                    session.master = process.master;
                    let _ = on_output.send(Output::Status {
                        status: "Running".into(),
                    });
                }
                Err(error) => {
                    let _ = on_output.send(Output::Status { status: error });
                    map.remove(&thread_id);
                    let _ = on_output.send(Output::Exit { code: -1 });
                    break;
                }
            }
        }
    });
    Ok(id)
}
fn wait_for_retry(cancelled: &AtomicBool, delay: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + delay;
    loop {
        if cancelled.load(Ordering::Acquire) {
            return false;
        }
        let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) else {
            return true;
        };
        std::thread::sleep(remaining.min(std::time::Duration::from_millis(100)));
    }
}
#[tauri::command]
fn write_session(sessions: State<'_, Sessions>, id: String, data: String) -> Result<()> {
    if data.len() > 1024 * 1024 {
        return Err("Input is too large.".into());
    }
    sessions
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .get_mut(&id)
        .ok_or("Session is no longer running.")?
        .master
        .write_all(data.as_bytes())
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn remove_item(
    app: tauri::AppHandle,
    workspace: Value,
    kind: String,
    item: Value,
) -> Result<Value> {
    let next = workspace::edit_workspace(workspace, kind.clone(), item.clone(), true)?;
    if kind == "key" {
        delete_key(app, workspace::text(&item["id"]).into())?;
    }
    Ok(next)
}
#[tauri::command]
fn run_snippet(
    app: tauri::AppHandle,
    sessions: State<'_, Sessions>,
    id: String,
    snippet_id: String,
) -> Result<()> {
    let workspace = load_workspace(app)?;
    let snippets = workspace::list(&workspace["snippets"]);
    let snippet = snippets
        .iter()
        .find(|s| s["id"] == snippet_id)
        .ok_or("Snippet no longer exists.")?;
    write_session(
        sessions,
        id,
        format!("{}\r", workspace::text(&snippet["command"])),
    )
}
#[tauri::command]
fn resize_session(sessions: State<'_, Sessions>, id: String, rows: u16, cols: u16) -> Result<()> {
    let mut map = sessions.0.lock().map_err(|e| e.to_string())?;
    let session = map.get_mut(&id).ok_or("Session is no longer running.")?;
    session.rows = rows;
    session.cols = cols;
    pty::resize(&session.master, rows, cols)
}
#[tauri::command]
fn close_session(sessions: State<'_, Sessions>, id: String) -> Result<()> {
    if let Some(session) = sessions.0.lock().map_err(|e| e.to_string())?.remove(&id) {
        session.cancelled.store(true, Ordering::Release);
        if session.pid != 0 {
            pty::stop(session.pid);
        }
    }
    Ok(())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KeyEntry {
    id: String,
    name: String,
    kind: String,
    public_key: String,
    fingerprint: String,
}
fn public_fingerprint(public: &str) -> Result<(String, String)> {
    let parts: Vec<_> = public.split_whitespace().collect();
    if parts.len() < 2
        || ![
            "ssh-ed25519",
            "ssh-rsa",
            "ecdsa-sha2-nistp256",
            "ecdsa-sha2-nistp384",
            "ecdsa-sha2-nistp521",
            "sk-ssh-ed25519@openssh.com",
            "sk-ecdsa-sha2-nistp256@openssh.com",
        ]
        .contains(&parts[0])
    {
        return Err("Unsupported OpenSSH public key.".into());
    }
    let bytes = STANDARD
        .decode(parts[1])
        .map_err(|_| "Invalid public key encoding.")?;
    if bytes.len() < 8 {
        return Err("Public key is truncated.".into());
    }
    let length = u32::from_be_bytes(bytes[..4].try_into().map_err(|_| "Invalid key.")?) as usize;
    if bytes.get(4..4 + length) != Some(parts[0].as_bytes()) {
        return Err("Public key type does not match its contents.".into());
    }
    let fingerprint = format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(Sha256::digest(bytes))
    );
    Ok((parts[0].into(), fingerprint))
}
fn key_entry(path: &Path, id: String, name: String) -> Result<KeyEntry> {
    let public_key = fs::read_to_string(path.with_extension("pub")).map_err(|e| e.to_string())?;
    let (kind, fingerprint) = public_fingerprint(&public_key)?;
    Ok(KeyEntry {
        id,
        name,
        kind,
        public_key: public_key.trim().into(),
        fingerprint,
    })
}
fn persist_key(app: &tauri::AppHandle, source: &Path, id: &str) -> Result<()> {
    ssh::valid_id(id)?;
    let destination = data_dir(app)?.join("keys").join(id);
    if destination.exists() || destination.with_extension("pub").exists() {
        return Err("A key with this identifier already exists.".into());
    }
    let public = fs::read(source.with_extension("pub")).map_err(|e| e.to_string())?;
    let secret = zeroize::Zeroizing::new(fs::read(source).map_err(|e| e.to_string())?);
    // Publish the private file last so a failed write cannot leave a usable partial key.
    local::write(app, &destination.with_extension("pub"), &public)?;
    if let Err(error) = local::write(app, &destination, &secret) {
        let _ = fs::remove_file(destination.with_extension("pub"));
        return Err(error);
    }
    Ok(())
}

fn merge_session_trust(app: &tauri::AppHandle, scratch: &Path, initial: &mut String) -> Result<()> {
    let content = fs::read_to_string(scratch.join("known_hosts")).map_err(|e| e.to_string())?;
    let added: Vec<_> = content
        .lines()
        .filter(|line| !initial.lines().any(|old| old == *line))
        .collect();
    if added.is_empty() {
        return Ok(());
    }
    let path = data_dir(app)?.join("known_hosts");
    // Serialize trust merges independently of file encryption to avoid lost session updates.
    let _guard = TRUST.lock().map_err(|e| e.to_string())?;
    let mut current = if path.exists() {
        local::text(app, &path)?
    } else {
        String::new()
    };
    for line in added {
        parse_known(line)?;
        if !current.lines().any(|old| old == line) {
            current.push('\n');
            current.push_str(line);
        }
    }
    local::write(app, &path, current.as_bytes())?;
    *initial = content;
    Ok(())
}
fn generate_private_key(path: &Path, name: &str, kind: &str, passphrase: &str) -> Result<()> {
    if passphrase.len() > 1024 || passphrase.chars().any(char::is_control) {
        return Err(
            "Passphrases must not contain control characters and must be shorter than 1024 bytes."
                .into(),
        );
    }
    let mut command = Command::new(ssh::program("ssh-keygen")?);
    command
        .args(["-q", "-t", kind, "-a", "100", "-C", name, "-f"])
        .arg(path);
    if kind == "rsa" {
        command.args(["-b", "4096"]);
    }
    // Feed interactive prompts through the PTY so the passphrase never appears in argv.
    let mut process = pty::spawn(&mut command, 24, 80)?;
    let mut output = String::new();
    let mut buffer = [0; 2048];
    let mut first = false;
    let mut second = false;
    loop {
        match process.master.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                output.push_str(&String::from_utf8_lossy(&buffer[..n]));
                if !first && output.contains("Enter passphrase") {
                    process
                        .master
                        .write_all(format!("{passphrase}\r").as_bytes())
                        .map_err(|e| e.to_string())?;
                    first = true;
                }
                if !second && output.contains("Enter same passphrase again") {
                    process
                        .master
                        .write_all(format!("{passphrase}\r").as_bytes())
                        .map_err(|e| e.to_string())?;
                    second = true;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    if !process.child.wait().map_err(|e| e.to_string())?.success() {
        let _ = fs::remove_file(path);
        return Err(format!("Key generation failed: {output}"));
    }
    Ok(())
}
#[tauri::command]
async fn generate_key(
    app: tauri::AppHandle,
    name: String,
    kind: String,
    passphrase: String,
) -> Result<KeyEntry> {
    tauri::async_runtime::spawn_blocking(move || {
        if name.trim().is_empty() || name.len() > 200 || name.chars().any(char::is_control) {
            return Err("Invalid key name.".into());
        }
        if !["ed25519", "rsa"].contains(&kind.as_str()) {
            return Err("Choose Ed25519 or RSA.".into());
        }
        let id = Uuid::new_v4().to_string();
        let scratch = local::scratch()?;
        let path = scratch.path().join("keys").join(&id);
        generate_private_key(&path, &name, &kind, &passphrase)?;
        let entry = key_entry(&path, id, name)?;
        persist_key(&app, &path, &entry.id)?;
        Ok(entry)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn import_key(
    app: tauri::AppHandle,
    name: String,
    content: String,
    public_key: String,
) -> Result<KeyEntry> {
    tauri::async_runtime::spawn_blocking(move||{
        if name.trim().is_empty()||content.len()>1024*1024||!content.contains("PRIVATE KEY-----"){return Err("Enter a label and an OpenSSH or PEM private key.".into());}
        let id=Uuid::new_v4().to_string();let scratch=local::scratch()?;let path=scratch.path().join("keys").join(&id);
        private_write(&path,format!("{}\n",content.trim()).as_bytes())?;
        let result=(||{
            let public=if public_key.trim().is_empty(){let output=Command::new(ssh::program("ssh-keygen")?).args(["-y","-P","","-f"]).arg(&path).output().map_err(|e|e.to_string())?;if !output.status.success(){return Err("Cannot read private key. For an encrypted key, supply its matching public key.".into());}String::from_utf8_lossy(&output.stdout).into_owned()}else{public_key};
            public_fingerprint(&public)?;private_write(&path.with_extension("pub"),public.as_bytes())?;let entry=key_entry(&path,id,name)?;persist_key(&app,&path,&entry.id)?;Ok(entry)
        })();
        if result.is_err(){let _=fs::remove_file(&path);let _=fs::remove_file(path.with_extension("pub"));}result
    }).await.map_err(|e|e.to_string())?
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncKey {
    id: String,
    secret: String,
    public_key: String,
}
fn read_sync_keys(app: tauri::AppHandle, ids: Vec<String>) -> Result<Vec<SyncKey>> {
    if ids.len() > 1000 {
        return Err("Too many keys to synchronize.".into());
    }
    let directory = data_dir(&app)?.join("keys");
    ids.into_iter()
        .map(|id| {
            ssh::valid_id(&id)?;
            let path = directory.join(&id);
            let secret = local::text(&app, &path)?;
            if secret.len() > 128 * 1024 {
                return Err("Private key exceeds 128 KB.".into());
            }
            Ok(SyncKey {
                id,
                secret,
                public_key: local::text(&app, &path.with_extension("pub"))?,
            })
        })
        .collect()
}
fn wire_string<'a>(data: &mut &'a [u8]) -> Result<&'a [u8]> {
    let prefix = data.get(..4).ok_or("Truncated OpenSSH key.")?;
    let size = u32::from_be_bytes(prefix.try_into().map_err(|_| "Invalid OpenSSH key.")?) as usize;
    let value = data.get(4..4 + size).ok_or("Truncated OpenSSH key.")?;
    *data = &data[4 + size..];
    Ok(value)
}
fn openssh_public(content: &str) -> Result<Option<String>> {
    if !content.contains("-----BEGIN OPENSSH PRIVATE KEY-----") {
        return Ok(None);
    }
    let encoded = content
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect::<String>();
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "Invalid OpenSSH private key.")?;
    let mut data = bytes
        .strip_prefix(b"openssh-key-v1\0")
        .ok_or("Invalid OpenSSH private key header.")?;
    for _ in 0..3 {
        wire_string(&mut data)?;
    }
    if data.get(..4) != Some(&[0, 0, 0, 1]) {
        return Err("Only single-key OpenSSH files are supported.".into());
    }
    data = &data[4..];
    let public = wire_string(&mut data)?;
    let mut fields = public;
    let kind = std::str::from_utf8(wire_string(&mut fields)?).map_err(|_| "Invalid key type.")?;
    Ok(Some(format!("{} {}", kind, STANDARD.encode(public))))
}
fn derive_sync_public(path: &Path, content: &str, passphrase: &str) -> Result<String> {
    if let Some(public) = openssh_public(content)? {
        public_fingerprint(&public)?;
        return Ok(public);
    }
    if passphrase.len() > 1024 || passphrase.chars().any(char::is_control) {
        return Err("Invalid key passphrase.".into());
    }
    let mut command = Command::new(ssh::program("ssh-keygen")?);
    command.args(["-y", "-f"]).arg(path);
    let mut process = pty::spawn(&mut command, 24, 80)?;
    let mut output = String::new();
    let mut buffer = [0; 4096];
    let mut sent = false;
    loop {
        match process.master.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                output.push_str(&String::from_utf8_lossy(&buffer[..n]));
                if !sent && output.contains("Enter passphrase") {
                    process
                        .master
                        .write_all(format!("{passphrase}\r").as_bytes())
                        .map_err(|e| e.to_string())?;
                    sent = true;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    if !process.child.wait().map_err(|e| e.to_string())?.success() {
        return Err(
            "Cannot import this private key. Its format or saved passphrase is unsupported.".into(),
        );
    }
    let tokens = output.split_whitespace().collect::<Vec<_>>();
    tokens
        .windows(2)
        .find_map(|pair| {
            let candidate = format!("{} {}", pair[0], pair[1]);
            public_fingerprint(&candidate).ok().map(|_| candidate)
        })
        .ok_or("Cannot extract the public key.".into())
}
async fn import_sync_key(
    app: tauri::AppHandle,
    name: String,
    secret: String,
    public_key: String,
    passphrase: String,
) -> Result<KeyEntry> {
    tauri::async_runtime::spawn_blocking(move || {
        if secret.len() > 128 * 1024 || !secret.contains("PRIVATE KEY") || name.trim().is_empty() {
            return Err("Invalid synchronized private key.".into());
        }
        let id = Uuid::new_v4().to_string();
        let scratch = local::scratch()?;
        let path = scratch.path().join("keys").join(&id);
        private_write(&path, secret.as_bytes())?;
        let result = (|| {
            let derived = derive_sync_public(&path, &secret, &passphrase)?;
            let (_, fingerprint) = public_fingerprint(&derived)?;
            if !public_key.is_empty() && public_fingerprint(&public_key)?.1 != fingerprint {
                return Err("The synchronized public and private keys do not match.".into());
            }
            private_write(&path.with_extension("pub"), derived.as_bytes())?;
            let entry = key_entry(&path, id, name)?;
            persist_key(&app, &path, &entry.id)?;
            Ok(entry)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&path);
            let _ = fs::remove_file(path.with_extension("pub"));
        }
        result
    })
    .await
    .map_err(|e| e.to_string())?
}
fn delete_key(app: tauri::AppHandle, id: String) -> Result<()> {
    ssh::valid_id(&id)?;
    let path = data_dir(&app)?.join("keys").join(id);
    for file in [&path, &path.with_extension("pub")] {
        if file.exists() {
            fs::remove_file(file).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
#[derive(Serialize)]
struct KnownHost {
    line: String,
    host: String,
    kind: String,
    fingerprint: String,
}
fn parse_known(line: &str) -> Result<KnownHost> {
    let parts: Vec<_> = line.split_whitespace().collect();
    if parts.len() < 3 || parts[0].starts_with('@') {
        return Err("Use ordinary OpenSSH known_hosts entries, without markers.".into());
    }
    let (kind, fingerprint) = public_fingerprint(&format!("{} {}", parts[1], parts[2]))?;
    Ok(KnownHost {
        line: line.into(),
        host: parts[0].into(),
        kind,
        fingerprint,
    })
}
#[tauri::command]
fn known_hosts(app: tauri::AppHandle) -> Result<Vec<KnownHost>> {
    let path = data_dir(&app)?.join("known_hosts");
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = local::text(&app, &path)?;
    content
        .lines()
        .filter(|s| !s.trim().is_empty() && !s.starts_with('#'))
        .map(parse_known)
        .collect()
}
#[tauri::command]
fn import_known_hosts(app: tauri::AppHandle, content: String) -> Result<()> {
    let _guard = TRUST.lock().map_err(|e| e.to_string())?;
    if content.len() > 1024 * 1024 {
        return Err("File exceeds 1 MB.".into());
    }
    let lines: Vec<_> = content
        .lines()
        .filter(|s| !s.trim().is_empty() && !s.starts_with('#'))
        .collect();
    if lines.is_empty() {
        return Err("No host keys found.".into());
    }
    for line in &lines {
        parse_known(line)?;
    }
    let path = data_dir(&app)?.join("known_hosts");
    let mut existing = if path.exists() {
        local::text(&app, &path)?
    } else {
        String::new()
    };
    existing.push_str(&format!("\n{}\n", lines.join("\n")));
    local::write(&app, &path, existing.as_bytes())
}
fn remove_known_host(app: tauri::AppHandle, line: String) -> Result<()> {
    let _guard = TRUST.lock().map_err(|e| e.to_string())?;
    let dir = data_dir(&app)?;
    let path = dir.join("known_hosts");
    let content = local::text(&app, &path)?;
    let kept = content
        .lines()
        .filter(|s| *s != line)
        .collect::<Vec<_>>()
        .join("\n");
    local::write(&app, &path, format!("{kept}\n").as_bytes())
}
fn prune_fingerprints(content: &str, removed: &[String]) -> String {
    let mut result = Vec::new();
    for line in content.lines() {
        let Ok(entry) = parse_known(line) else {
            result.push(line.to_string());
            continue;
        };
        let names = entry
            .host
            .split(',')
            .filter(|name| {
                let (host, port) = if let Some((host, port)) = name
                    .strip_prefix('[')
                    .and_then(|name| name.rsplit_once("]:"))
                {
                    (host, port)
                } else {
                    (*name, "22")
                };
                let version = format!(
                    "{}|{}|{}|{}",
                    host.to_lowercase(),
                    port,
                    entry.kind,
                    entry
                        .fingerprint
                        .trim_start_matches("SHA256:")
                        .trim_end_matches('=')
                );
                !removed.contains(&version)
            })
            .collect::<Vec<_>>();
        if !names.is_empty() {
            let rest = line
                .split_once(char::is_whitespace)
                .map(|(_, rest)| rest)
                .unwrap_or("");
            result.push(format!("{} {}", names.join(","), rest));
        }
    }
    format!("{}\n", result.join("\n"))
}
fn prune_synced_fingerprints(app: tauri::AppHandle, removed: Vec<String>) -> Result<()> {
    let _guard = TRUST.lock().map_err(|e| e.to_string())?;
    let directory = data_dir(&app)?;
    let path = directory.join("known_hosts");
    if removed.is_empty() || !path.exists() {
        return Ok(());
    }
    let content = local::text(&app, &path)?;
    let pruned = prune_fingerprints(&content, &removed);
    local::write(&app, &path, pruned.as_bytes())
}
#[tauri::command]
fn apply_sync(app: tauri::AppHandle, previous: Value, workspace: Value) -> Result<Vec<String>> {
    if load_workspace(app.clone())? != previous {
        return Err(
            "The local workspace changed after upload. Sync again to apply the remote result."
                .into(),
        );
    }
    let removed = workspace::list(&previous["sync"]["knownBaseline"])
        .iter()
        .filter(|old| {
            !workspace::list(&workspace["sync"]["fingerprints"])
                .iter()
                .any(|new| new["id"] == old["id"] && new["digest"] == old["digest"])
        })
        .map(|old| {
            format!(
                "{}|{}",
                workspace::text(&old["id"]),
                workspace::text(&old["digest"]).trim_end_matches('=')
            )
        })
        .collect();
    prune_synced_fingerprints(app.clone(), removed)?;
    save_workspace(app.clone(), workspace.clone())?;
    let mut warnings = Vec::new();
    for key in workspace::list(&previous["keys"]) {
        if !workspace::list(&workspace["keys"])
            .iter()
            .any(|k| k["id"] == key["id"])
            && delete_key(app.clone(), workspace::text(&key["id"]).into()).is_err()
        {
            warnings
                .push("Sync saved, but an unused private key file could not be removed.".into());
        }
    }
    Ok(warnings)
}

pub fn run() {
    let app = tauri::Builder::default()
        .manage(Sessions::default())
        .manage(local::LocalStorage::default())
        .invoke_handler(tauri::generate_handler![
            workspace::new_item,
            workspace::edit_workspace,
            remove_item,
            workspace::prepare_session,
            ssh_config::preview_ssh_config,
            ssh_config::import_ssh_config,
            tabby::synchronize_workspace,
            migration::migrate_configuration,
            tabby::list_configurations,
            tabby::display_known,
            tabby::forget_fingerprint,
            apply_sync,
            local::storage_status,
            local::configure_storage,
            local::unlock_storage,
            load_workspace,
            save_workspace,
            start_session,
            write_session,
            run_snippet,
            resize_session,
            close_session,
            generate_key,
            import_key,
            known_hosts,
            import_known_hosts,
        ])
        .build(tauri::generate_context!())
        .expect("Unable to initialize Tabius");
    app.run(|app, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            let sessions = app.state::<Sessions>();
            if let Ok(map) = sessions.0.lock() {
                for session in map.values() {
                    session.cancelled.store(true, Ordering::Release);
                    if session.pid != 0 {
                        pty::stop(session.pid);
                    }
                }
            };
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_a_session_interrupts_its_pending_retry() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_flag = cancelled.clone();
        let started = std::time::Instant::now();
        let worker = std::thread::spawn(move || {
            wait_for_retry(&worker_flag, std::time::Duration::from_secs(10))
        });
        cancelled.store(true, Ordering::Release);
        assert!(!worker.join().unwrap());
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert!(wait_for_retry(
            &AtomicBool::new(false),
            std::time::Duration::ZERO
        ));
    }
    #[test]
    fn private_files_are_exclusive_and_readable_by_openssh() {
        pty::test_with_timeout(|| {
            let temporary = tempfile::tempdir().unwrap();
            let directory = temporary.path().join("private");
            storage::directory(&directory).unwrap();
            let key = directory.join("key");
            generate_private_key(&key, "test", "ed25519", "").unwrap();
            let imported = directory.join("imported");
            let content = fs::read(&key).unwrap();
            private_write(&imported, &content).unwrap();
            assert!(private_write(&imported, b"replacement").is_err());
            assert_eq!(fs::read(&imported).unwrap(), content);
            let output = Command::new(ssh::program("ssh-keygen").unwrap())
                .args(["-y", "-P", "", "-f"])
                .arg(&imported)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(public_fingerprint(&String::from_utf8(output.stdout).unwrap()).is_ok());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
                    0o700
                );
                assert_eq!(
                    fs::metadata(&imported).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
        });
    }

    fn public_key_fixture() -> String {
        let mut blob = Vec::new();
        blob.extend_from_slice(&11u32.to_be_bytes());
        blob.extend_from_slice(b"ssh-ed25519");
        blob.extend_from_slice(&32u32.to_be_bytes());
        blob.extend_from_slice(&[1u8; 32]);
        format!("ssh-ed25519 {}", STANDARD.encode(blob))
    }

    #[test]
    fn fingerprint_matches_independent_sha256_vector() {
        let public = public_key_fixture();
        let (kind, fingerprint) = public_fingerprint(&public).unwrap();
        assert_eq!(kind, "ssh-ed25519");
        assert_eq!(
            fingerprint,
            "SHA256:RXm/ruZ0eTzRXKwi1AQEDynB0VgHQ2ac9KPSFdf/YnA"
        );
        assert_eq!(
            parse_known(&format!("example.com {public}"))
                .unwrap()
                .fingerprint,
            fingerprint
        );
    }

    #[test]
    fn extracts_public_identity_without_decrypting_an_openssh_envelope() {
        let public = public_key_fixture();
        let blob = STANDARD
            .decode(public.split_whitespace().nth(1).unwrap())
            .unwrap();
        let mut envelope = b"openssh-key-v1\0".to_vec();
        for field in [
            b"aes256-ctr".as_slice(),
            b"bcrypt".as_slice(),
            b"fixture-kdf-options".as_slice(),
        ] {
            envelope.extend_from_slice(&(field.len() as u32).to_be_bytes());
            envelope.extend_from_slice(field);
        }
        envelope.extend_from_slice(&1u32.to_be_bytes());
        envelope.extend_from_slice(&(blob.len() as u32).to_be_bytes());
        envelope.extend_from_slice(&blob);
        let content = format!(
            "-----BEGIN OPENSSH PRIVATE KEY-----\n{}\n-----END OPENSSH PRIVATE KEY-----",
            STANDARD.encode(envelope)
        );
        assert_eq!(
            openssh_public(&content).unwrap().as_deref(),
            Some(public.as_str())
        );
        assert!(openssh_public("-----BEGIN OPENSSH PRIVATE KEY-----\ninvalid").is_err());
        assert!(
            openssh_public("-----BEGIN RSA PRIVATE KEY-----")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn prunes_only_matching_fingerprint_aliases() {
        let public = public_key_fixture();
        let fingerprint = public_fingerprint(&public).unwrap().1;
        let line = format!("one.example,[two.example]:2222 {public}\n# retained comment\n");
        let removed = format!(
            "one.example|22|ssh-ed25519|{}",
            fingerprint.trim_start_matches("SHA256:")
        );
        let output = prune_fingerprints(&line, &[removed]);
        assert!(!output.contains("one.example"));
        assert!(output.contains("[two.example]:2222"));
        assert!(output.contains("# retained comment"));
    }

    #[test]
    fn rejects_bad_key_and_path_traversal() {
        assert!(public_fingerprint("ssh-ed25519 AAAA").is_err());
        assert!(ssh::valid_id("../../id_rsa").is_err());
        assert!(parse_known("host invalid key").is_err());
    }
}
