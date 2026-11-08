//! SFTP v3 over an OpenSSH subsystem; no remote shell commands or filename parsing.
//! Wire format: https://www.ietf.org/archive/id/draft-ietf-secsh-filexfer-02.txt
use crate::{Result, host_key, local, ssh};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, atomic::AtomicBool};
use tauri::{Manager, State, ipc::Channel};

#[derive(Default, Clone)]
pub struct Connections(Arc<Mutex<HashMap<String, Live>>>);
struct Live {
    connection: Arc<Mutex<Connection>>,
    approvals: Arc<host_key::Approvals>,
    child: Arc<Mutex<Child>>,
}
impl Drop for Live {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }
}
struct Connection {
    child: Arc<Mutex<Child>>,
    input: ChildStdin,
    output: ChildStdout,
    sequence: u32,
    _broker: host_key::Broker,
    _scratch: tempfile::TempDir,
}
impl Drop for Connection {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
fn number(bytes: &mut &[u8]) -> Result<u32> {
    let word = bytes.get(..4).ok_or("Truncated SFTP packet.")?;
    let n = u32::from_be_bytes(word.try_into().unwrap());
    *bytes = &bytes[4..];
    Ok(n)
}
fn string(bytes: &mut &[u8]) -> Result<Vec<u8>> {
    let n = number(bytes)? as usize;
    let value = bytes.get(..n).ok_or("Truncated SFTP string.")?.to_vec();
    *bytes = &bytes[n..];
    Ok(value)
}
fn put(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.extend((value.len() as u32).to_be_bytes());
    bytes.extend(value);
}
fn text(bytes: Vec<u8>) -> Result<String> {
    String::from_utf8(bytes).map_err(|_| "This filename is not UTF-8.".into())
}
fn status(packet: &[u8]) -> Result<()> {
    let mut bytes = packet.get(5..).ok_or("Invalid SFTP status.")?;
    let code = number(&mut bytes)?;
    if code == 0 {
        return Ok(());
    }
    let message = text(string(&mut bytes)?)?;
    Err(format!("SFTP {code}: {message}"))
}
impl Connection {
    fn packet(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        self.input
            .write_all(&(bytes.len() as u32).to_be_bytes())
            .and_then(|()| self.input.write_all(bytes))
            .map_err(|e| e.to_string())?;
        self.input.flush().map_err(|e| e.to_string())?;
        let mut length = [0; 4];
        self.output.read_exact(&mut length).map_err(|e| {
            format!("SFTP connection closed ({e}). Check host trust and key/agent authentication.")
        })?;
        let n = u32::from_be_bytes(length) as usize;
        if !(5..=4 * 1024 * 1024).contains(&n) {
            return Err("Invalid SFTP packet size.".into());
        }
        let mut packet = vec![0; n];
        self.output
            .read_exact(&mut packet)
            .map_err(|e| e.to_string())?;
        Ok(packet)
    }
    fn request(&mut self, kind: u8, payload: &[u8], expected: u8) -> Result<Vec<u8>> {
        self.sequence = self.sequence.wrapping_add(1);
        let mut bytes = vec![kind];
        bytes.extend(self.sequence.to_be_bytes());
        bytes.extend(payload);
        let reply = self.packet(&bytes)?;
        if reply[1..5] != self.sequence.to_be_bytes() {
            return Err("SFTP response identifier mismatch.".into());
        }
        if reply[0] == 101 {
            status(&reply)?;
        }
        if reply[0] != expected {
            return Err("Unexpected SFTP response.".into());
        }
        Ok(reply[5..].to_vec())
    }
    fn path_request(&mut self, kind: u8, path: &str, expected: u8) -> Result<Vec<u8>> {
        let mut payload = Vec::new();
        put(&mut payload, path.as_bytes());
        self.request(kind, &payload, expected)
    }
    fn close(&mut self, handle: &[u8]) -> Result<()> {
        let mut payload = Vec::new();
        put(&mut payload, handle);
        self.request(4, &payload, 101).map(|_| ())
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    name: String,
    directory: bool,
    size: u64,
    modified: u32,
}
fn attributes(bytes: &mut &[u8]) -> Result<(bool, u64, u32)> {
    let flags = number(bytes)?;
    let size = if flags & 1 != 0 {
        (u64::from(number(bytes)?) << 32) | u64::from(number(bytes)?)
    } else {
        0
    };
    if flags & 2 != 0 {
        number(bytes)?;
        number(bytes)?;
    }
    let mode = if flags & 4 != 0 { number(bytes)? } else { 0 };
    let modified = if flags & 8 != 0 {
        number(bytes)?;
        number(bytes)?
    } else {
        0
    };
    if flags & 0x80000000 != 0 {
        for _ in 0..number(bytes)? {
            string(bytes)?;
            string(bytes)?;
        }
    }
    Ok((mode & 0o170000 == 0o040000, size, modified))
}
#[derive(Serialize)]
pub struct Listing {
    path: String,
    entries: Vec<Entry>,
}
fn list_remote(connection: &mut Connection, path: &str) -> Result<Listing> {
    let canonical = connection.path_request(16, path, 104)?;
    let mut bytes = canonical.as_slice();
    if number(&mut bytes)? != 1 {
        return Err("Invalid canonical path.".into());
    }
    let path = text(string(&mut bytes)?)?;
    let reply = connection.path_request(11, &path, 102)?;
    let handle = string(&mut reply.as_slice())?;
    let result = (|| {
        let mut entries = Vec::new();
        loop {
            let mut payload = Vec::new();
            put(&mut payload, &handle);
            // EOF is a normal end to directory enumeration.
            let reply = match connection.request(12, &payload, 104) {
                Err(e) if e.starts_with("SFTP 1:") => break,
                result => result?,
            };
            let mut bytes = reply.as_slice();
            for _ in 0..number(&mut bytes)? {
                let name = text(string(&mut bytes)?)?;
                string(&mut bytes)?;
                let (directory, size, modified) = attributes(&mut bytes)?;
                if name != "." && name != ".." {
                    entries.push(Entry {
                        name,
                        directory,
                        size,
                        modified,
                    });
                }
            }
        }
        Ok(Listing { path, entries })
    })();
    let closed = connection.close(&handle);
    result.and_then(|listing| closed.map(|()| listing))
}
#[tauri::command]
pub async fn sftp_connect(
    app: tauri::AppHandle,
    connections: State<'_, Connections>,
    id: String,
    host: ssh::Host,
    settings: ssh::Settings,
    on_key: Channel<host_key::Request>,
) -> Result<()> {
    ssh::valid_id(&id)?;
    let connections = connections.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let scratch = local::scratch()?;
        local::stage_session(&app, scratch.path(), &host.key_id)?;
        host_key::config(
            scratch.path(),
            &std::env::current_exe().map_err(|e| e.to_string())?,
        )?;
        let lookup_app = app.clone();
        let lookup_dir = scratch.path().to_path_buf();
        let save_app = app.clone();
        let save_dir = scratch.path().to_path_buf();
        let broker = host_key::Broker::start(
            Arc::new(AtomicBool::new(false)),
            move |host| crate::lookup_session_trust(&lookup_app, &lookup_dir, host),
            move |entry| crate::save_session_trust(&save_app, &save_dir, entry),
            move |request| on_key.send(request).map_err(|e| e.to_string()),
        )?;
        let mut args = ssh::args(&host, &settings, None, scratch.path())?;
        let destination = args
            .iter()
            .position(|a| a == "--")
            .ok_or("Missing SSH destination.")?;
        args.truncate(destination);
        args.retain(|a| a != "-tt");
        args.extend([
            "-o".into(),
            "BatchMode=yes".into(),
            "-s".into(),
            "--".into(),
            host.address,
            "sftp".into(),
        ]);
        let mut command = Command::new(ssh::program("ssh")?);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        broker.configure(&mut command);
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let input = child.stdin.take().ok_or("Missing SFTP input.")?;
        let output = child.stdout.take().ok_or("Missing SFTP output.")?;
        let child = Arc::new(Mutex::new(child));
        let approvals = broker.approvals.clone();
        let connection = Arc::new(Mutex::new(Connection {
            child: child.clone(),
            input,
            output,
            sequence: 0,
            _broker: broker,
            _scratch: scratch,
        }));
        {
            let mut map = connections.0.lock().map_err(|e| e.to_string())?;
            if map.contains_key(&id) {
                return Err("SFTP connection already exists.".into());
            }
            map.insert(
                id.clone(),
                Live {
                    connection: connection.clone(),
                    approvals,
                    child,
                },
            );
        }
        let result = (|| {
            let reply = connection
                .lock()
                .map_err(|e| e.to_string())?
                .packet(&[1, 0, 0, 0, 3])?;
            if reply[..5] != [2, 0, 0, 0, 3] {
                return Err("Server must support SFTP v3.".into());
            }
            Ok(())
        })();
        if result.is_err() {
            connections.0.lock().map_err(|e| e.to_string())?.remove(&id);
        }
        result
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn sftp_answer(
    connections: State<'_, Connections>,
    id: String,
    request_id: String,
    decision: host_key::Decision,
) -> Result<()> {
    // Approvals must remain accessible while the connection mutex is busy reading SSH.
    let map = connections.0.lock().map_err(|e| e.to_string())?;
    let connection = map.get(&id).ok_or("SFTP connection closed.")?;
    connection.approvals.answer(&request_id, decision)
}
#[tauri::command]
pub fn sftp_disconnect(connections: State<'_, Connections>, id: String) -> Result<()> {
    connections.0.lock().map_err(|e| e.to_string())?.remove(&id);
    Ok(())
}
#[tauri::command]
pub async fn sftp_list(
    connections: State<'_, Connections>,
    id: Option<String>,
    path: String,
) -> Result<Listing> {
    let connections = connections.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(id) = id {
            let connection = connections
                .0
                .lock()
                .map_err(|e| e.to_string())?
                .get(&id)
                .map(|live| live.connection.clone())
                .ok_or("SFTP connection closed.")?;
            return list_remote(&mut *connection.lock().map_err(|e| e.to_string())?, &path);
        }
        let path = if path.is_empty() {
            PathBuf::from(
                std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                    .ok_or("Home directory unavailable.")?,
            )
        } else {
            PathBuf::from(path)
        };
        let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
        let mut entries = Vec::new();
        for entry in fs::read_dir(&path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            entries.push(Entry {
                name: entry
                    .file_name()
                    .into_string()
                    .map_err(|_| "This filename is not UTF-8.")?,
                directory: meta.is_dir(),
                size: meta.len(),
                modified: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |t| t.as_secs() as u32),
            });
        }
        Ok(Listing {
            path: path.to_str().ok_or("This path is not UTF-8.")?.into(),
            entries,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn sftp_modify(
    connections: State<'_, Connections>,
    id: Option<String>,
    action: String,
    path: String,
    destination: String,
) -> Result<()> {
    let connections = connections.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(id) = id {
            let connection = connections
                .0
                .lock()
                .map_err(|e| e.to_string())?
                .get(&id)
                .map(|live| live.connection.clone())
                .ok_or("SFTP connection closed.")?;
            let mut connection = connection.lock().map_err(|e| e.to_string())?;
            let mut payload = Vec::new();
            put(&mut payload, path.as_bytes());
            let kind = match action.as_str() {
                "mkdir" => {
                    payload.extend(0u32.to_be_bytes());
                    14
                }
                "rename" => {
                    put(&mut payload, destination.as_bytes());
                    18
                }
                "delete" => 13,
                "rmdir" => 15,
                _ => return Err("Unknown file operation.".into()),
            };
            connection.request(kind, &payload, 101).map(|_| ())
        } else {
            match action.as_str() {
                "mkdir" => fs::create_dir(path),
                "rename" => {
                    if std::path::Path::new(&destination)
                        .try_exists()
                        .map_err(|e| e.to_string())?
                    {
                        return Err("The destination already exists.".into());
                    }
                    fs::rename(path, destination)
                }
                "delete" => fs::remove_file(path),
                "rmdir" => fs::remove_dir(path),
                _ => return Err("Unknown file operation.".into()),
            }
            .map_err(|e| e.to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn sftp_transfer(
    connections: State<'_, Connections>,
    id: String,
    local: String,
    remote: String,
    upload: bool,
) -> Result<()> {
    let connections = connections.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = connections
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .get(&id)
            .map(|live| live.connection.clone())
            .ok_or("SFTP connection closed.")?;
        let mut connection = connection.lock().map_err(|e| e.to_string())?;
        transfer(&mut connection, &local, &remote, upload)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn transfer(connection: &mut Connection, local: &str, remote: &str, upload: bool) -> Result<()> {
    // Exclusive creation on both ends prevents accidental replacement of existing files.
    let mut temporary = if upload {
        None
    } else {
        let parent = std::path::Path::new(local)
            .parent()
            .ok_or("Missing destination directory.")?;
        Some(tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?)
    };
    let mut source = if upload {
        Some(fs::File::open(local).map_err(|e| e.to_string())?)
    } else {
        None
    };
    let file = if let Some(source) = source.as_mut() {
        source
    } else {
        temporary.as_mut().unwrap().as_file_mut()
    };
    let mut payload = Vec::new();
    put(&mut payload, remote.as_bytes());
    payload.extend(if upload { 2u32 | 8 | 32 } else { 1u32 }.to_be_bytes());
    payload.extend(0u32.to_be_bytes());
    let reply = connection.request(3, &payload, 102)?;
    let handle = string(&mut reply.as_slice())?;
    let result = (|| {
        let mut offset = 0u64;
        loop {
            let mut payload = Vec::new();
            put(&mut payload, &handle);
            payload.extend(offset.to_be_bytes());
            if upload {
                let mut chunk = [0; 32768];
                let n = file.read(&mut chunk).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                put(&mut payload, &chunk[..n]);
                connection.request(6, &payload, 101)?;
                offset += n as u64;
            } else {
                payload.extend(32768u32.to_be_bytes());
                let reply = match connection.request(5, &payload, 103) {
                    Err(e) if e.starts_with("SFTP 1:") => break,
                    result => result?,
                };
                let chunk = string(&mut reply.as_slice())?;
                if chunk.is_empty() {
                    return Err("Server returned empty file data.".into());
                }
                file.write_all(&chunk).map_err(|e| e.to_string())?;
                offset += chunk.len() as u64;
            }
        }
        if !upload {
            file.sync_all().map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    let closed = connection.close(&handle);
    result.and(closed)?;
    if let Some(temporary) = temporary {
        temporary
            .persist_noclobber(local)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn shutdown(app: &tauri::AppHandle) {
    if let Ok(mut map) = app.state::<Connections>().0.lock() {
        map.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncated_packets_and_attributes_are_rejected() {
        assert!(number(&mut [0, 0, 0].as_slice()).is_err());
        assert!(string(&mut [0, 0, 0, 4, b'a'].as_slice()).is_err());
        assert!(attributes(&mut [0, 0, 0, 1].as_slice()).is_err());
        let mut bytes = Vec::new();
        bytes.extend(13u32.to_be_bytes());
        bytes.extend(12345678901u64.to_be_bytes());
        bytes.extend(0o040755u32.to_be_bytes());
        bytes.extend(10u32.to_be_bytes());
        bytes.extend(20u32.to_be_bytes());
        assert_eq!(
            attributes(&mut bytes.as_slice()).unwrap(),
            (true, 12345678901, 20)
        );
        assert!(text(vec![255]).is_err());
    }

    // Standard unit tests remain portable. This contract test uses an installed
    // OpenSSH subsystem directly and needs neither a daemon nor credentials.
    #[test]
    fn openssh_subsystem_contract() {
        let server = [
            "/usr/lib/ssh/sftp-server",
            "/usr/lib/openssh/sftp-server",
            "/usr/libexec/sftp-server",
        ]
        .into_iter()
        .find(|path| std::path::Path::new(path).is_file());
        let Some(server) = server else {
            eprintln!("Skipping OpenSSH subsystem contract: sftp-server is not installed.");
            return;
        };
        let scratch = tempfile::tempdir().unwrap();
        let broker = host_key::Broker::start(
            Arc::new(AtomicBool::new(false)),
            |_| Ok(String::new()),
            |_| Ok(()),
            |_| Ok(()),
        )
        .unwrap();
        let mut child = Command::new(server)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let mut connection = Connection {
            child: Arc::new(Mutex::new(child)),
            input,
            output,
            sequence: 0,
            _broker: broker,
            _scratch: scratch,
        };
        assert_eq!(
            connection.packet(&[1, 0, 0, 0, 3]).unwrap()[..5],
            [2, 0, 0, 0, 3]
        );
        let files = tempfile::tempdir().unwrap();
        let root = files.path().to_str().unwrap();
        let source = files.path().join("source");
        let remote = files.path().join("spaces 'quotes' [glob] 日本語.bin");
        let remote = remote.to_str().unwrap();
        let destination = files.path().join("download");
        let destination = destination.to_str().unwrap();
        let data: Vec<_> = (0..100_003).map(|n| (n % 256) as u8).collect();
        fs::write(&source, &data).unwrap();
        transfer(&mut connection, source.to_str().unwrap(), remote, true).unwrap();
        assert!(transfer(&mut connection, source.to_str().unwrap(), remote, true).is_err());
        let listing = list_remote(&mut connection, root).unwrap();
        assert!(
            listing
                .entries
                .iter()
                .any(|entry| entry.name == "spaces 'quotes' [glob] 日本語.bin"
                    && entry.size == data.len() as u64)
        );
        transfer(&mut connection, destination, remote, false).unwrap();
        assert_eq!(fs::read(destination).unwrap(), data);
        assert!(transfer(&mut connection, destination, remote, false).is_err());
        assert_eq!(fs::read(destination).unwrap(), data);
        let absent = files.path().join("missing");
        let failed = files.path().join("failed");
        assert!(
            transfer(
                &mut connection,
                failed.to_str().unwrap(),
                absent.to_str().unwrap(),
                false
            )
            .is_err()
        );
        assert!(!failed.exists());
        let mut payload = Vec::new();
        put(&mut payload, remote.as_bytes());
        put(&mut payload, absent.to_str().unwrap().as_bytes());
        connection.request(18, &payload, 101).unwrap();
        connection
            .path_request(13, absent.to_str().unwrap(), 101)
            .unwrap();
        let folder = files.path().join("folder");
        let folder = folder.to_str().unwrap();
        let mut payload = Vec::new();
        put(&mut payload, folder.as_bytes());
        payload.extend(0u32.to_be_bytes());
        connection.request(14, &payload, 101).unwrap();
        assert!(
            list_remote(&mut connection, root)
                .unwrap()
                .entries
                .iter()
                .any(|entry| entry.name == "folder" && entry.directory)
        );
        connection.path_request(15, folder, 101).unwrap();
        assert!(connection.path_request(13, folder, 101).is_err());
    }
}
