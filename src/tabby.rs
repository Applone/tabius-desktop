use crate::{
    Result,
    vault::{self, Encryption, hex},
    workspace::{self, list, text},
};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256, Sha512};
use std::collections::HashSet;
const LIMIT: usize = 8 * 1024 * 1024;
fn object(value: &Value) -> Result<()> {
    if value.is_object() {
        Ok(())
    } else {
        Err("Invalid Tabby object.".into())
    }
}
fn rows(value: &Value) -> Result<Vec<Value>> {
    if value.is_null() {
        Ok(vec![])
    } else if let Some(rows) = value.as_array() {
        for row in rows {
            object(row)?;
        }
        Ok(rows.clone())
    } else {
        Err("Invalid Tabby list.".into())
    }
}
fn decode(value: &str) -> Result<Vec<u8>> {
    STANDARD
        .decode(value)
        .or_else(|_| STANDARD_NO_PAD.decode(value))
        .map_err(|_| "Invalid base64 in the Tabby vault.".into())
}
pub fn merge(
    local: &[Value],
    remote: &[Value],
    baseline: &[Value],
    fields: bool,
) -> Result<Vec<Value>> {
    for rows in [local, remote, baseline] {
        let mut ids = HashSet::new();
        for row in rows {
            if text(&row["id"]).is_empty() || !ids.insert(text(&row["id"])) {
                return Err("Duplicate or missing sync IDs.".into());
            }
        }
    }
    let mut ids = Vec::new();
    for row in remote.iter().chain(local).chain(baseline) {
        if !ids.contains(&row["id"]) {
            ids.push(row["id"].clone());
        }
    }
    let mut result = Vec::new();
    for id in ids {
        let a = local.iter().find(|v| v["id"] == id);
        let c = remote.iter().find(|v| v["id"] == id);
        let d = baseline.iter().find(|v| v["id"] == id);
        let value = if a == c || c == d {
            a.cloned()
        } else if a == d {
            c.cloned()
        } else if fields && let (Some(a), Some(c), Some(d)) = (a, c, d) {
            let mut merged = c.clone();
            for key in c.as_object().ok_or("Invalid sync row.")?.keys() {
                merged[key] = if a[key] == c[key] || c[key] == d[key] {
                    a[key].clone()
                } else if a[key] == d[key] {
                    c[key].clone()
                } else {
                    return Err(format!(
                        "Sync conflict in {}: both copies changed {key}. Nothing was uploaded.",
                        text(&id)
                    ));
                };
            }
            Some(merged)
        } else {
            return Err(format!(
                "Sync conflict in {}: both copies changed. Nothing was uploaded.",
                text(&id)
            ));
        };
        if let Some(value) = value {
            result.push(value);
        }
    }
    Ok(result)
}
fn remote_port(value: &Value) -> Result<u64> {
    if value.is_null() {
        return Ok(22);
    }
    value
        .as_u64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|p| (1..=65535).contains(p))
        .ok_or("Invalid Tabby SSH port.".into())
}
pub struct Document {
    pub root: Value,
    pub config: Value,
    pub vault: Option<Value>,
}
impl Document {
    pub fn open(content: &str, password: &str) -> Result<Self> {
        if content.len() > LIMIT {
            return Err("Tabby configuration exceeds 8 MB.".into());
        }
        let yaml: serde_yaml_ng::Value =
            serde_yaml_ng::from_str(content).map_err(|_| "Invalid Tabby YAML configuration.")?;
        let root: Value =
            serde_json::to_value(yaml).map_err(|_| "Invalid Tabby YAML configuration.")?;
        let root = if root.is_null() { json!({}) } else { root };
        object(&root)?;
        if root["vault"]["version"] == 2 && root["encrypted"] != true {
            return Err("Modern vaults must encrypt the entire configuration.".into());
        }
        let vault = if root["vault"].is_null() {
            None
        } else {
            Some(vault::decrypt(&root["vault"], password)?)
        };
        let config = if root["encrypted"] == true {
            vault
                .as_ref()
                .ok_or("Encrypted Tabby configuration has no vault.")?["config"]
                .clone()
        } else {
            root.clone()
        };
        object(&config)?;
        Ok(Self {
            root,
            config,
            vault,
        })
    }
    pub fn profiles(&self) -> Result<Vec<Value>> {
        let mut result = Vec::new();
        for p in rows(&self.config["profiles"])? {
            if p["type"] != "ssh" || p["isTemplate"] == true {
                continue;
            }
            let o = &p["options"];
            if !o.is_null() {
                object(o)?;
            }
            let address = text(&o["host"]);
            if address.is_empty() {
                continue;
            }
            if text(&p["id"]).is_empty() {
                return Err("A Tabby SSH profile has no ID.".into());
            }
            for key in ["proxyCommand", "httpProxyHost", "socksProxyHost"] {
                if !o[key].is_null() && o[key] != false && o[key] != "" {
                    return Err(format!(
                        "{} uses an unsupported {key}. Nothing was uploaded.",
                        text(&p["name"])
                    ));
                }
            }
            let refs = if o["privateKeys"].is_null() {
                vec![]
            } else {
                o["privateKeys"]
                    .as_array()
                    .ok_or("Invalid private key references.")?
                    .clone()
            };
            if refs.iter().any(|r| !r.is_string()) {
                return Err("Invalid private key references.".into());
            }
            let key = refs.first().cloned().unwrap_or(Value::Null);
            let port = remote_port(&o["port"])?;
            let profile = json!({"id":p["id"],"name":p["name"].as_str().unwrap_or(address),"address":address,"port":port,"username":o["user"].as_str().unwrap_or("root"),"authentication":o["auth"].as_str().unwrap_or(if key.is_null(){"password"}else{"publicKey"}),"keyRef":key});
            workspace::validate_host(&profile)?;
            result.push(profile);
        }
        merge(&result, &result, &[], false)?;
        Ok(result)
    }
    pub fn keys(&self) -> Result<Vec<Value>> {
        let mut result = vec![];
        let mut ids = HashSet::new();
        for entry in rows(&self.vault.as_ref().unwrap_or(&Value::Null)["secrets"])? {
            if entry["type"] != "file" {
                continue;
            }
            object(&entry["key"])?;
            if !entry["tabiusDesktop"].is_null() {
                object(&entry["tabiusDesktop"])?;
            }
            let id = text(&entry["key"]["id"]);
            if id.is_empty() || !ids.insert(id.to_owned()) {
                return Err("Duplicate or missing vault file IDs.".into());
            }
            let bytes = decode(text(&entry["value"]))?;
            if bytes.len() > 128 * 1024 {
                return Err("A vault key exceeds 128 KB.".into());
            }
            if let Ok(secret) = String::from_utf8(bytes)
                && secret.contains("PRIVATE KEY")
            {
                result.push(json!({"id":format!("file:{id}"),"name":entry["key"]["description"].as_str().unwrap_or("SSH key"),"secret":secret,"publicKey":text(&entry["tabiusDesktop"]["publicKey"])}));
            }
        }
        Ok(result)
    }
    pub fn passphrase(&self, secret: &str) -> Result<String> {
        let hash = hex(&Sha512::digest(secret.as_bytes()));
        Ok(
            rows(&self.vault.as_ref().unwrap_or(&Value::Null)["secrets"])?
                .iter()
                .find(|v| v["type"] == "ssh:key-passphrase" && v["key"]["hash"] == hash)
                .map(|v| text(&v["value"]).to_owned())
                .unwrap_or_default(),
        )
    }
    pub fn fingerprints(&self) -> Result<Vec<Value>> {
        if !self.config["ssh"].is_null() {
            object(&self.config["ssh"])?;
        }
        rows(&self.config["ssh"]["knownHosts"])?.into_iter().map(|f| {
            let host=text(&f["host"]).to_lowercase(); let port=f["port"].as_u64().unwrap_or(22); workspace::validate_host(&json!({"name":host,"address":host,"port":port}))?;
            let digest=decode(text(&f["digest"]))?; if digest.len()!=32 || text(&f["type"]).is_empty(){return Err("Invalid Tabby host fingerprint.".into());}
            Ok(json!({"id":format!("{host}|{port}|{}",text(&f["type"])),"host":host,"port":port,"type":f["type"],"digest":STANDARD.encode(digest)}))
        }).collect()
    }
    pub fn replace(
        &mut self,
        profiles: &[Value],
        baseline: &[Value],
        keys: &[Value],
        key_baseline: &[Value],
        fingerprints: &[Value],
    ) -> Result<()> {
        let original = rows(&self.config["profiles"])?;
        let mut kept: Vec<_> = original
            .iter()
            .filter(|p| {
                p["type"] != "ssh"
                    || p["isTemplate"] == true
                    || !profiles.iter().chain(baseline).any(|v| v["id"] == p["id"])
            })
            .cloned()
            .collect();
        for p in profiles {
            let mut previous = original
                .iter()
                .find(|v| v["id"] == p["id"])
                .cloned()
                .unwrap_or(json!({"options":{}}));
            if previous["options"].is_null() {
                previous["options"] = json!({});
            }
            object(&previous["options"])?;
            for (a, b) in [
                ("host", "address"),
                ("port", "port"),
                ("user", "username"),
                ("auth", "authentication"),
            ] {
                previous["options"][a] = p[b].clone();
            }
            if previous["options"]["privateKeys"][0] != p["keyRef"] {
                if p["keyRef"].is_null() {
                    previous["options"]
                        .as_object_mut()
                        .unwrap()
                        .remove("privateKeys");
                } else {
                    previous["options"]["privateKeys"] = json!([p["keyRef"]]);
                }
            }
            previous["id"] = p["id"].clone();
            previous["name"] = p["name"].clone();
            previous["type"] = json!("ssh");
            kept.push(previous);
        }
        self.config["profiles"] = json!(kept);
        if let Some(vault) = &mut self.vault {
            let original = rows(&vault["secrets"])?;
            let mut kept: Vec<_> = original
                .iter()
                .filter(|v| {
                    v["type"] != "file"
                        || !keys
                            .iter()
                            .chain(key_baseline)
                            .any(|k| text(&k["id"]) == format!("file:{}", text(&v["key"]["id"])))
                })
                .cloned()
                .collect();
            for k in keys {
                let id = text(&k["id"])
                    .strip_prefix("file:")
                    .ok_or("Invalid key ID.")?;
                let mut v = original
                    .iter()
                    .find(|v| v["type"] == "file" && v["key"]["id"] == id)
                    .cloned()
                    .unwrap_or(json!({"key":{},"tabiusDesktop":{}}));
                if v["tabiusDesktop"].is_null() {
                    v["tabiusDesktop"] = json!({});
                }
                v["type"] = json!("file");
                v["key"]["id"] = json!(id);
                v["key"]["description"] = k["name"].clone();
                v["value"] = json!(STANDARD.encode(text(&k["secret"])));
                v["tabiusDesktop"]["publicKey"] = k["publicKey"].clone();
                kept.push(v);
            }
            vault["secrets"] = json!(kept);
        }
        if self.config["ssh"].is_null() {
            self.config["ssh"] = json!({});
        }
        object(&self.config["ssh"])?;
        self.config["ssh"]["knownHosts"]=json!(fingerprints.iter().map(|f|json!({"host":f["host"],"port":f["port"],"type":f["type"],"digest":f["digest"]})).collect::<Vec<_>>());
        Ok(())
    }
    fn prepare_encryption(&mut self, encryption: Encryption) -> Result<()> {
        if encryption == Encryption::Tabby {
            if self.root["vault"]["version"] == 2 {
                return Err(
                    "A modern vault cannot be downgraded to Tabby compatibility encryption.".into(),
                );
            }
        } else {
            // Modern sync always protects profiles and fingerprints as well as secrets.
            self.vault.get_or_insert_with(|| json!({"secrets":[]}));
            let config = self
                .config
                .as_object_mut()
                .ok_or("Invalid configuration.")?;
            config.remove("vault");
            config.remove("encrypted");
            self.root = json!({"encrypted":true});
        }
        Ok(())
    }
    pub fn serialize(&mut self, password: &str, encryption: Encryption) -> Result<String> {
        self.prepare_encryption(encryption)?;
        if let Some(vault) = &mut self.vault {
            if self.root["encrypted"] == true {
                vault["config"] = self.config.clone();
            }
            let envelope = vault::encrypt(vault, password, encryption)?;
            if self.root["encrypted"] == true {
                self.root["vault"] = envelope;
            } else {
                self.config["vault"] = envelope;
            }
        }
        let content = serde_yaml_ng::to_string(if self.root["encrypted"] == true {
            &self.root
        } else {
            &self.config
        })
        .map_err(|e| e.to_string())?;
        if content.len() > LIMIT {
            return Err("Tabby configuration exceeds 8 MB.".into());
        }
        Ok(content)
    }
}
pub fn native_fingerprints(known: &[Value]) -> Result<Vec<Value>> {
    let mut result: Vec<Value> = vec![];
    for entry in known {
        for name in text(&entry["host"]).split(',') {
            if name.starts_with(['|', '@']) {
                continue;
            }
            let (host, port) = if let Some((host, port)) =
                name.strip_prefix('[').and_then(|s| s.split_once("]:"))
            {
                (host, port.parse::<u64>().map_err(|e| e.to_string())?)
            } else {
                (name, 22)
            };
            let host = host.to_lowercase();
            let kind = text(&entry["kind"]);
            let digest = STANDARD.encode(decode(
                text(&entry["fingerprint"]).trim_start_matches("SHA256:"),
            )?);
            let id = format!("{host}|{port}|{kind}");
            let f = json!({"id":id,"host":host,"port":port,"type":kind,"digest":digest});
            if let Some(old) = result.iter_mut().find(|v| v["id"] == id) {
                *old = f;
            } else {
                result.push(f);
            }
        }
    }
    Ok(result)
}
fn key_id(key: &Value) -> String {
    key["syncId"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("file:{}", text(&key["id"])))
}
fn profile_id(host: &Value) -> String {
    host["syncId"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("ssh:tabius:{}", text(&host["id"])))
}
fn summary(key: &Value) -> Value {
    json!({"id":key["id"],"name":key["name"],"digest":hex(&Sha256::digest(text(&key["secret"]).as_bytes()))})
}
pub fn configuration(response: &crate::sync::Response, id: u64) -> Result<Value> {
    let c = &response.body;
    if c["id"] != id || !c["content"].is_string() {
        return Err("Sync server returned a different or empty configuration.".into());
    }
    Ok(c.clone())
}

trait Services {
    async fn request(
        &self,
        server: String,
        token: String,
        id: Option<u64>,
        content: Option<String>,
        etag: Option<String>,
    ) -> Result<crate::sync::Response>;
    fn read_keys(&self, ids: Vec<String>) -> Result<Vec<crate::SyncKey>>;
    async fn import_key(
        &self,
        name: String,
        secret: String,
        public_key: String,
        passphrase: String,
    ) -> Result<crate::KeyEntry>;
    fn delete_key(&self, id: String) -> Result<()>;
    fn known_hosts(&self) -> Result<Vec<crate::KnownHost>>;
    fn load_workspace(&self) -> Result<Value>;
}
struct NativeServices(tauri::AppHandle);
impl Services for NativeServices {
    async fn request(
        &self,
        server: String,
        token: String,
        id: Option<u64>,
        content: Option<String>,
        etag: Option<String>,
    ) -> Result<crate::sync::Response> {
        crate::sync::tabby_request(server, token, id, content, etag).await
    }
    fn read_keys(&self, ids: Vec<String>) -> Result<Vec<crate::SyncKey>> {
        crate::read_sync_keys(self.0.clone(), ids)
    }
    async fn import_key(
        &self,
        name: String,
        secret: String,
        public_key: String,
        passphrase: String,
    ) -> Result<crate::KeyEntry> {
        crate::import_sync_key(self.0.clone(), name, secret, public_key, passphrase).await
    }
    fn delete_key(&self, id: String) -> Result<()> {
        crate::delete_key(self.0.clone(), id)
    }
    fn known_hosts(&self) -> Result<Vec<crate::KnownHost>> {
        crate::known_hosts(self.0.clone())
    }
    fn load_workspace(&self) -> Result<Value> {
        crate::load_workspace(self.0.clone())
    }
}
async fn synchronize(
    services: &impl Services,
    mut workspace: Value,
    server: String,
    token: String,
    id: u64,
    password: String,
    encryption: Encryption,
) -> Result<Value> {
    if encryption == Encryption::Modern && password.is_empty() {
        return Err("Enter the vault password.".into());
    }
    let previous = workspace["sync"].clone();
    let baseline = if previous["server"] == server && previous["configurationId"] == id {
        previous
    } else {
        Value::Null
    };
    let response = services
        .request(server.clone(), token.clone(), Some(id), None, None)
        .await?;
    let remote = configuration(&response, id)?;
    let mut document = Document::open(text(&remote["content"]), &password)?;
    if baseline["encryption"] == "modern" && document.root["vault"]["version"] != 2 {
        return Err(
            "The remote configuration lost its modern encryption. Nothing was uploaded.".into(),
        );
    }
    document.prepare_encryption(encryption)?;
    let mut staged = Vec::new();
    let result = synchronize_inner(
        services,
        &mut workspace,
        server,
        token,
        id,
        &password,
        &baseline,
        &response,
        &remote,
        &mut document,
        &mut staged,
        encryption,
    )
    .await;
    if result.is_err() {
        for id in staged {
            let _ = services.delete_key(id);
        }
    }
    result
}
#[allow(clippy::too_many_arguments)]
async fn synchronize_inner(
    services: &impl Services,
    workspace: &mut Value,
    server: String,
    token: String,
    id: u64,
    password: &str,
    baseline: &Value,
    response: &crate::sync::Response,
    remote: &Value,
    document: &mut Document,
    staged: &mut Vec<String>,
    encryption: Encryption,
) -> Result<Value> {
    let mut warnings = Vec::new();
    if encryption == Encryption::Tabby {
        warnings.push("Tabby compatibility uses legacy encryption and is not recommended. Use modern encryption with compatible servers and clients.".into());
    }
    let remote_keys = document.keys()?;
    let mut keys = list(&workspace["keys"]);
    let mut merged_keys = vec![];
    let mut key_baseline = list(&baseline["keyBaseline"]);
    let file_ids: Vec<_> = rows(&document.vault.as_ref().unwrap_or(&Value::Null)["secrets"])?
        .iter()
        .filter(|v| v["type"] == "file")
        .map(|v| format!("file:{}", text(&v["key"]["id"])))
        .collect();
    for id in keys
        .iter()
        .map(key_id)
        .chain(key_baseline.iter().map(|k| text(&k["id"]).to_owned()))
    {
        if file_ids.contains(&id) && !remote_keys.iter().any(|k| k["id"] == id) {
            return Err("A synchronized remote key is unreadable. Nothing was uploaded.".into());
        }
    }
    if document.vault.is_some() {
        let raw = services.read_keys(keys.iter().map(|k| text(&k["id"]).to_owned()).collect())?;
        let mut locals = Vec::new();
        for raw in raw {
            let raw = serde_json::to_value(raw).map_err(|e| e.to_string())?;
            let metadata = keys
                .iter()
                .find(|k| k["id"] == raw["id"])
                .ok_or("Missing local key metadata.")?;
            locals.push(json!({"id":key_id(metadata),"name":metadata["name"],"secret":raw["secret"],"publicKey":raw["publicKey"]}));
        }
        for key in &mut keys {
            if key["syncId"].is_null() {
                let local_id = key_id(key);
                if let Some(local) = locals.iter_mut().find(|v| v["id"] == local_id)
                    && let Some(same) = remote_keys.iter().find(|v| {
                        v["secret"] == local["secret"]
                            && !list(&workspace["keys"])
                                .iter()
                                .any(|k| k["syncId"] == v["id"])
                    })
                {
                    key["syncId"] = same["id"].clone();
                    local["id"] = same["id"].clone();
                }
            }
        }
        let summaries = merge(
            &locals.iter().map(summary).collect::<Vec<_>>(),
            &remote_keys.iter().map(summary).collect::<Vec<_>>(),
            &key_baseline,
            true,
        )?;
        for s in &summaries {
            let mut source = locals
                .iter()
                .chain(&remote_keys)
                .find(|k| k["id"] == s["id"] && summary(k)["digest"] == s["digest"])
                .cloned()
                .ok_or("Cannot resolve a synchronized key.")?;
            source["name"] = s["name"].clone();
            merged_keys.push(source);
        }
        let mut next = vec![];
        for key in &merged_keys {
            let existing = keys.iter().find(|k| key_id(k) == text(&key["id"]));
            let original = locals.iter().find(|k| k["id"] == key["id"]);
            let mut metadata = if let Some(existing) = existing
                && original.is_some_and(|v| v["secret"] == key["secret"])
            {
                existing.clone()
            } else {
                let imported = services
                    .import_key(
                        text(&key["name"]).into(),
                        text(&key["secret"]).into(),
                        text(&key["publicKey"]).into(),
                        document.passphrase(text(&key["secret"]))?,
                    )
                    .await?;
                staged.push(imported.id.clone());
                serde_json::to_value(imported).map_err(|e| e.to_string())?
            };
            metadata["name"] = key["name"].clone();
            metadata["syncId"] = key["id"].clone();
            next.push(metadata);
        }
        keys = next;
        key_baseline = summaries;
    } else {
        warnings.push(
            "This configuration has no encrypted vault. Private keys were not synchronized."
                .to_owned(),
        );
    }
    let remote_profiles = document.profiles()?;
    let locals:Vec<_>=list(&workspace["hosts"]).iter().map(|h|{
            let key=keys.iter().find(|k|k["id"]==h["keyId"]);let id=profile_id(h);let mut p=json!({"id":id,"name":h["name"],"address":h["address"],"port":h["port"],"username":h["username"],"authentication":if key.is_some(){"publicKey"}else{h["syncAuth"].as_str().unwrap_or("password")},"keyRef":key.map(|k|json!(format!("vault://{}",key_id(k).trim_start_matches("file:")))).unwrap_or(h["syncKeyRef"].clone())});
            if document.vault.is_none(){let remote=remote_profiles.iter().find(|p|p["id"]==id);p["keyRef"]=remote.map_or(Value::Null,|p|p["keyRef"].clone());p["authentication"]=remote.map_or(json!("password"),|p|p["authentication"].clone());}p
        }).collect();
    let profiles = merge(
        &locals,
        &remote_profiles,
        &list(&baseline["baseline"]),
        true,
    )?;
    let known = serde_json::to_value(services.known_hosts()?).map_err(|e| e.to_string())?;
    let mut local_known = list(&baseline["fingerprints"]);
    for f in native_fingerprints(&list(&known))? {
        if let Some(old) = local_known.iter_mut().find(|v| v["id"] == f["id"]) {
            *old = f;
        } else {
            local_known.push(f);
        }
    }
    let fingerprints = merge(
        &local_known,
        &document.fingerprints()?,
        &list(&baseline["knownBaseline"]),
        false,
    )?;
    document.replace(
        &profiles,
        &list(&baseline["baseline"]),
        &merged_keys,
        &list(&baseline["keyBaseline"]),
        &fingerprints,
    )?;
    let mut hosts = vec![];
    for p in &profiles {
        let existing = list(&workspace["hosts"])
            .into_iter()
            .find(|h| profile_id(h) == text(&p["id"]));
        let key = keys.iter().find(|k| {
            format!("vault://{}", key_id(k).trim_start_matches("file:")) == text(&p["keyRef"])
        });
        if !p["keyRef"].is_null() && key.is_none() {
            warnings.push(format!("{}: the referenced key is unavailable on this device. Choose a local key before connecting.",text(&p["name"])));
        }
        let mut host = existing
            .clone()
            .unwrap_or(workspace::new_item("host".into())?);
        for field in ["name", "address", "port", "username"] {
            host[field] = p[field].clone();
        }
        host["syncId"] = p["id"].clone();
        host["syncAuth"] = p["authentication"].clone();
        host["syncKeyRef"] = p["keyRef"].clone();
        if document.vault.is_some() {
            host["keyId"] = key.map_or(json!(""), |k| k["id"].clone());
        }
        if existing.is_none() {
            let raw = rows(&document.config["profiles"])?;
            let mut jump = raw
                .iter()
                .find(|v| v["id"] == p["id"])
                .map_or("", |v| text(&v["options"]["jumpHost"]))
                .to_owned();
            let mut visited = HashSet::from([text(&p["id"]).to_owned()]);
            let mut chain = vec![];
            while !jump.is_empty() {
                if !visited.insert(jump.clone()) || visited.len() > 17 {
                    return Err("Invalid jump-host chain. Nothing was uploaded.".into());
                }
                let j = profiles
                    .iter()
                    .find(|v| v["id"] == jump)
                    .ok_or("Missing jump host. Nothing was uploaded.")?;
                let address = text(&j["address"]);
                chain.insert(
                    0,
                    format!(
                        "{}{}:{}",
                        if text(&j["username"]).is_empty() {
                            String::new()
                        } else {
                            format!("{}@", text(&j["username"]))
                        },
                        if address.contains(':') {
                            format!("[{address}]")
                        } else {
                            address.to_owned()
                        },
                        j["port"]
                    ),
                );
                jump = raw
                    .iter()
                    .find(|v| v["id"] == jump)
                    .map_or("", |v| text(&v["options"]["jumpHost"]))
                    .to_owned();
            }
            host["jump"] = json!(chain.join(","));
        }
        hosts.push(host);
    }
    let content = document.serialize(password, encryption)?;
    let latest = configuration(
        &services
            .request(server.clone(), token.clone(), Some(id), None, None)
            .await?,
        id,
    )?;
    if latest["content"] != remote["content"] || latest["modified_at"] != remote["modified_at"] {
        return Err(
            "The server configuration changed during sync. Try again. Nothing was uploaded.".into(),
        );
    }
    // Refuse to upload a snapshot that no longer matches the persisted local workspace.
    if services.load_workspace()? != *workspace {
        return Err("The local workspace changed during sync. Try again.".into());
    }
    services
        .request(
            server.clone(),
            token,
            Some(id),
            Some(content),
            response.etag.clone(),
        )
        .await?;
    workspace["forwards"] = json!(
        list(&workspace["forwards"])
            .into_iter()
            .filter(|f| hosts.iter().any(|h| h["id"] == f["hostId"]))
            .collect::<Vec<_>>()
    );
    workspace["hosts"] = json!(hosts);
    workspace["keys"] = json!(keys);
    workspace["sync"] = json!({"server":server,"configurationId":id,"encryption":encryption,"baseline":profiles,"keyBaseline":key_baseline,"knownBaseline":fingerprints,"fingerprints":fingerprints,"lastSyncedAt":workspace::now()});
    Ok(json!({"workspace":workspace,"warnings":warnings}))
}
#[tauri::command]
pub async fn synchronize_workspace(
    app: tauri::AppHandle,
    workspace: Value,
    server: String,
    token: String,
    configuration_id: u64,
    password: String,
    encryption: Option<Encryption>,
) -> Result<Value> {
    let server = server.trim().trim_end_matches('/').to_owned();
    let encryption = encryption.unwrap_or_else(|| {
        if workspace["sync"]["server"] == server
            && workspace["sync"]["configurationId"] == configuration_id
        {
            if workspace["sync"]["encryption"] == "modern" {
                Encryption::Modern
            } else {
                Encryption::Tabby
            }
        } else {
            Encryption::Modern
        }
    });
    synchronize(
        &NativeServices(app),
        workspace,
        server,
        token.trim().into(),
        configuration_id,
        password,
        encryption,
    )
    .await
}
#[tauri::command]
pub async fn list_configurations(server: String, token: String) -> Result<Value> {
    let response = crate::sync::tabby_request(
        server.trim().trim_end_matches('/').into(),
        token.trim().into(),
        None,
        None,
        None,
    )
    .await?;
    if !response.body.is_array() {
        return Err("Invalid sync server configurations response.".into());
    }
    let entries = rows(&response.body)?;
    for entry in &entries {
        if !entry["id"]
            .as_u64()
            .is_some_and(|id| id > 0 && id <= 9_007_199_254_740_991)
            || !entry["name"].is_string()
        {
            return Err("Invalid sync server configuration.".into());
        }
    }
    Ok(json!(
        entries
            .iter()
            .map(|entry| json!({"id":entry["id"],"name":entry["name"]}))
            .collect::<Vec<_>>()
    ))
}
#[tauri::command]
pub fn display_known(workspace: Value, known: Vec<Value>) -> Result<Vec<Value>> {
    let local = native_fingerprints(&known)?;
    let mut result = known;
    for f in list(&workspace["sync"]["fingerprints"]) {
        if !local
            .iter()
            .any(|k| k["id"] == f["id"] && k["digest"] == f["digest"])
        {
            result.push(json!({"line":f["id"],"host":if f["port"]==22{text(&f["host"]).to_owned()}else{format!("[{}]:{}",text(&f["host"]),f["port"])},"kind":f["type"],"fingerprint":format!("SHA256:{}",text(&f["digest"]).trim_end_matches('=')),"synced":true}));
        }
    }
    Ok(result)
}
#[tauri::command]
pub fn forget_fingerprint(
    app: tauri::AppHandle,
    mut workspace: Value,
    item: Value,
) -> Result<Value> {
    let ids = if item["synced"] == true {
        vec![item["line"].clone()]
    } else {
        native_fingerprints(std::slice::from_ref(&item))?
            .iter()
            .map(|f| f["id"].clone())
            .collect()
    };
    if workspace["sync"].is_object() {
        workspace["sync"]["fingerprints"] = json!(
            list(&workspace["sync"]["fingerprints"])
                .into_iter()
                .filter(|f| !ids.contains(&f["id"]))
                .collect::<Vec<_>>()
        );
    }
    if item["synced"] != true {
        crate::remove_known_host(app, text(&item["line"]).into())?;
    }
    Ok(workspace)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::decrypt;
    fn encrypt(vault: &Value, password: &str) -> Result<Value> {
        vault::encrypt(vault, password, Encryption::Tabby)
    }
    use std::sync::Mutex;
    fn profile() -> Value {
        json!({"id":"ssh:remote","name":"Remote","address":"remote.example.com","port":22,"username":"root","authentication":"password","keyRef":null})
    }
    #[test]
    fn merges_deletions_independent_fields_and_conflicts() {
        let base = profile();
        let mut local = base.clone();
        local["name"] = json!("Local");
        let mut remote = base.clone();
        remote["username"] = json!("admin");
        let merged = merge(
            &[local.clone()],
            &[remote.clone()],
            std::slice::from_ref(&base),
            true,
        )
        .unwrap();
        assert_eq!(merged[0]["name"], "Local");
        assert_eq!(merged[0]["username"], "admin");
        assert!(
            merge(
                &[],
                std::slice::from_ref(&base),
                std::slice::from_ref(&base),
                true
            )
            .unwrap()
            .is_empty()
        );
        assert!(merge(&[], &[remote.clone()], std::slice::from_ref(&base), true).is_err());
        remote["name"] = json!("Remote edit");
        assert!(merge(&[local], &[remote], std::slice::from_ref(&base), true).is_err());
        assert!(merge(&[base.clone(), base], &[], &[], false).is_err());
    }
    #[test]
    fn validates_remote_ports_without_replacing_invalid_values_with_defaults() {
        assert_eq!(remote_port(&Value::Null).unwrap(), 22);
        assert_eq!(remote_port(&json!("2222")).unwrap(), 2222);
        for port in [
            json!(-1),
            json!(0),
            json!(65536),
            json!(22.5),
            json!("invalid"),
        ] {
            assert!(remote_port(&port).is_err());
        }
    }
    #[test]
    fn independent_vault_fixture_and_fresh_randomness() {
        let fixture: Value =
            serde_json::from_str(include_str!("fixtures/tabby-vault-v1.json")).unwrap();
        let vault = decrypt(&fixture, "fixture-password").unwrap();
        assert!(vault["config"].is_object());
        assert_eq!(list(&vault["secrets"]).len(), 1);
        assert!(decrypt(&fixture, "wrong").is_err());
        let mut bad = fixture;
        bad["version"] = json!(2);
        assert!(decrypt(&bad, "pw").is_err());
        let vault = json!({"config":{"profiles":[]},"secrets":[{"type":"unknown","value":"café"}]});
        let a = encrypt(&vault, "pw").unwrap();
        let b = encrypt(&vault, "pw").unwrap();
        assert_ne!(a["iv"], b["iv"]);
        assert_ne!(a["keySalt"], b["keySalt"]);
        assert_eq!(decrypt(&a, "pw").unwrap(), vault);
    }
    #[test]
    fn rejects_bad_yaml_and_unsupported_profiles() {
        for input in [
            "profiles: []\nprofiles: []",
            "- one",
            "profiles: invalid",
            "profiles: [{id: x, type: ssh, options: {host: example.com, proxyCommand: bad}}]",
        ] {
            assert!(
                Document::open(input, "")
                    .and_then(|d| d.profiles())
                    .is_err(),
                "{input}"
            );
        }
        assert!(
            configuration(
                &crate::sync::Response {
                    body: json!({"id":2,"content":""}),
                    etag: None
                },
                1
            )
            .is_err()
        );
    }
    #[test]
    fn preserves_unrelated_options_templates_and_passwords() {
        let mut d=Document::open("appearance: {frame: native}\nprofiles:\n  - {id: local, type: local, options: {command: /bin/zsh}}\n  - {id: template, type: ssh, isTemplate: true}\n  - {id: 'ssh:remote', name: Remote, type: ssh, options: {host: remote.example.com, user: root, keepaliveInterval: 5000, password: retained}}","").unwrap();
        let mut profiles = d.profiles().unwrap();
        profiles[0]["name"] = json!("Renamed");
        d.replace(&profiles, &[], &[], &[], &[]).unwrap();
        let content = d.serialize("", Encryption::Tabby).unwrap();
        for retained in [
            "retained",
            "5000",
            "/bin/zsh",
            "frame: native",
            "isTemplate: true",
        ] {
            assert!(content.contains(retained));
        }
        d.replace(&[], &profiles, &[], &[], &[]).unwrap();
        assert!(d.profiles().unwrap().is_empty());
        assert_eq!(list(&d.config["profiles"]).len(), 2);
    }
    #[test]
    fn preserves_secrets_only_and_whole_config_encryption() {
        for encrypted in [false, true] {
            let vault = json!({"config":{"profiles":[]},"secrets":[{"type":"ssh:password","value":"keep-me"},{"type":"file","key":{"id":"text"},"value":STANDARD.encode("ordinary file")}]});
            let root =
                json!({"encrypted":encrypted,"profiles":[],"vault":encrypt(&vault,"pw").unwrap()});
            let mut d = Document::open(&serde_yaml_ng::to_string(&root).unwrap(), "pw").unwrap();
            d.replace(&[], &[], &[], &[], &[]).unwrap();
            let serialized = d.serialize("pw", Encryption::Tabby).unwrap();
            assert!(!serialized.contains("keep-me"));
            let reopened = Document::open(&serialized, "pw").unwrap();
            assert_eq!(reopened.vault.unwrap()["secrets"], vault["secrets"]);
        }
    }
    #[test]
    fn fingerprints_normalize_aliases_and_skip_hashed_hosts() {
        let digest = STANDARD_NO_PAD.encode([1u8; 32]);
        let known = json!([{"host":"EXAMPLE.com,[other]:2222,|hashed|label","kind":"ssh-ed25519","fingerprint":format!("SHA256:{digest}")}]);
        let f = native_fingerprints(&list(&known)).unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0]["id"], "example.com|22|ssh-ed25519");
        assert_eq!(f[1]["port"], 2222);
        assert!(text(&f[0]["digest"]).ends_with('='));
    }
    struct Fake {
        workspace: Value,
        content: Mutex<String>,
        reads: Mutex<usize>,
        writes: Mutex<usize>,
        removed: Mutex<Vec<String>>,
        fail: bool,
        concurrent: bool,
        local_change: bool,
    }
    impl Fake {
        fn new(content: &str) -> Self {
            Self {
                workspace: workspace::empty(),
                content: Mutex::new(content.into()),
                reads: Mutex::new(0),
                writes: Mutex::new(0),
                removed: Mutex::new(vec![]),
                fail: false,
                concurrent: false,
                local_change: false,
            }
        }
    }
    impl Services for Fake {
        async fn request(
            &self,
            _server: String,
            _token: String,
            _id: Option<u64>,
            content: Option<String>,
            etag: Option<String>,
        ) -> Result<crate::sync::Response> {
            if let Some(content) = content {
                assert_eq!(etag.as_deref(), Some("etag-1"));
                if self.fail {
                    return Err("HTTP 412".into());
                }
                *self.writes.lock().unwrap() += 1;
                *self.content.lock().unwrap() = content;
                return Ok(crate::sync::Response {
                    body: Value::Null,
                    etag: None,
                });
            }
            let mut reads = self.reads.lock().unwrap();
            *reads += 1;
            Ok(crate::sync::Response {
                body: json!({"id":1,"content":if self.concurrent && *reads==2 {"profiles: []\n# changed".to_owned()}else{self.content.lock().unwrap().clone()},"modified_at":"now"}),
                etag: Some("etag-1".into()),
            })
        }
        fn read_keys(&self, _ids: Vec<String>) -> Result<Vec<crate::SyncKey>> {
            Ok(vec![])
        }
        async fn import_key(
            &self,
            name: String,
            _secret: String,
            _public_key: String,
            _passphrase: String,
        ) -> Result<crate::KeyEntry> {
            Ok(crate::KeyEntry {
                id: "imported".into(),
                name,
                kind: "ssh-ed25519".into(),
                public_key: "public".into(),
                fingerprint: "fingerprint".into(),
            })
        }
        fn delete_key(&self, id: String) -> Result<()> {
            self.removed.lock().unwrap().push(id);
            Ok(())
        }
        fn known_hosts(&self) -> Result<Vec<crate::KnownHost>> {
            Ok(vec![])
        }
        fn load_workspace(&self) -> Result<Value> {
            if self.local_change {
                Ok(json!({}))
            } else {
                Ok(self.workspace.clone())
            }
        }
    }
    fn run(fake: &Fake, password: &str) -> Result<Value> {
        run_mode(fake, password, Encryption::Tabby)
    }
    fn run_mode(fake: &Fake, password: &str, encryption: Encryption) -> Result<Value> {
        tauri::async_runtime::block_on(synchronize(
            fake,
            fake.workspace.clone(),
            "https://example.com".into(),
            "token".into(),
            1,
            password.into(),
            encryption,
        ))
    }
    #[test]
    fn modern_sync_encrypts_plaintext_and_repeats_without_exposing_hosts() {
        let mut f = Fake::new(
            "appearance: {theme: retained}\nprofiles: [{id: remote, type: ssh, options: {host: secret.example.com}}]",
        );
        assert!(run_mode(&f, "", Encryption::Modern).is_err());
        assert_eq!(*f.writes.lock().unwrap(), 0);
        let first = run_mode(&f, "pw", Encryption::Modern).unwrap();
        let content = f.content.lock().unwrap().clone();
        assert!(!content.contains("secret.example.com"));
        assert!(!content.contains("retained"));
        let document = Document::open(&content, "pw").unwrap();
        assert_eq!(document.root["encrypted"], true);
        assert_eq!(document.root["vault"]["version"], 2);
        assert_eq!(document.config["appearance"]["theme"], "retained");
        assert_eq!(first["workspace"]["sync"]["encryption"], "modern");
        f.workspace = first["workspace"].clone();
        let next = run_mode(&f, "pw", Encryption::Modern).unwrap();
        assert_eq!(next["workspace"]["hosts"], first["workspace"]["hosts"]);
        assert!(run_mode(&f, "pw", Encryption::Tabby).is_err());
        assert_eq!(*f.writes.lock().unwrap(), 2);
        *f.content.lock().unwrap() = "profiles: []".into();
        assert!(
            run_mode(&f, "pw", Encryption::Modern)
                .unwrap_err()
                .contains("lost its modern encryption")
        );
        assert_eq!(*f.writes.lock().unwrap(), 2);
    }
    #[test]
    fn upgrades_both_legacy_vault_forms_and_preserves_secrets() {
        for encrypted in [false, true] {
            let vault = json!({"config":{"profiles":[],"appearance":{"theme":"retained"}},"secrets":[{"type":"ssh:password","value":"keep-secret"}]});
            let root = json!({"encrypted":encrypted,"profiles":[],"appearance":{"theme":"retained"},"vault":encrypt(&vault,"pw").unwrap()});
            let f = Fake::new(&serde_yaml_ng::to_string(&root).unwrap());
            assert!(run_mode(&f, "wrong", Encryption::Modern).is_err());
            assert_eq!(*f.writes.lock().unwrap(), 0);
            run_mode(&f, "pw", Encryption::Modern).unwrap();
            let content = f.content.lock().unwrap();
            assert!(!content.contains("keep-secret"));
            let mut document = Document::open(&content, "pw").unwrap();
            assert_eq!(document.root["vault"]["version"], 2);
            assert_eq!(
                document.vault.as_ref().unwrap()["secrets"],
                vault["secrets"]
            );
            assert_eq!(document.config["appearance"]["theme"], "retained");
            assert!(document.serialize("pw", Encryption::Tabby).is_err());
            let mut root = document.root.clone();
            root["encrypted"] = json!(false);
            assert!(Document::open(&serde_yaml_ng::to_string(&root).unwrap(), "pw").is_err());
        }
    }
    #[test]
    fn sync_imports_and_repeats_without_duplicate_hosts() {
        let mut f = Fake::new(
            "profiles: [{id: 'ssh:remote', type: ssh, name: Remote, options: {host: remote.example.com}}]",
        );
        let mut first = run(&f, "").unwrap();
        assert_eq!(list(&first["workspace"]["hosts"]).len(), 1);
        first["workspace"]["groups"] = json!([{"id":"local-group","name":"Production"}]);
        first["workspace"]["hosts"][0]["groupId"] = json!("local-group");
        first["workspace"]["settings"]["shortcuts"] = json!({"newTab":"Alt+KeyT"});
        f.workspace = first["workspace"].clone();
        let next = run(&f, "").unwrap();
        assert_eq!(next["workspace"]["hosts"], first["workspace"]["hosts"]);
        assert_eq!(next["workspace"]["groups"], first["workspace"]["groups"]);
        assert_eq!(
            next["workspace"]["settings"]["shortcuts"],
            first["workspace"]["settings"]["shortcuts"]
        );
        assert!(!f.content.lock().unwrap().contains("local-group"));
        assert_eq!(*f.writes.lock().unwrap(), 2);
    }
    #[test]
    fn pushes_local_hosts_and_propagates_deletions() {
        let mut f = Fake::new(
            "profiles: [{id: local, type: local}]
",
        );
        let mut host = workspace::new_item("host".into()).unwrap();
        host["name"] = json!("Local");
        host["address"] = json!("local.example.com");
        f.workspace["hosts"] = json!([host]);
        let first = run(&f, "").unwrap();
        assert!(f.content.lock().unwrap().contains("local.example.com"));
        f.workspace = first["workspace"].clone();
        f.workspace["hosts"] = json!([]);
        let next = run(&f, "").unwrap();
        assert_eq!(next["workspace"]["hosts"], json!([]));
        let d = Document::open(&f.content.lock().unwrap(), "").unwrap();
        assert_eq!(d.config["profiles"], json!([{"id":"local","type":"local"}]));
    }
    #[test]
    fn imported_keys_keep_assignments_and_secrets_out_of_workspace() {
        for encryption in [Encryption::Tabby, Encryption::Modern] {
            let vault = json!({"config":{"profiles":[{"id":"target","type":"ssh","options":{"host":"target.example.com","privateKeys":["vault://key"]}}]},"secrets":[{"type":"file","key":{"id":"key","description":"Key"},"value":STANDARD.encode("-----BEGIN OPENSSH PRIVATE KEY-----fixture")},{"type":"ssh:password","value":"retained-password"}]});
            let f = Fake::new(
                &serde_yaml_ng::to_string(
                    &json!({"encrypted":true,"vault":encrypt(&vault,"pw").unwrap()}),
                )
                .unwrap(),
            );
            let result = run_mode(&f, "pw", encryption).unwrap();
            assert_eq!(result["workspace"]["hosts"][0]["keyId"], "imported");
            let metadata = serde_json::to_string(&result["workspace"]).unwrap();
            assert!(!metadata.contains("PRIVATE KEY"));
            assert!(!metadata.contains("retained-password"));
            let remote = Document::open(&f.content.lock().unwrap(), "pw").unwrap();
            assert_eq!(remote.vault.unwrap()["secrets"][1]["type"], "file");
            assert!(f.removed.lock().unwrap().is_empty());
        }
    }
    #[test]
    fn rejects_concurrent_changes_before_upload() {
        for local in [true, false] {
            let mut f = Fake::new("profiles: []");
            f.local_change = local;
            f.concurrent = !local;
            assert!(run(&f, "").is_err());
            assert_eq!(*f.writes.lock().unwrap(), 0);
        }
    }
    #[test]
    fn staged_keys_are_cleaned_on_failed_upload() {
        for encryption in [Encryption::Tabby, Encryption::Modern] {
            let vault = json!({"config":{"profiles":[]},"secrets":[{"type":"file","key":{"id":"key","description":"Key"},"value":STANDARD.encode("-----BEGIN OPENSSH PRIVATE KEY-----fixture")} ]});
            let root = json!({"encrypted":true,"vault":encrypt(&vault,"pw").unwrap()});
            let mut f = Fake::new(&serde_yaml_ng::to_string(&root).unwrap());
            f.fail = true;
            assert!(run_mode(&f, "pw", encryption).is_err());
            assert_eq!(*f.removed.lock().unwrap(), ["imported"]);
            assert!(list(&f.workspace["keys"]).is_empty());
        }
    }
    #[test]
    fn jump_references_resolve_and_cycles_block_upload() {
        let mut f = Fake::new(
            "profiles: [{id: jump, type: ssh, options: {host: jump.example.com, user: root}}, {id: target, type: ssh, options: {host: target.example.com, jumpHost: jump}}]",
        );
        let result = run(&f, "").unwrap();
        assert_eq!(
            result["workspace"]["hosts"][1]["jump"],
            "root@jump.example.com:22"
        );
        *f.content.lock().unwrap()="profiles: [{id: target, type: ssh, options: {host: target.example.com, jumpHost: target}}]".into();
        *f.writes.lock().unwrap() = 0;
        assert!(run(&f, "").is_err());
        assert_eq!(*f.writes.lock().unwrap(), 0);
        f.workspace["keys"] = json!([{"id":"local","syncId":"file:key"}]);
        *f.content.lock().unwrap()=serde_yaml_ng::to_string(&json!({"profiles":[],"vault":encrypt(&json!({"secrets":[{"type":"file","key":{"id":"key"},"value":STANDARD.encode("unreadable")}]}),"pw").unwrap()})).unwrap();
        assert!(run(&f, "pw").unwrap_err().contains("unreadable"));
    }
}
