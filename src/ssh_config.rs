use crate::{Result, workspace};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::Manager;
use zeroize::Zeroizing;

const MAX_CONFIG: usize = 4 * 1024 * 1024;
const MAX_KEY: u64 = 128 * 1024;

enum Directive {
    Host(Vec<String>),
    Match,
    Include(Vec<Directive>),
    Option(String, Vec<String>),
}

struct Config<'a> {
    home: &'a Path,
    bytes: usize,
    files: usize,
    hash: Sha256,
    warnings: BTreeSet<String>,
}

fn words(line: &str) -> Result<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '#' if !quoted => break,
            c if c.is_ascii_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            '\\' if chars.peek().is_some_and(|c| "\\\"# \t".contains(*c)) => {
                word.push(chars.next().unwrap());
                started = true;
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return Err("Unterminated quoted value.".into());
    }
    if started {
        words.push(word);
    }
    Ok(words)
}

fn home_path(value: &str, home: &Path) -> Result<PathBuf> {
    if let Some(rest) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    {
        Ok(home.join(rest))
    } else if value.starts_with('~') {
        Err("Paths for another user's home directory are unsupported.".into())
    } else {
        Ok(PathBuf::from(value))
    }
}

impl Config<'_> {
    fn read(&mut self, path: &Path, stack: &mut Vec<PathBuf>) -> Result<Vec<Directive>> {
        let path = fs::canonicalize(path)
            .map_err(|e| format!("Cannot read SSH configuration {}: {e}", path.display()))?;
        if stack.contains(&path) || stack.len() >= 16 || self.files >= 128 {
            return Err(
                "SSH configuration includes are cyclic or exceed the import limits.".into(),
            );
        }
        let mut content = String::new();
        File::open(&path)
            .and_then(|file| {
                file.take(MAX_CONFIG as u64 + 1)
                    .read_to_string(&mut content)
            })
            .map_err(|e| format!("Cannot read SSH configuration {}: {e}", path.display()))?;
        self.bytes += content.len();
        self.files += 1;
        if self.bytes > MAX_CONFIG {
            return Err("SSH configuration exceeds the 4 MB import limit.".into());
        }
        self.hash.update(content.len().to_le_bytes());
        self.hash.update(content.as_bytes());
        stack.push(path.clone());
        let mut entries = Vec::new();
        for (index, line) in content.lines().enumerate() {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let split = line.find(|c: char| c.is_ascii_whitespace() || c == '=');
            let (keyword, rest) = split.map_or((line, ""), |split| line.split_at(split));
            let rest = rest.trim_start().strip_prefix('=').unwrap_or(rest).trim();
            let values =
                words(rest).map_err(|e| format!("{}:{}: {e}", path.display(), index + 1))?;
            if values.is_empty() {
                return Err(format!(
                    "{}:{}: Missing {keyword} value.",
                    path.display(),
                    index + 1
                ));
            }
            match keyword.to_ascii_lowercase().as_str() {
                "host" => entries.push(Directive::Host(values)),
                "match" => {
                    self.warnings.insert("Match blocks are skipped; their conditions and commands are not evaluated.".into());
                    entries.push(Directive::Match);
                }
                "include" => {
                    for value in values {
                        if value.contains('%') || value.contains("${") {
                            return Err("Include paths with tokens or environment variables are unsupported.".into());
                        }
                        let mut pattern = home_path(&value, self.home)?;
                        if !pattern.is_absolute() {
                            pattern = self.home.join(".ssh").join(pattern);
                        }
                        let paths = glob::glob(&pattern.to_string_lossy())
                            .map_err(|e| format!("Invalid SSH Include pattern: {e}"))?;
                        for included in paths {
                            let included =
                                included.map_err(|e| format!("Cannot expand SSH Include: {e}"))?;
                            entries.push(Directive::Include(self.read(&included, stack)?));
                        }
                    }
                }
                keyword => entries.push(Directive::Option(keyword.into(), values)),
            }
        }
        stack.pop();
        Ok(entries)
    }
}

fn matches(pattern: &str, host: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    let host = host.to_ascii_lowercase();
    let (pattern, host) = (pattern.as_bytes(), host.as_bytes());
    let (mut p, mut h, mut star, mut retry) = (0, 0, None, 0);
    while h < host.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == host[h]) {
            p += 1;
            h += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            retry = h;
        } else if let Some(start) = star {
            retry += 1;
            h = retry;
            p = start + 1;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|c| *c == b'*')
}

fn aliases(entries: &[Directive], result: &mut Vec<String>) {
    for entry in entries {
        match entry {
            Directive::Host(patterns) => {
                for alias in patterns {
                    if !alias.contains(['*', '?', '!'])
                        && !result.iter().any(|name| name.eq_ignore_ascii_case(alias))
                    {
                        result.push(alias.clone());
                    }
                }
            }
            Directive::Include(entries) => aliases(entries, result),
            _ => (),
        }
    }
}

fn options(
    entries: &[Directive],
    alias: &str,
    active: bool,
    out: &mut BTreeMap<String, Vec<String>>,
) -> bool {
    let mut active = active;
    let mut declared = false;
    for entry in entries {
        match entry {
            Directive::Host(patterns) => {
                active = patterns
                    .iter()
                    .any(|p| !p.starts_with('!') && matches(p, alias))
                    && !patterns
                        .iter()
                        .any(|p| p.strip_prefix('!').is_some_and(|p| matches(p, alias)));
                declared |= active && patterns.iter().any(|p| p.eq_ignore_ascii_case(alias));
            }
            Directive::Match => active = false,
            Directive::Include(entries) if active => {
                // Included Host/Match sections do not change the caller's active block.
                declared |= options(entries, alias, active, out);
            }
            Directive::Option(key, values) if active => {
                if key == "identityfile" {
                    out.entry(key.clone()).or_default().extend(values.clone());
                } else if key == "proxyjump" || key == "proxycommand" {
                    if !out.contains_key("proxyjump") && !out.contains_key("proxycommand") {
                        out.insert(key.clone(), values.clone());
                    }
                } else {
                    out.entry(key.clone()).or_insert_with(|| values.clone());
                }
            }
            _ => (),
        }
    }
    declared
}

fn value<'a>(options: &'a BTreeMap<String, Vec<String>>, key: &str) -> Result<Option<&'a str>> {
    match options.get(key).map(Vec::as_slice) {
        None => Ok(None),
        Some([value]) => Ok(Some(value)),
        _ => Err(format!("{key} requires one value.")),
    }
}

fn expand(value: &str, host: &Value, home: &Path) -> Result<String> {
    let mut result = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            result.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => result.push('%'),
            Some('d') => result.push_str(&home.to_string_lossy()),
            Some('h') => result.push_str(workspace::text(&host["address"])),
            Some('n') => result.push_str(workspace::text(&host["name"])),
            Some('p') => result.push_str(&host["port"].to_string()),
            Some('r') => result.push_str(workspace::text(&host["username"])),
            _ => return Err("Unsupported SSH path or hostname token.".into()),
        }
    }
    if result.contains("${") || result.contains('\0') {
        return Err("Environment variables and NUL bytes are unsupported in SSH paths.".into());
    }
    Ok(result)
}

fn host(alias: &str, options: &BTreeMap<String, Vec<String>>, home: &Path) -> Result<Value> {
    let mut host = workspace::new_item("host".into())?;
    host["name"] = json!(alias);
    host["address"] = json!(alias);
    if let Some(address) = value(options, "hostname")? {
        host["address"] = json!(expand(address, &host, home)?);
    }
    if let Some(user) = value(options, "user")? {
        host["username"] = json!(user);
    }
    if let Some(port) = value(options, "port")? {
        host["port"] = json!(port.parse::<u16>().map_err(|_| "Invalid SSH port.")?);
    }
    if let Some(agent) = value(options, "forwardagent")? {
        host["agentForwarding"] = json!(match agent.to_ascii_lowercase().as_str() {
            "yes" => true,
            "no" => false,
            _ => return Err("ForwardAgent socket paths are unsupported.".into()),
        });
    }
    if value(options, "proxycommand")?.is_some_and(|command| command != "none") {
        return Err("ProxyCommand cannot be imported; use an SSH ProxyJump instead.".into());
    }
    if let Some(jump) = value(options, "proxyjump")?
        && !jump.eq_ignore_ascii_case("none")
    {
        host["jump"] = json!(expand(jump, &host, home)?);
    }
    workspace::validate_host(&host)?;
    let native_host: crate::ssh::Host =
        serde_json::from_value(host.clone()).map_err(|e| e.to_string())?;
    let settings = serde_json::from_value(workspace::settings()).map_err(|e| e.to_string())?;
    crate::ssh::args(&native_host, &settings, None, home)?;
    Ok(host)
}

fn resolve_jumps(
    entries: &[Directive],
    jump: &str,
    home: &Path,
    visited: &mut HashSet<String>,
) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for hop in jump.split(',') {
        let (user, endpoint) = hop
            .rsplit_once('@')
            .map_or((None, hop), |(user, host)| (Some(user), host));
        let (alias, port) = if let Some(endpoint) = endpoint.strip_prefix('[') {
            let (address, suffix) = endpoint.split_once(']').ok_or("Invalid IPv6 jump host.")?;
            let port = if suffix.is_empty() {
                None
            } else {
                Some(suffix.strip_prefix(':').ok_or("Invalid jump host port.")?)
            };
            (address, port)
        } else if endpoint.matches(':').count() == 1 {
            let (alias, port) = endpoint.split_once(':').unwrap();
            (alias, Some(port))
        } else {
            (endpoint, None)
        };
        if visited.len() >= 16 || !visited.insert(alias.to_ascii_lowercase()) {
            return Err("Cyclic or excessive ProxyJump chain.".into());
        }
        let mut effective = BTreeMap::new();
        options(entries, alias, true, &mut effective);
        let mut hop = host(alias, &effective, home)?;
        if let Some(user) = user {
            hop["username"] = json!(user);
        }
        if let Some(port) = port {
            let port = port.parse::<u16>().map_err(|_| "Invalid jump host port.")?;
            if port == 0 {
                return Err("Invalid jump host port.".into());
            }
            hop["port"] = json!(port);
        }
        if !workspace::text(&hop["jump"]).is_empty() {
            result.extend(resolve_jumps(
                entries,
                workspace::text(&hop["jump"]),
                home,
                visited,
            )?);
        }
        let address = workspace::text(&hop["address"]);
        result.push(format!(
            "{}{}:{}",
            if workspace::text(&hop["username"]).is_empty() {
                String::new()
            } else {
                format!("{}@", workspace::text(&hop["username"]))
            },
            if address.contains(':') {
                format!("[{address}]")
            } else {
                address.to_owned()
            },
            hop["port"]
        ));
        visited.remove(&alias.to_ascii_lowercase());
        if result.len() > 16 {
            return Err("Excessive ProxyJump chain.".into());
        }
    }
    Ok(result)
}

struct Key {
    id: String,
    content: Zeroizing<String>,
    public: String,
}

fn read_key(path: &Path) -> Result<(Zeroizing<String>, String)> {
    let mut content = Zeroizing::new(String::new());
    File::open(path)
        .and_then(|file| file.take(MAX_KEY + 1).read_to_string(&mut content))
        .map_err(|e| format!("Cannot read identity {}: {e}", path.display()))?;
    if content.len() as u64 > MAX_KEY || !content.contains("PRIVATE KEY-----") {
        return Err(format!(
            "Identity {} is not a supported private key under 128 KB.",
            path.display()
        ));
    }
    let public = if let Some(public) = crate::openssh_public(&content)? {
        public
    } else if public_path(path).is_file() {
        let mut public = String::new();
        File::open(public_path(path))
            .and_then(|file| file.take(MAX_KEY + 1).read_to_string(&mut public))
            .map_err(|e| e.to_string())?;
        if public.len() as u64 > MAX_KEY {
            return Err("Public key exceeds 128 KB.".into());
        }
        public
    } else {
        let output = std::process::Command::new(crate::ssh::program("ssh-keygen")?)
            .args(["-y", "-P", "", "-f"])
            .arg(path)
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| format!("Cannot read identity public key: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "Identity {} needs its matching .pub file to import without decrypting it.",
                path.display()
            ));
        }
        String::from_utf8(output.stdout).map_err(|_| "Invalid public key encoding.")?
    };
    crate::public_fingerprint(&public)?;
    Ok((content, public.trim().into()))
}

fn public_path(path: &Path) -> PathBuf {
    let mut path = path.as_os_str().to_os_string();
    path.push(".pub");
    path.into()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    hosts: Vec<Value>,
    key_count: usize,
    skipped: usize,
    warnings: Vec<String>,
    fingerprint: String,
}

struct Plan {
    workspace: Value,
    keys: Vec<Key>,
    preview: Preview,
}

fn plan(mut workspace: Value, path: &str, home: &Path) -> Result<Plan> {
    let path = home_path(path.trim(), home)?;
    let source =
        fs::canonicalize(&path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
    let mut config = Config {
        home,
        bytes: 0,
        files: 0,
        hash: Sha256::new(),
        warnings: BTreeSet::new(),
    };
    config
        .hash
        .update(serde_json::to_vec(&workspace).map_err(|e| e.to_string())?);
    let entries = config.read(&source, &mut Vec::new())?;
    let mut names = Vec::new();
    aliases(&entries, &mut names);
    if names.len() > 1000 {
        return Err("SSH configuration exceeds the 1,000 host import limit.".into());
    }
    let mut hosts = Vec::new();
    let mut keys = Vec::new();
    let mut skipped = 0;
    let mut seen_paths = BTreeMap::<PathBuf, String>::new();
    let mut existing = workspace::list(&workspace["hosts"]);
    let mut key_entries = workspace::list(&workspace["keys"]);
    for alias in names {
        if existing.iter().any(|h| {
            workspace::text(&h["name"]).eq_ignore_ascii_case(&alias)
                || (h["sshConfigPath"] == source.to_string_lossy().as_ref()
                    && workspace::text(&h["sshConfigAlias"]).eq_ignore_ascii_case(&alias))
        }) {
            skipped += 1;
            continue;
        }
        let mut effective = BTreeMap::new();
        if !options(&entries, &alias, true, &mut effective) {
            continue;
        }
        let mut host = match host(&alias, &effective, home) {
            Ok(host) => host,
            Err(error) => {
                config.warnings.insert(format!("{alias}: skipped. {error}"));
                skipped += 1;
                continue;
            }
        };
        if !workspace::text(&host["jump"]).is_empty() {
            match resolve_jumps(
                &entries,
                workspace::text(&host["jump"]),
                home,
                &mut HashSet::from([alias.to_ascii_lowercase()]),
            ) {
                Ok(chain) => host["jump"] = json!(chain.join(",")),
                Err(error) => {
                    config.warnings.insert(format!("{alias}: skipped. {error}"));
                    skipped += 1;
                    continue;
                }
            }
            config.warnings.insert(format!("{alias}: jump-host authentication uses the SSH agent or default identity files; managed key assignments apply to the destination host."));
        }
        for keyword in effective.keys() {
            if ![
                "hostname",
                "user",
                "port",
                "forwardagent",
                "proxyjump",
                "proxycommand",
                "identityfile",
            ]
            .contains(&keyword.as_str())
            {
                config
                    .warnings
                    .insert(format!("{alias}: {keyword} is not imported."));
            }
        }
        let identity_files = if let Some(paths) = effective.get("identityfile") {
            paths.clone()
        } else {
            [
                "id_rsa",
                "id_ecdsa",
                "id_ecdsa_sk",
                "id_ed25519",
                "id_ed25519_sk",
                "id_mldsa44_ed25519",
            ]
            .into_iter()
            .map(|name| home.join(".ssh").join(name))
            .filter(|path| path.is_file())
            .map(|path| path.to_string_lossy().into_owned())
            .collect()
        };
        let has_identities = identity_files
            .iter()
            .any(|path| !path.eq_ignore_ascii_case("none"));
        let mut host_keys = Vec::new();
        for identity in identity_files {
            if identity.eq_ignore_ascii_case("none") {
                continue;
            }
            let result = (|| {
                let identity = expand(&identity, &host, home)?;
                let path = home_path(&identity, home)?;
                let path = fs::canonicalize(&path)
                    .map_err(|e| format!("Cannot open identity {}: {e}", path.display()))?;
                if let Some(id) = seen_paths.get(&path) {
                    return Ok(id.clone());
                }
                let (content, public) = read_key(&path)?;
                config.hash.update(content.len().to_le_bytes());
                config.hash.update(content.as_bytes());
                config.hash.update(public.as_bytes());
                let (kind, fingerprint) = crate::public_fingerprint(&public)?;
                let id = if let Some(key) = key_entries
                    .iter()
                    .find(|key| key["fingerprint"] == fingerprint)
                {
                    workspace::text(&key["id"]).to_owned()
                } else {
                    let id = workspace::id();
                    key_entries.push(json!({"id":id,"name":path.file_name().unwrap_or_default().to_string_lossy(),"kind":kind,"publicKey":public,"fingerprint":fingerprint}));
                    keys.push(Key {
                        id: id.clone(),
                        content,
                        public,
                    });
                    id
                };
                seen_paths.insert(path, id.clone());
                Ok::<_, String>(id)
            })();
            match result {
                Ok(id) if !host_keys.contains(&id) => host_keys.push(id),
                Ok(_) => (),
                Err(error) => {
                    config.warnings.insert(format!("{alias}: {error}"));
                }
            }
        }
        if has_identities && host_keys.is_empty() {
            config.warnings.insert(format!(
                "{alias}: skipped because no configured private key could be migrated."
            ));
            skipped += 1;
            continue;
        }
        if let Some(id) = host_keys.first() {
            host["keyId"] = json!(id);
        }
        if host_keys.len() > 1 {
            config.warnings.insert(format!("{alias}: all identity files will be migrated; the first key is assigned to this host. Select a different key in host settings if needed."));
        }
        host["sshConfigAlias"] = json!(alias);
        host["sshConfigPath"] = json!(source);
        existing.push(host.clone());
        hosts.push(host);
    }
    workspace["hosts"] = json!(existing);
    workspace["keys"] = json!(key_entries);
    let preview = Preview {
        hosts,
        key_count: keys.len(),
        skipped,
        warnings: config.warnings.into_iter().collect(),
        fingerprint: crate::vault::hex(&config.hash.finalize()),
    };
    Ok(Plan {
        workspace,
        keys,
        preview,
    })
}

fn apply(
    plan: Plan,
    directory: &Path,
    fingerprint: &str,
    save: impl FnOnce(&Value) -> Result<()>,
) -> Result<Value> {
    if plan.preview.fingerprint != fingerprint {
        return Err(
            "SSH configuration or keys changed. Read the configuration again before importing."
                .into(),
        );
    }
    crate::storage::directory(directory)?;
    let mut created = Vec::new();
    let result = (|| {
        for key in &plan.keys {
            for (path, content) in [
                (directory.join(&key.id), key.content.as_bytes()),
                (
                    directory.join(&key.id).with_extension("pub"),
                    key.public.as_bytes(),
                ),
            ] {
                let mut file = crate::storage::options()
                    .create_new(true)
                    .open(&path)
                    .map_err(|e| e.to_string())?;
                created.push(path);
                file.write_all(content)
                    .and_then(|()| file.sync_all())
                    .map_err(|e| e.to_string())?;
            }
        }
        save(&plan.workspace)?;
        Ok(
            json!({"workspace":plan.workspace,"imported":plan.preview.hosts.len(),"keyCount":plan.preview.key_count}),
        )
    })();
    if result.is_err() {
        for path in created {
            let _ = fs::remove_file(path);
        }
    }
    result
}

#[tauri::command]
pub async fn preview_ssh_config(
    app: tauri::AppHandle,
    workspace: Value,
    path: String,
) -> Result<Preview> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        plan(workspace, &path, &home).map(|plan| plan.preview)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn import_ssh_config(
    app: tauri::AppHandle,
    workspace: Value,
    path: String,
    fingerprint: String,
) -> Result<Value> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        if crate::load_workspace(app.clone())? != workspace {
            return Err(
                "The workspace changed. Read the configuration again before importing.".into(),
            );
        }
        let previous = workspace.clone();
        let plan = plan(workspace, &path, &home)?;
        let scratch = crate::local::scratch()?;
        let key_ids: Vec<_> = plan.keys.iter().map(|key| key.id.clone()).collect();
        apply(
            plan,
            &scratch.path().join("keys"),
            &fingerprint,
            |workspace| {
                if crate::load_workspace(app.clone())? != previous {
                    return Err(
                        "The workspace changed during import. Read the configuration again.".into(),
                    );
                }
                let mut created = Vec::new();
                let saved = (|| {
                    for id in &key_ids {
                        crate::persist_key(&app, &scratch.path().join("keys").join(id), id)?;
                        created.push(id.clone());
                    }
                    crate::save_workspace(app.clone(), workspace.clone())
                })();
                if let Err(error) = saved {
                    for id in created {
                        let _ = crate::delete_key(app.clone(), id);
                    }
                    return Err(error);
                }
                Ok(())
            },
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configuration(content: &str) -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir(home.path().join(".ssh")).unwrap();
        fs::write(home.path().join(".ssh/config"), content).unwrap();
        home
    }

    fn preview(home: &Path) -> Plan {
        plan(workspace::empty(), "~/.ssh/config", home).unwrap()
    }

    fn generate_key(path: &Path, password: &str) {
        let output = std::process::Command::new(crate::ssh::program("ssh-keygen").unwrap())
            .args(["-q", "-t", "ed25519", "-a", "1", "-N", password, "-f"])
            .arg(path)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "ssh-keygen fixture generation failed"
        );
    }

    #[test]
    fn imports_aliases_quotes_comments_equals_and_first_value_defaults() {
        let home = configuration(
            "\u{feff}User global\r\nHoSt app alternate\n HostName = \"server.example.com\" # comment\n Port=2200\n ForwardAgent yes\nHost app\n Port 9999\nHost v6\n HostName 2001:db8::1\nHost *\n Port 2022\n User ignored\n",
        );
        let plan = preview(home.path());
        assert_eq!(plan.preview.hosts.len(), 3);
        assert_eq!(plan.preview.hosts[0]["name"], "app");
        assert_eq!(plan.preview.hosts[1]["name"], "alternate");
        assert_eq!(plan.preview.hosts[0]["address"], "server.example.com");
        assert_eq!(plan.preview.hosts[0]["port"], 2200);
        assert_eq!(plan.preview.hosts[0]["agentForwarding"], true);
        assert_eq!(plan.preview.hosts[0]["username"], "global");
        assert_eq!(plan.preview.hosts[2]["address"], "2001:db8::1");
        assert_eq!(plan.preview.hosts[2]["port"], 2022);
        assert!(plan.preview.warnings.is_empty());
        assert_eq!(words("\"a#b\" # comment").unwrap(), ["a#b"]);
        assert_eq!(
            words(r#""C:\Users\Alice\key file""#).unwrap(),
            [r"C:\Users\Alice\key file"]
        );
    }

    #[test]
    fn supported_options_match_openssh_configuration_resolution() {
        crate::pty::test_with_timeout(|| {
            let home = configuration(
                "Include conf.d/*.conf\nHost a* !alternate\n Port 2200\nHost alpha alternate\n HostName server.example.com\n ForwardAgent yes\nHost *\n Port 2222\n User fallback\n",
            );
            let includes = home.path().join(".ssh/config files");
            fs::create_dir(&includes).unwrap();
            fs::write(includes.join("hosts.conf"), "Host alpha\n User alice\n").unwrap();
            // ssh -F still resolves relative includes under the real user's home.
            // Use an absolute, quoted path with forward slashes for SSH's glob syntax.
            let pattern = includes
                .join("*.conf")
                .to_str()
                .unwrap()
                .replace(std::path::MAIN_SEPARATOR, "/");
            let path = home.path().join(".ssh/config");
            let content = fs::read_to_string(&path)
                .unwrap()
                .replace("conf.d/*.conf", &format!("\"{pattern}\""));
            fs::write(&path, content).unwrap();
            let planned = preview(home.path());
            assert_eq!(planned.preview.hosts.len(), 2);
            for host in &planned.preview.hosts {
                let alias = workspace::text(&host["name"]);
                let output = std::process::Command::new(crate::ssh::program("ssh").unwrap())
                    .args(["-vvv", "-G", "-F"])
                    .arg(home.path().join(".ssh/config"))
                    .arg(alias)
                    .stdin(std::process::Stdio::null())
                    .output()
                    .unwrap();
                let diagnostics = String::from_utf8_lossy(&output.stderr);
                assert!(
                    output.status.success(),
                    "ssh -G failed for {alias}: {diagnostics}"
                );
                #[cfg(windows)]
                assert!(
                    diagnostics.contains("OpenSSH_for_Windows"),
                    "Expected native Windows OpenSSH, not an MSYS client: {diagnostics}"
                );
                let output = String::from_utf8(output.stdout).unwrap();
                let options: BTreeMap<_, _> = output
                    .lines()
                    .filter_map(|line| line.split_once(' '))
                    .collect();
                assert_eq!(
                    host["username"],
                    if alias == "alpha" {
                        "alice"
                    } else {
                        "fallback"
                    }
                );
                assert_eq!(
                    host["address"], options["hostname"],
                    "{alias}: {diagnostics}"
                );
                assert_eq!(host["username"], options["user"], "{alias}: {diagnostics}");
                assert_eq!(
                    host["port"],
                    options["port"].parse::<u16>().unwrap(),
                    "{alias}: {diagnostics}"
                );
                assert_eq!(
                    host["agentForwarding"],
                    options["forwardagent"] == "yes",
                    "{alias}: {diagnostics}"
                );
            }
        });
    }

    #[test]
    fn wildcard_defaults_and_negations_do_not_create_hosts() {
        let home = configuration(
            "Host prod* !prod-backup\n User deploy\nHost prod-web prod-backup\n HostName %h.example.com\nHost *\n User fallback\nHost excluded !excluded\n Port 3333\n",
        );
        let plan = preview(home.path());
        assert_eq!(plan.preview.hosts.len(), 2);
        assert_eq!(plan.preview.hosts[0]["username"], "deploy");
        assert_eq!(plan.preview.hosts[0]["address"], "prod-web.example.com");
        assert_eq!(plan.preview.hosts[1]["username"], "fallback");
        assert!(
            preview(configuration("Host *\n Port 2222\n").path())
                .preview
                .hosts
                .is_empty()
        );
    }

    #[test]
    fn includes_are_ordered_and_keep_the_callers_host_scope() {
        let home = configuration(
            "Include conf.d/*.conf missing/*.conf\nHost parent\n Include parent.conf\n Port 2222\nHost outside\n",
        );
        fs::create_dir(home.path().join(".ssh/conf.d")).unwrap();
        fs::write(
            home.path().join(".ssh/conf.d/20-last.conf"),
            "Host included\n User later\n",
        )
        .unwrap();
        fs::write(
            home.path().join(".ssh/conf.d/10-first.conf"),
            "Host included\n User first\n",
        )
        .unwrap();
        fs::write(
            home.path().join(".ssh/parent.conf"),
            "Host *\n User parent-user\n",
        )
        .unwrap();
        let plan = preview(home.path());
        assert_eq!(plan.preview.hosts.len(), 3);
        assert_eq!(plan.preview.hosts[0]["username"], "first");
        assert_eq!(plan.preview.hosts[1]["username"], "parent-user");
        assert_eq!(plan.preview.hosts[1]["port"], 2222);
        assert_eq!(plan.preview.hosts[2]["port"], 22);
        assert_eq!(plan.preview.hosts[2]["username"], "");
    }

    #[test]
    fn skips_conditions_without_executing_commands_and_reports_unsupported_options() {
        let home = configuration(
            "Host good\n HostName example.com\n Compression yes\nMatch exec \"this-command-must-never-run\"\n User conditional\nHost bad\n HostName bad;command\nHost invalid-port\n Port 0\nHost proxy\n ProxyCommand nc %h %p\nHost cyclic-a\n ProxyJump cyclic-b\nHost cyclic-b\n ProxyJump cyclic-a\n",
        );
        let plan = preview(home.path());
        assert_eq!(plan.preview.hosts.len(), 1);
        assert_eq!(plan.preview.hosts[0]["name"], "good");
        assert_eq!(plan.preview.hosts[0]["username"], "");
        assert_eq!(plan.preview.skipped, 5);
        assert!(
            plan.preview
                .warnings
                .iter()
                .any(|warning| warning.contains("Match blocks"))
        );
        assert!(
            plan.preview
                .warnings
                .iter()
                .any(|warning| warning.contains("compression"))
        );
    }

    #[test]
    fn resolves_jump_aliases_nested_hops_and_explicit_ipv6_endpoints() {
        let home = configuration(
            "Host target\n ProxyJump jump\nHost jump\n HostName bastion.example.com\n User alice\n Port 2222\n ProxyJump bob@[2001:db8::1]:2200\n",
        );
        let plan = preview(home.path());
        assert_eq!(
            plan.preview.hosts[0]["jump"],
            "bob@[2001:db8::1]:2200,alice@bastion.example.com:2222"
        );
        assert_eq!(plan.preview.hosts[1]["jump"], "bob@[2001:db8::1]:2200");
    }

    #[test]
    fn rejects_missing_malformed_cyclic_and_oversized_configuration() {
        let home = configuration("Include config\n");
        assert!(
            plan(workspace::empty(), "~/.ssh/config", home.path())
                .err()
                .unwrap()
                .contains("cyclic")
        );
        for text in ["Host\n", "Host \"unclosed\n", "Include ${HOME}/config\n"] {
            fs::write(home.path().join(".ssh/config"), text).unwrap();
            assert!(plan(workspace::empty(), "~/.ssh/config", home.path()).is_err());
        }
        fs::write(home.path().join(".ssh/config"), "#".repeat(MAX_CONFIG + 1)).unwrap();
        assert!(plan(workspace::empty(), "~/.ssh/config", home.path()).is_err());
        assert!(plan(workspace::empty(), "~/.ssh/missing", home.path()).is_err());
    }

    #[test]
    fn reimports_preserve_existing_hosts_even_after_renaming() {
        let home = configuration("Host app\n HostName original.example\n");
        let mut workspace = preview(home.path()).workspace;
        let id = workspace["hosts"][0]["id"].clone();
        workspace["hosts"][0]["name"] = json!("Renamed");
        workspace["hosts"][0]["address"] = json!("edited.example");
        let plan = plan(workspace.clone(), "~/.ssh/config", home.path()).unwrap();
        assert!(plan.preview.hosts.is_empty());
        assert_eq!(plan.preview.skipped, 1);
        assert_eq!(plan.workspace, workspace);
        assert_eq!(plan.workspace["hosts"][0]["id"], id);
    }

    #[test]
    fn migrates_encrypted_keys_deduplicates_and_keeps_secrets_out_of_metadata() {
        crate::pty::test_with_timeout(|| {
            let home = configuration("Host one two\n IdentityFile \"~/.ssh/key file\"\n");
            let source = home.path().join(".ssh/key file");
            generate_key(&source, "synthetic-test-password");
            let original = fs::read(&source).unwrap();
            // OpenSSH public identity can be read without a password or .pub file.
            fs::remove_file(public_path(&source)).unwrap();
            let planned = preview(home.path());
            assert_eq!(planned.preview.key_count, 1);
            assert_eq!(
                planned.preview.hosts[0]["keyId"],
                planned.preview.hosts[1]["keyId"]
            );
            let serialized = serde_json::to_string(&planned.preview).unwrap();
            assert!(!serialized.contains("PRIVATE KEY"));
            let directory = home.path().join("managed-keys");
            assert!(!directory.exists());
            let fingerprint = planned.preview.fingerprint.clone();
            let planned = preview(home.path());
            assert_eq!(planned.preview.fingerprint, fingerprint);
            let result = apply(planned, &directory, &fingerprint, |workspace| {
                assert!(!workspace.to_string().contains("PRIVATE KEY"));
                Ok(())
            })
            .unwrap();
            let key = &result["workspace"]["keys"][0];
            let stored = directory.join(workspace::text(&key["id"]));
            assert_eq!(fs::read(&stored).unwrap(), original);
            assert_eq!(fs::read(&source).unwrap(), original);
            assert!(public_path(&stored).is_file());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(&stored).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
            let mut previous = result["workspace"].clone();
            previous["hosts"] = json!([]);
            let again = plan(previous, "~/.ssh/config", home.path()).unwrap();
            assert_eq!(again.preview.key_count, 0);
            assert_eq!(again.workspace["keys"].as_array().unwrap().len(), 1);
            assert_eq!(again.preview.hosts[0]["keyId"], key["id"]);
        });
    }

    #[test]
    fn migrates_default_and_multiple_identities_and_reports_unreadable_keys() {
        crate::pty::test_with_timeout(|| {
            let home = configuration(
                "Host defaults\nHost explicit\n IdentityFile ~/.ssh/id_ed25519\n IdentityFile ~/.ssh/second.key\n IdentityFile ~/.ssh/missing\n",
            );
            generate_key(&home.path().join(".ssh/id_ed25519"), "");
            generate_key(&home.path().join(".ssh/second.key"), "");
            let plan = preview(home.path());
            assert_eq!(plan.preview.key_count, 2);
            assert_eq!(
                plan.preview.hosts[0]["keyId"],
                plan.preview.hosts[1]["keyId"]
            );
            assert!(
                plan.preview
                    .warnings
                    .iter()
                    .any(|warning| warning.contains("missing"))
            );
            assert!(
                plan.preview
                    .warnings
                    .iter()
                    .any(|warning| warning.contains("first key"))
            );
            assert_eq!(
                public_path(Path::new("second.key")),
                Path::new("second.key.pub")
            );
        });
    }

    #[test]
    fn encrypted_pem_keys_require_a_companion_public_key_and_are_copied_unchanged() {
        crate::pty::test_with_timeout(|| {
            let home = configuration("Host pem\n IdentityFile ~/.ssh/key.pem\n");
            let path = home.path().join(".ssh/key.pem");
            let output = std::process::Command::new(crate::ssh::program("ssh-keygen").unwrap())
                .args([
                    "-q",
                    "-t",
                    "ecdsa",
                    "-m",
                    "PEM",
                    "-N",
                    "synthetic-password",
                    "-f",
                ])
                .arg(&path)
                .stdin(std::process::Stdio::null())
                .output()
                .unwrap();
            assert!(output.status.success(), "PEM fixture generation failed");
            let planned = preview(home.path());
            assert_eq!(planned.preview.key_count, 1);
            assert_eq!(planned.keys[0].content.as_bytes(), fs::read(&path).unwrap());
            fs::remove_file(public_path(&path)).unwrap();
            let planned = preview(home.path());
            assert!(planned.preview.hosts.is_empty());
            assert_eq!(planned.preview.skipped, 1);
            assert!(
                planned
                    .preview
                    .warnings
                    .iter()
                    .any(|warning| warning.contains(".pub file"))
            );
        });
    }

    #[test]
    fn changed_sources_and_failed_saves_leave_no_partial_key_import() {
        crate::pty::test_with_timeout(|| {
            let home = configuration("Host one\n IdentityFile ~/.ssh/key\n");
            generate_key(&home.path().join(".ssh/key"), "");
            let first = preview(home.path());
            let fingerprint = first.preview.fingerprint.clone();
            let directory = home.path().join("managed");
            assert!(
                apply(first, &directory, &fingerprint, |_| Err(
                    "save failed".into()
                ))
                .is_err()
            );
            assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
            let changed = format!(
                "{}\n",
                fs::read_to_string(home.path().join(".ssh/key")).unwrap()
            );
            fs::write(home.path().join(".ssh/key"), changed).unwrap();
            assert!(apply(preview(home.path()), &directory, &fingerprint, |_| Ok(())).is_err());
            assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
            let planned = preview(home.path());
            let fingerprint = planned.preview.fingerprint.clone();
            let occupied = public_path(&directory.join(&planned.keys[0].id));
            fs::write(&occupied, "existing file").unwrap();
            assert!(apply(planned, &directory, &fingerprint, |_| Ok(())).is_err());
            assert_eq!(fs::read_to_string(occupied).unwrap(), "existing file");
            assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        });
    }
}
