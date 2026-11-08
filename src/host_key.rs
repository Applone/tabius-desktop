//! OpenSSH host-key verification over an authenticated, per-session loopback channel.
use crate::{Result, parse_known, ssh};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::Command;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Clone, Serialize)]
pub struct Request {
    pub id: String,
    pub host: String,
    pub kind: String,
    pub fingerprint: String,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Abort,
    Once,
    Save,
}
#[derive(Serialize, Deserialize)]
struct Lookup {
    token: String,
    host: String,
    kind: String,
    key: String,
}
struct Pending {
    id: String,
    sender: mpsc::Sender<Decision>,
}
#[derive(Default)]
pub struct Approvals {
    pending: Mutex<Option<Pending>>,
    temporary: Mutex<HashMap<String, String>>,
    stopped: AtomicBool,
}
impl Approvals {
    pub fn clear_temporary(&self) {
        if let Ok(mut temporary) = self.temporary.lock() {
            temporary.clear();
        }
    }
    pub fn answer(&self, id: &str, decision: Decision) -> Result<()> {
        let mut pending = self.pending.lock().map_err(|e| e.to_string())?;
        if pending.as_ref().is_none_or(|p| p.id != id) {
            return Err("This host-key request is no longer pending.".into());
        }
        pending
            .take()
            .unwrap()
            .sender
            .send(decision)
            .map_err(|_| "Host-key verification has ended.".into())
    }
}
pub struct Broker {
    pub approvals: Arc<Approvals>,
    address: SocketAddr,
    token: String,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Broker {
    fn drop(&mut self) {
        self.approvals.stopped.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Broker {
    pub fn start(
        cancelled: Arc<AtomicBool>,
        lookup: impl Fn(&str) -> Result<String> + Send + 'static,
        save: impl Fn(&str) -> Result<()> + Send + 'static,
        notify: impl Fn(Request) -> Result<()> + Send + 'static,
    ) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        let token = Uuid::new_v4().to_string();
        let approvals = Arc::new(Approvals::default());
        let state = approvals.clone();
        let secret = token.clone();
        let worker = std::thread::spawn(move || {
            while !state.stopped.load(Ordering::Acquire) && !cancelled.load(Ordering::Acquire) {
                let (mut socket, _) = match listener.accept() {
                    Ok(client) => client,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(25));
                        continue;
                    }
                    Err(_) => break,
                };
                let response: Result<String> = (|| {
                    socket
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .map_err(|e| e.to_string())?;
                    socket
                        .set_write_timeout(Some(Duration::from_secs(2)))
                        .map_err(|e| e.to_string())?;
                    let request: Lookup = serde_json::from_reader((&mut socket).take(16 * 1024))
                        .map_err(|e| e.to_string())?;
                    if request.token != secret {
                        return Err("Invalid host-key channel token.".into());
                    }
                    validate_host(&request.host)?;
                    let known = lookup(&request.host)?;
                    // ORDER is a pre-handshake query so OpenSSH can prefer an already trusted key type.
                    if request.kind == "NONE" && request.key == "NONE" {
                        return Ok(known);
                    }
                    let entry = format!("{} {} {}", request.host, request.kind, request.key);
                    let key = parse_known(&entry)?;
                    if !known.is_empty() {
                        return Ok(known);
                    }
                    if let Some(entry) = state
                        .temporary
                        .lock()
                        .map_err(|e| e.to_string())?
                        .get(&request.host)
                    {
                        return Ok(entry.clone());
                    }
                    let id = Uuid::new_v4().to_string();
                    let (sender, receiver) = mpsc::channel();
                    *state.pending.lock().map_err(|e| e.to_string())? = Some(Pending {
                        id: id.clone(),
                        sender,
                    });
                    notify(Request {
                        id,
                        host: request.host.clone(),
                        kind: key.kind,
                        fingerprint: key.fingerprint,
                    })?;
                    let deadline = Instant::now() + Duration::from_secs(300);
                    loop {
                        if state.stopped.load(Ordering::Acquire)
                            || cancelled.load(Ordering::Acquire)
                            || Instant::now() >= deadline
                        {
                            return Err("Host-key verification cancelled or timed out.".into());
                        }
                        match receiver.recv_timeout(Duration::from_millis(50)) {
                            Ok(Decision::Abort) => {
                                cancelled.store(true, Ordering::Release);
                                return Err("Login aborted.".into());
                            }
                            Ok(decision) => {
                                // Another session may have trusted a different key while this dialog was open.
                                let known = lookup(&request.host)?;
                                if !known.is_empty() {
                                    return Ok(known);
                                }
                                if matches!(decision, Decision::Save) {
                                    save(&entry)?;
                                } else {
                                    state
                                        .temporary
                                        .lock()
                                        .map_err(|e| e.to_string())?
                                        .insert(request.host.clone(), entry.clone());
                                }
                                return Ok(entry);
                            }
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                            Err(mpsc::RecvTimeoutError::Disconnected) => {
                                return Err("Host-key verification ended.".into());
                            }
                        }
                    }
                })();
                if let Ok(mut pending) = state.pending.lock() {
                    *pending = None;
                }
                let _ = serde_json::to_writer(&mut socket, &response);
            }
        });
        Ok(Self {
            approvals,
            address,
            token,
            worker: Some(worker),
        })
    }
    pub fn configure(&self, command: &mut Command) {
        command
            .env("TABIUS_HOST_KEY_ADDRESS", self.address.to_string())
            .env("TABIUS_HOST_KEY_TOKEN", &self.token);
    }
}
fn validate_host(host: &str) -> Result<()> {
    if host.is_empty()
        || host.len() > 300
        || host.starts_with('-')
        || !host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-:[]".contains(&c))
    {
        return Err("Invalid host-key lookup name.".into());
    }
    Ok(())
}
pub fn lookup_file(file: &Path, host: &str) -> Result<String> {
    validate_host(host)?;
    let output = Command::new(ssh::program("ssh-keygen")?)
        .arg("-F")
        .arg(host)
        .arg("-f")
        .arg(file)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.code() == Some(1) {
        return Ok(String::new());
    }
    if !output.status.success() {
        return Err("Cannot read known-host fingerprints.".into());
    }
    Ok(String::from_utf8(output.stdout)
        .map_err(|e| e.to_string())?
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n"))
}
pub fn config(directory: &Path, executable: &Path) -> Result<()> {
    // OpenSSH splits this into argv directly; it does not invoke a shell.
    let executable = executable.to_string_lossy().replace('\\', "/");
    if executable.contains(['"', '\n', '\r']) {
        return Err("Unsupported executable path for host verification.".into());
    }
    crate::local::atomic_write(&directory.join("ssh_config"), format!("Host *\n  UserKnownHostsFile none\n  GlobalKnownHostsFile none\n  StrictHostKeyChecking yes\n  CheckHostIP no\n  UpdateHostKeys no\n  KnownHostsCommand \"{executable}\" --host-key %I %H %t %K\n").as_bytes())
}
/// Internal helper invoked by OpenSSH, before the desktop runtime starts.
pub fn helper() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(2).collect();
    if args.len() != 4 {
        return Err("Invalid host-key helper invocation.".into());
    }
    if args[0] != "HOSTNAME" && args[0] != "ORDER" {
        return Ok(());
    }
    let address: SocketAddr = std::env::var("TABIUS_HOST_KEY_ADDRESS")
        .map_err(|e| e.to_string())?
        .parse::<SocketAddr>()
        .map_err(|e| e.to_string())?;
    if !address.ip().is_loopback() {
        return Err("Host-key channel must be local.".into());
    }
    let request = Lookup {
        token: std::env::var("TABIUS_HOST_KEY_TOKEN").map_err(|e| e.to_string())?,
        host: args[1].clone(),
        kind: args[2].clone(),
        key: args[3].clone(),
    };
    let mut socket =
        TcpStream::connect_timeout(&address, Duration::from_secs(3)).map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_secs(310)))
        .map_err(|e| e.to_string())?;
    socket
        .set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut socket, &request).map_err(|e| e.to_string())?;
    socket
        .shutdown(Shutdown::Write)
        .map_err(|e| e.to_string())?;
    let result: Result<String> =
        serde_json::from_reader(socket.take(64 * 1024)).map_err(|e| e.to_string())?;
    let entry = result?;
    if !entry.is_empty() {
        writeln!(std::io::stdout(), "{entry}").map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::sync::atomic::AtomicUsize;

    fn public_key() -> String {
        let mut blob = Vec::new();
        blob.extend_from_slice(&11u32.to_be_bytes());
        blob.extend_from_slice(b"ssh-ed25519");
        blob.extend_from_slice(&32u32.to_be_bytes());
        blob.extend_from_slice(&[1u8; 32]);
        STANDARD.encode(blob)
    }
    fn request(broker: &Broker, token: String) -> std::thread::JoinHandle<Result<String>> {
        send_lookup(broker, token, "ssh-ed25519".into(), public_key())
    }
    fn send_lookup(
        broker: &Broker,
        token: String,
        kind: String,
        key: String,
    ) -> std::thread::JoinHandle<Result<String>> {
        let address = broker.address;
        std::thread::spawn(move || {
            let mut socket = TcpStream::connect(address).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            serde_json::to_writer(
                &mut socket,
                &Lookup {
                    token,
                    host: "example.com".into(),
                    kind,
                    key,
                },
            )
            .unwrap();
            socket.shutdown(Shutdown::Write).unwrap();
            serde_json::from_reader(socket).unwrap()
        })
    }
    #[test]
    fn key_order_queries_never_prompt_and_return_existing_trust() {
        for known in ["", "example.com ssh-ed25519 STORED"] {
            let broker = Broker::start(
                Arc::new(AtomicBool::new(false)),
                move |_| Ok(known.into()),
                |_| panic!("must not save"),
                |_| panic!("must not prompt"),
            )
            .unwrap();
            assert_eq!(
                send_lookup(&broker, broker.token.clone(), "NONE".into(), "NONE".into())
                    .join()
                    .unwrap()
                    .unwrap(),
                known
            );
        }
    }
    #[test]
    fn once_is_temporary_save_is_explicit_and_abort_cancels_login() {
        for decision in [Decision::Once, Decision::Save, Decision::Abort] {
            let saved = Arc::new(Mutex::new(Vec::<String>::new()));
            let output = saved.clone();
            let cancelled = Arc::new(AtomicBool::new(false));
            let (sender, receiver) = mpsc::channel();
            let broker = Broker::start(
                cancelled.clone(),
                |_| Ok(String::new()),
                move |entry| {
                    output.lock().unwrap().push(entry.into());
                    Ok(())
                },
                move |request| sender.send(request).map_err(|e| e.to_string()),
            )
            .unwrap();
            let response = request(&broker, broker.token.clone());
            let prompt = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(prompt.host, "example.com");
            assert!(prompt.fingerprint.starts_with("SHA256:"));
            assert!(broker.approvals.answer("wrong-request", decision).is_err());
            broker.approvals.answer(&prompt.id, decision).unwrap();
            assert!(broker.approvals.answer(&prompt.id, decision).is_err());
            let result = response.join().unwrap();
            if matches!(decision, Decision::Abort) {
                assert!(result.is_err());
                assert!(cancelled.load(Ordering::Acquire));
            } else {
                assert_eq!(
                    result.unwrap(),
                    format!("example.com ssh-ed25519 {}", public_key())
                );
                assert!(!cancelled.load(Ordering::Acquire));
            }
            assert_eq!(
                saved.lock().unwrap().len(),
                usize::from(matches!(decision, Decision::Save))
            );
            if matches!(decision, Decision::Once) {
                // Repeated OpenSSH checks in one connection reuse temporary trust.
                assert!(
                    request(&broker, broker.token.clone())
                        .join()
                        .unwrap()
                        .is_ok()
                );
                assert!(receiver.try_recv().is_err());
                broker.approvals.clear_temporary();
                let next = request(&broker, broker.token.clone());
                let prompt = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
                broker.approvals.answer(&prompt.id, Decision::Once).unwrap();
                assert!(next.join().unwrap().is_ok());
            }
        }
    }
    #[test]
    fn known_and_changed_keys_are_left_to_openssh_without_an_approval_bypass() {
        let broker = Broker::start(
            Arc::new(AtomicBool::new(false)),
            |_| Ok("example.com ssh-ed25519 EXISTING-KEY".into()),
            |_| panic!("must not save"),
            |_| panic!("must not prompt"),
        )
        .unwrap();
        assert_eq!(
            request(&broker, broker.token.clone())
                .join()
                .unwrap()
                .unwrap(),
            "example.com ssh-ed25519 EXISTING-KEY"
        );
        assert!(
            request(&broker, "wrong-token".into())
                .join()
                .unwrap()
                .is_err()
        );
    }
    #[test]
    fn concurrent_trust_changes_override_an_older_confirmation() {
        let lookups = AtomicUsize::new(0);
        let (sender, receiver) = mpsc::channel();
        let broker = Broker::start(
            Arc::new(AtomicBool::new(false)),
            move |_| {
                Ok(if lookups.fetch_add(1, Ordering::Relaxed) == 0 {
                    String::new()
                } else {
                    "example.com ssh-ed25519 NEW-TRUST".into()
                })
            },
            |_| panic!("must not overwrite new trust"),
            move |request| sender.send(request).map_err(|e| e.to_string()),
        )
        .unwrap();
        let response = request(&broker, broker.token.clone());
        let prompt = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        broker.approvals.answer(&prompt.id, Decision::Save).unwrap();
        assert_eq!(
            response.join().unwrap().unwrap(),
            "example.com ssh-ed25519 NEW-TRUST"
        );
    }
    #[test]
    fn closing_the_session_unblocks_pending_verification_and_storage_failures_block_login() {
        for fail_save in [false, true] {
            let (sender, receiver) = mpsc::channel();
            let broker = Broker::start(
                Arc::new(AtomicBool::new(false)),
                |_| Ok(String::new()),
                |_| Err("disk full".into()),
                move |request| sender.send(request).map_err(|e| e.to_string()),
            )
            .unwrap();
            let response = request(&broker, broker.token.clone());
            let prompt = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
            if fail_save {
                broker.approvals.answer(&prompt.id, Decision::Save).unwrap();
            } else {
                drop(broker);
            }
            assert!(response.join().unwrap().is_err());
        }
    }
    #[test]
    fn lookup_supports_hashed_hosts_and_ports_and_config_is_inherited_by_jump_hosts() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("known_hosts");
        let line = format!("[example.com]:2222 ssh-ed25519 {}", public_key());
        crate::local::atomic_write(&file, line.as_bytes()).unwrap();
        assert_eq!(lookup_file(&file, "[example.com]:2222").unwrap(), line);
        assert!(lookup_file(&file, "example.com").unwrap().is_empty());
        assert!(
            Command::new(ssh::program("ssh-keygen").unwrap())
                .args(["-H", "-f"])
                .arg(&file)
                .output()
                .unwrap()
                .status
                .success()
        );
        assert!(
            lookup_file(&file, "[example.com]:2222")
                .unwrap()
                .starts_with("|1|")
        );
        for invalid in ["", "-x", "host\nInjected", "host;bad"] {
            assert!(lookup_file(&file, invalid).is_err());
        }
        config(
            directory.path(),
            Path::new("/application with spaces/tabius"),
        )
        .unwrap();
        let output = Command::new(ssh::program("ssh").unwrap())
            .arg("-G")
            .arg("-F")
            .arg(directory.path().join("ssh_config"))
            .arg("example.com")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let settings = String::from_utf8(output.stdout).unwrap();
        assert!(settings.contains("stricthostkeychecking true"));
        assert!(settings.contains(
            "knownhostscommand \"/application with spaces/tabius\" --host-key %I %H %t %K"
        ));
        assert!(settings.contains("userknownhostsfile none"));
    }
}
