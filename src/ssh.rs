use serde::Deserialize;
use std::path::{Path, PathBuf};

pub fn program(name: &str) -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        // Git Bash prepends its MSYS tools to PATH. Those tools interpret native
        // Windows paths differently, so use the required Windows OpenSSH Client.
        let root = std::env::var_os("SystemRoot").ok_or("SystemRoot is not set.")?;
        let program = PathBuf::from(root)
            .join("System32")
            .join("OpenSSH")
            .join(format!("{name}.exe"));
        if !program.is_file() {
            return Err(format!(
                "Windows OpenSSH Client is required: {} was not found. Enable the OpenSSH Client optional feature.",
                program.display()
            ));
        }
        Ok(program)
    }
    #[cfg(not(windows))]
    {
        Ok(PathBuf::from(name))
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    pub address: String,
    pub port: u16,
    pub username: String,
    pub key_id: String,
    pub agent_forwarding: bool,
    pub startup: String,
    pub jump: String,
    pub proxy: String,
    pub env: String,
    pub utf8: bool,
    pub mosh: bool,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub emulator: String,
    pub post_quantum: bool,
    #[serde(rename = "detectOS")]
    pub detect_os: bool,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Forward {
    pub kind: String,
    pub bind_address: String,
    pub bind_port: u16,
    pub destination: String,
    pub destination_port: u16,
}
pub fn address(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 253
        || value.starts_with('-')
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b":._-".contains(&c))
    {
        return Err("Enter a valid hostname or IP address.".into());
    }
    Ok(())
}
pub fn valid_id(value: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| "Invalid identifier.".into())
}
fn jump(value: &str) -> Result<(), String> {
    for hop in value.split(',') {
        if hop.is_empty()
            || hop.starts_with('-')
            || !hop
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-:@[]".contains(&c))
        {
            return Err("Jump hosts must use [user@]hostname[:port].".into());
        }
    }
    Ok(())
}
fn endpoint(value: &str) -> String {
    if value.contains(':') {
        format!("[{value}]")
    } else {
        value.into()
    }
}
pub fn args(
    host: &Host,
    settings: &Settings,
    forward: Option<&Forward>,
    data: &Path,
) -> Result<Vec<String>, String> {
    address(&host.address)?;
    if host.port == 0 {
        return Err("Port must be between 1 and 65535.".into());
    }
    if !host.username.is_empty()
        && (host.username.starts_with('-')
            || !host
                .username
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c)))
    {
        return Err("Invalid SSH username.".into());
    }
    if !["xterm-256color", "xterm", "linux", "vt100"].contains(&settings.emulator.as_str()) {
        return Err("Invalid terminal type.".into());
    }
    let mut args: Vec<String> = vec![
        "-F".into(),
        if cfg!(windows) { "NUL" } else { "/dev/null" }.into(),
        "-p".into(),
        host.port.to_string(),
    ];
    for option in [
        format!(
            "UserKnownHostsFile=\"{}\"",
            data.join("known_hosts").display()
        ),
        "GlobalKnownHostsFile=none".into(),
        "StrictHostKeyChecking=ask".into(),
        "HashKnownHosts=no".into(),
        "ServerAliveInterval=15".into(),
        "ServerAliveCountMax=3".into(),
        "ConnectTimeout=15".into(),
        "PermitLocalCommand=no".into(),
    ] {
        args.extend(["-o".into(), option]);
    }
    if host.agent_forwarding {
        args.push("-A".into());
    } else {
        args.push("-a".into());
    }
    if !settings.post_quantum {
        args.extend(["-o".into(), "KexAlgorithms=-mlkem*,sntrup*".into()]);
    }
    if !host.username.is_empty() {
        args.extend(["-l".into(), host.username.clone()]);
    }
    if !host.key_id.is_empty() {
        valid_id(&host.key_id)?;
        let path = data.join("keys").join(&host.key_id);
        if !path.is_file() {
            return Err("The selected private key is missing.".into());
        }
        args.extend([
            "-i".into(),
            path.to_string_lossy().into(),
            "-o".into(),
            "IdentitiesOnly=yes".into(),
        ]);
    }
    let hops: Vec<&str> = [host.proxy.as_str(), host.jump.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    if !hops.is_empty() {
        let hops = hops.join(",");
        jump(&hops)?;
        args.extend(["-J".into(), hops]);
    }
    for line in host.env.lines().filter(|s| !s.trim().is_empty()) {
        let (name, value) = line
            .split_once('=')
            .ok_or("Use NAME=value for environment variables.")?;
        if name.is_empty()
            || name.as_bytes()[0].is_ascii_digit()
            || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || value
                .chars()
                .any(|c| c.is_control() || c.is_whitespace() || c == '"' || c == '\'')
        {
            return Err("Environment names must be identifiers and values must have no whitespace, quotes, or control characters.".into());
        }
        args.extend(["-o".into(), format!("SetEnv={name}={value}")]);
    }
    if host.utf8 {
        args.extend(["-o".into(), "SetEnv=LANG=C.UTF-8".into()]);
    }
    if let Some(rule) = forward {
        if !["127.0.0.1", "::1"].contains(&rule.bind_address.as_str()) || rule.bind_port == 0 {
            return Err("Use a loopback bind address and nonzero port.".into());
        }
        let bind = format!("{}:{}", endpoint(&rule.bind_address), rule.bind_port);
        let specification = if rule.kind == "dynamic" {
            bind
        } else {
            address(&rule.destination)?;
            if rule.destination_port == 0 {
                return Err("Invalid destination port.".into());
            }
            format!(
                "{bind}:{}:{}",
                endpoint(&rule.destination),
                rule.destination_port
            )
        };
        let flag = match rule.kind.as_str() {
            "local" => "-L",
            "remote" => "-R",
            "dynamic" => "-D",
            _ => return Err("Invalid forwarding type.".into()),
        };
        args.extend([
            "-N".into(),
            "-o".into(),
            "ExitOnForwardFailure=yes".into(),
            flag.into(),
            specification,
        ]);
    } else {
        args.push("-tt".into());
    }
    args.extend(["--".into(), host.address.clone()]);
    if forward.is_none() && (settings.detect_os || !host.startup.is_empty()) {
        if host.startup.contains('\0') || host.startup.len() > 128 * 1024 {
            return Err("Invalid startup snippet.".into());
        }
        let mut command = String::new();
        if settings.detect_os {
            command.push_str("printf '\\033]777;tabius-os=%s\\007' \"$(uname -s 2>/dev/null)\"; ");
        }
        if !host.startup.is_empty() {
            command.push_str(&host.startup);
            command.push('\n');
        }
        command.push_str("exec \"${SHELL:-/bin/sh}\" -l");
        args.push(command);
    }
    Ok(args)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn host() -> Host {
        Host {
            address: "example.com".into(),
            port: 22,
            username: "alice".into(),
            key_id: String::new(),
            agent_forwarding: false,
            startup: String::new(),
            jump: String::new(),
            proxy: String::new(),
            env: String::new(),
            utf8: true,
            mosh: false,
        }
    }
    fn settings() -> Settings {
        Settings {
            emulator: "xterm-256color".into(),
            post_quantum: true,
            detect_os: false,
        }
    }
    #[test]
    fn session_settings_accept_workspace_field_names() {
        for detect_os in [true, false] {
            let mut payload = crate::workspace::settings();
            payload["detectOS"] = serde_json::json!(detect_os);
            let settings: Settings = serde_json::from_value(payload).unwrap();
            assert_eq!(settings.emulator, "xterm-256color");
            assert!(settings.post_quantum);
            assert_eq!(settings.detect_os, detect_os);
            let arguments = args(&host(), &settings, None, Path::new("/tmp/data")).unwrap();
            assert_eq!(
                arguments.iter().any(|arg| arg.contains("tabius-os=")),
                detect_os
            );
        }
    }
    #[test]
    fn rejects_options_and_shell_characters() {
        for value in [
            "-oProxyCommand=bad",
            "a;touch /tmp/x",
            "$(id)",
            "user@host",
            "a\nb",
            "",
        ] {
            assert!(address(value).is_err());
        }
        assert!(address("2001:db8::1").is_ok());
    }
    #[test]
    fn preserves_security_and_disables_pq_explicitly() {
        let mut s = settings();
        s.post_quantum = false;
        let result = args(&host(), &s, None, Path::new("/tmp/data")).unwrap();
        assert!(result.contains(&"StrictHostKeyChecking=ask".into()));
        assert!(result.contains(&"KexAlgorithms=-mlkem*,sntrup*".into()));
        assert!(result.windows(2).any(|a| a == ["--", "example.com"]));
    }
    #[test]
    fn validates_jump_and_environment() {
        let mut h = host();
        h.jump = "alice@bastion:22".into();
        h.env = "APP_ENV=production".into();
        assert!(args(&h, &settings(), None, Path::new("/tmp")).is_ok());
        h.jump = "bastion;id".into();
        assert!(args(&h, &settings(), None, Path::new("/tmp")).is_err());
        h.jump.clear();
        h.env = "A=one two".into();
        assert!(args(&h, &settings(), None, Path::new("/tmp")).is_err());
    }
    #[test]
    fn all_forward_modes_and_ipv6() {
        for (kind, flag) in [("local", "-L"), ("remote", "-R"), ("dynamic", "-D")] {
            let mut f = Forward {
                kind: kind.into(),
                bind_address: "::1".into(),
                bind_port: 8080,
                destination: "::1".into(),
                destination_port: 80,
            };
            let result = args(&host(), &settings(), Some(&f), Path::new("/tmp")).unwrap();
            assert!(result.contains(&flag.into()));
            assert!(result.contains(&"-N".into()));
            assert!(!result.contains(&"-tt".into()));
            f.bind_address = "0.0.0.0".into();
            assert!(args(&host(), &settings(), Some(&f), Path::new("/tmp")).is_err());
        }
    }
}
