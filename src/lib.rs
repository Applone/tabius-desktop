mod pty;
mod ssh;
mod storage;
mod sync;

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use tauri::{Manager, State, ipc::Channel};
use uuid::Uuid;

type Result<T> = std::result::Result<T, String>;
struct Session {
    master: pty::Terminal,
    pid: u32,
}
#[derive(Default, Clone)]
struct Sessions(Arc<Mutex<HashMap<String, Session>>>);
fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf> {
    let path = app.path().app_data_dir().map_err(|e| e.to_string())?;
    storage::directory(&path)?;
    storage::directory(&path.join("keys"))?;
    Ok(path)
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
    if !path.exists() {
        return Ok(
            json!({"hosts":[],"keys":[],"snippets":[],"forwards":[],"history":[],"settings":{}}),
        );
    }
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
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
    let bytes = serde_json::to_vec_pretty(&workspace).map_err(|e| e.to_string())?;
    if bytes.len() > 5 * 1024 * 1024 {
        return Err("Workspace exceeds 5 MB.".into());
    }
    let dir = data_dir(&app)?;
    let temp = dir.join(format!("workspace-{}.tmp", Uuid::new_v4()));
    private_write(&temp, &bytes)?;
    let result = fs::rename(&temp, dir.join("workspace.json")).map_err(|e| e.to_string());
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Output {
    Data { data: Vec<u8> },
    Exit { code: i32 },
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
    let directory = data_dir(&app)?;
    let arguments = ssh::args(&host, &settings, forward.as_ref(), &directory)?;
    let mosh = host.mosh && forward.is_none();
    let mut cmd = Command::new(if mosh { "mosh" } else { "ssh" });
    if mosh {
        // Mosh's --ssh option is shell parsed. Quote every argument and keep the destination separate.
        let position = arguments
            .iter()
            .position(|a| a == "--")
            .ok_or("Missing destination.")?;
        let ssh_command = std::iter::once("ssh".to_string())
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
    let pid = process.child.id();
    let mut reader = process.master.try_clone().map_err(|e| e.to_string())?;
    map.insert(
        id.clone(),
        Session {
            master: process.master,
            pid,
        },
    );
    drop(map);
    let shared = sessions.inner().clone();
    let thread_id = id.clone();
    let mut child = process.child;
    std::thread::spawn(move || {
        let mut buffer = [0; 16384];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    if on_output
                        .send(Output::Data {
                            data: buffer[..n].to_vec(),
                        })
                        .is_err()
                    {
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
        if let Ok(mut map) = shared.0.lock()
            && map
                .get(&thread_id)
                .is_some_and(|session| session.pid == pid)
        {
            map.remove(&thread_id);
        }
        let _ = on_output.send(Output::Exit { code });
    });
    Ok(id)
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
fn resize_session(sessions: State<'_, Sessions>, id: String, rows: u16, cols: u16) -> Result<()> {
    let map = sessions.0.lock().map_err(|e| e.to_string())?;
    let session = map.get(&id).ok_or("Session is no longer running.")?;
    pty::resize(&session.master, rows, cols)
}
#[tauri::command]
fn close_session(sessions: State<'_, Sessions>, id: String) -> Result<()> {
    if let Some(session) = sessions.0.lock().map_err(|e| e.to_string())?.remove(&id) {
        pty::stop(session.pid);
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
fn generate_private_key(path: &Path, name: &str, kind: &str, passphrase: &str) -> Result<()> {
    if passphrase.len() > 1024 || passphrase.chars().any(char::is_control) {
        return Err(
            "Passphrases must not contain control characters and must be shorter than 1024 bytes."
                .into(),
        );
    }
    let mut command = Command::new("ssh-keygen");
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
        let path = data_dir(&app)?.join("keys").join(&id);
        generate_private_key(&path, &name, &kind, &passphrase)?;
        key_entry(&path, id, name)
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
        let id=Uuid::new_v4().to_string();let path=data_dir(&app)?.join("keys").join(&id);
        private_write(&path,format!("{}\n",content.trim()).as_bytes())?;
        let result=(||{
            let public=if public_key.trim().is_empty(){let output=Command::new("ssh-keygen").args(["-y","-P","","-f"]).arg(&path).output().map_err(|e|e.to_string())?;if !output.status.success(){return Err("Cannot read private key. For an encrypted key, supply its matching public key.".into());}String::from_utf8_lossy(&output.stdout).into_owned()}else{public_key};
            public_fingerprint(&public)?;private_write(&path.with_extension("pub"),public.as_bytes())?;key_entry(&path,id,name)
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
#[tauri::command]
fn read_sync_keys(app: tauri::AppHandle, ids: Vec<String>) -> Result<Vec<SyncKey>> {
    if ids.len() > 1000 {
        return Err("Too many keys to synchronize.".into());
    }
    let directory = data_dir(&app)?.join("keys");
    ids.into_iter()
        .map(|id| {
            ssh::valid_id(&id)?;
            let path = directory.join(&id);
            if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 128 * 1024 {
                return Err("Private key exceeds 128 KB.".into());
            }
            Ok(SyncKey {
                id,
                secret: fs::read_to_string(&path).map_err(|e| e.to_string())?,
                public_key: fs::read_to_string(path.with_extension("pub"))
                    .map_err(|e| e.to_string())?,
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
    let mut command = Command::new("ssh-keygen");
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
#[tauri::command]
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
        let path = data_dir(&app)?.join("keys").join(&id);
        private_write(&path, secret.as_bytes())?;
        let result = (|| {
            let derived = derive_sync_public(&path, &secret, &passphrase)?;
            let (_, fingerprint) = public_fingerprint(&derived)?;
            if !public_key.is_empty() && public_fingerprint(&public_key)?.1 != fingerprint {
                return Err("The synchronized public and private keys do not match.".into());
            }
            private_write(&path.with_extension("pub"), derived.as_bytes())?;
            key_entry(&path, id, name)
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
#[tauri::command]
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
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    content
        .lines()
        .filter(|s| !s.trim().is_empty() && !s.starts_with('#'))
        .map(parse_known)
        .collect()
}
#[tauri::command]
fn import_known_hosts(app: tauri::AppHandle, content: String) -> Result<()> {
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
    let mut file = storage::options()
        .create(true)
        .append(true)
        .open(data_dir(&app)?.join("known_hosts"))
        .map_err(|e| e.to_string())?;
    file.write_all(format!("\n{}\n", lines.join("\n")).as_bytes())
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn remove_known_host(app: tauri::AppHandle, line: String) -> Result<()> {
    let dir = data_dir(&app)?;
    let path = dir.join("known_hosts");
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let kept = content
        .lines()
        .filter(|s| *s != line)
        .collect::<Vec<_>>()
        .join("\n");
    let temp = dir.join(format!("known-{}.tmp", Uuid::new_v4()));
    private_write(&temp, format!("{kept}\n").as_bytes())?;
    fs::rename(temp, path).map_err(|e| e.to_string())
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
#[tauri::command]
fn prune_synced_fingerprints(app: tauri::AppHandle, removed: Vec<String>) -> Result<()> {
    let directory = data_dir(&app)?;
    let path = directory.join("known_hosts");
    if removed.is_empty() || !path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let pruned = prune_fingerprints(&content, &removed);
    let temporary = directory.join(format!("known-{}.tmp", Uuid::new_v4()));
    private_write(&temporary, pruned.as_bytes())?;
    fs::rename(temporary, path).map_err(|e| e.to_string())
}
pub fn run() {
    let app = tauri::Builder::default()
        .manage(Sessions::default())
        .invoke_handler(tauri::generate_handler![
            load_workspace,
            save_workspace,
            start_session,
            write_session,
            resize_session,
            close_session,
            generate_key,
            import_key,
            delete_key,
            known_hosts,
            import_known_hosts,
            remove_known_host,
            sync::tabby_request,
            read_sync_keys,
            import_sync_key,
            prune_synced_fingerprints
        ])
        .build(tauri::generate_context!())
        .expect("Unable to initialize Tabius");
    app.run(|app, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            let sessions = app.state::<Sessions>();
            if let Ok(map) = sessions.0.lock() {
                for session in map.values() {
                    pty::stop(session.pid);
                }
            };
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_files_are_exclusive_and_readable_by_openssh() {
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
        let output = Command::new("ssh-keygen")
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
