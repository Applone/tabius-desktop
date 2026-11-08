use crate::Result;
use serde_json::{Value, json};
use std::collections::HashSet;
use uuid::Uuid;

pub fn id() -> String {
    Uuid::new_v4().to_string()
}
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
pub fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}
pub fn list(value: &Value) -> Vec<Value> {
    value.as_array().cloned().unwrap_or_default()
}
pub fn settings() -> Value {
    json!({"autoReconnect":false,"bell":false,"optionAsMeta":true,"emulator":"xterm-256color","font":"monospace","fontSize":14,"lineHeight":1.2,"theme":"Graphite","highlighting":true,"detectOS":true,"postQuantum":true,"shortcuts":{}})
}
pub fn empty() -> Value {
    json!({"hosts":[],"groups":[],"keys":[],"snippets":[],"forwards":[],"history":[],"settings":settings()})
}
#[tauri::command]
pub fn new_item(kind: String) -> Result<Value> {
    Ok(match kind.as_str() {
        "host" => {
            json!({"id":id(),"name":"","address":"","port":22,"username":"","keyId":"","agentForwarding":false,"startup":"","jump":"","proxy":"","env":"","utf8":true,"mosh":false,"system":"Server"})
        }
        "forward" => {
            json!({"id":id(),"name":"","hostId":"","kind":"local","bindAddress":"127.0.0.1","bindPort":8080,"destination":"127.0.0.1","destinationPort":80})
        }
        "snippet" => json!({"id":id(),"name":"","command":""}),
        "group" => json!({"id":id(),"name":""}),
        _ => return Err("Invalid item kind.".into()),
    })
}
fn port(value: &Value) -> bool {
    value.as_u64().is_some_and(|p| (1..=65535).contains(&p))
}
pub fn validate_host(host: &Value) -> Result<()> {
    if text(&host["name"]).trim().is_empty() {
        return Err("Give this host a name.".into());
    }
    let address = text(&host["address"]);
    if address.is_empty()
        || address.starts_with('-')
        || !address
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b":._-".contains(&c))
    {
        return Err("Enter a hostname or IP address without a protocol or path.".into());
    }
    if !port(&host["port"]) {
        return Err("Port must be between 1 and 65535.".into());
    }
    Ok(())
}
pub fn history(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut commands = Vec::new();
    for line in text.lines() {
        let line = if let Some(rest) = line.strip_prefix(": ") {
            rest.split_once(';')
                .filter(|(prefix, _)| {
                    prefix.split_once(':').is_some_and(|(a, b)| {
                        !a.is_empty()
                            && !b.is_empty()
                            && a.bytes().chain(b.bytes()).all(|c| c.is_ascii_digit())
                    })
                })
                .map_or(line, |(_, command)| command)
        } else {
            line
        };
        let line = line.trim();
        if !line.is_empty() && !line.starts_with('#') && seen.insert(line.to_owned()) {
            commands.push(line.to_owned());
        }
    }
    commands.into_iter().rev().take(500).collect()
}
#[tauri::command]
pub fn edit_workspace(
    mut workspace: Value,
    kind: String,
    mut item: Value,
    remove: bool,
) -> Result<Value> {
    let collection = match kind.as_str() {
        "host" => "hosts",
        "group" => "groups",
        "key" => "keys",
        "forward" => "forwards",
        "snippet" => "snippets",
        "history" => "history",
        "os" => "hosts",
        _ => return Err("Invalid collection.".into()),
    };
    let item_id = text(&item["id"]).to_owned();
    if kind == "history" {
        let commands = history(text(&item));
        if commands.is_empty() {
            return Err("No commands found.".into());
        }
        let mut entries: Vec<_> = commands.into_iter().map(|command| json!({"id":id(),"host":"Imported history","command":command,"date":now()})).collect();
        entries.extend(list(&workspace["history"]));
        entries.truncate(1000);
        workspace["history"] = json!(entries);
        return Ok(workspace);
    }
    if !remove {
        match kind.as_str() {
            "host" => {
                validate_host(&item)?;
                if !text(&item["groupId"]).is_empty()
                    && !list(&workspace["groups"])
                        .iter()
                        .any(|g| g["id"] == item["groupId"])
                {
                    return Err("Choose an existing host group.".into());
                }
                if let Some(old) = list(&workspace["hosts"])
                    .iter()
                    .find(|h| h["id"] == item["id"])
                    && old["keyId"] != item["keyId"]
                {
                    item["syncAuth"] = json!(if text(&item["keyId"]).is_empty() {
                        "password"
                    } else {
                        "publicKey"
                    });
                    item["syncKeyRef"] = Value::Null;
                }
            }
            "forward" => {
                if text(&item["name"]).trim().is_empty()
                    || !list(&workspace["hosts"])
                        .iter()
                        .any(|h| h["id"] == item["hostId"])
                {
                    return Err("Enter a name and choose a host.".into());
                }
                if !port(&item["bindPort"])
                    || (item["kind"] != "dynamic" && !port(&item["destinationPort"]))
                {
                    return Err("Ports must be between 1 and 65535.".into());
                }
            }
            "group" => {
                let name = text(&item["name"]).trim();
                if name.is_empty()
                    || name.chars().count() > 100
                    || name.chars().any(char::is_control)
                {
                    return Err(
                        "Enter a group name of 1–100 characters without control characters.".into(),
                    );
                }
                if list(&workspace["groups"])
                    .iter()
                    .any(|g| g["id"] != item["id"] && text(&g["name"]).eq_ignore_ascii_case(name))
                {
                    return Err("A host group with this name already exists.".into());
                }
                item["name"] = json!(name);
            }
            "snippet"
                if text(&item["name"]).trim().is_empty()
                    || text(&item["command"]).trim().is_empty() =>
            {
                return Err("Enter a name and script.".into());
            }
            _ => (),
        }
    }
    let mut items = list(&workspace[collection]);
    if remove {
        items.retain(|v| text(&v["id"]) != item_id);
    } else if let Some(old) = items.iter_mut().find(|v| text(&v["id"]) == item_id) {
        if kind == "os" {
            old["system"] = json!(
                text(&item["system"])
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || " _.-".contains(*c))
                    .take(60)
                    .collect::<String>()
            );
        } else {
            *old = item;
        }
    } else if kind != "os" {
        if item_id.is_empty() {
            item["id"] = json!(id());
        }
        items.push(item);
    }
    workspace[collection] = json!(items);
    if remove && kind == "group" {
        for host in workspace["hosts"].as_array_mut().ok_or("Invalid hosts.")? {
            if text(&host["groupId"]) == item_id {
                host["groupId"] = json!("");
            }
        }
    }
    if remove && kind == "host" {
        workspace["forwards"] = json!(
            list(&workspace["forwards"])
                .into_iter()
                .filter(|v| text(&v["hostId"]) != item_id)
                .collect::<Vec<_>>()
        );
    }
    if remove && kind == "key" {
        for host in workspace["hosts"].as_array_mut().ok_or("Invalid hosts.")? {
            if text(&host["keyId"]) == item_id {
                host["keyId"] = json!("");
            }
        }
    }
    Ok(workspace)
}
#[tauri::command]
pub fn prepare_session(workspace: Value, mut host: Value, forward: Option<Value>) -> Value {
    host["startup"] = list(&workspace["snippets"])
        .iter()
        .find(|s| s["id"] == host["startup"])
        .map_or(json!(""), |s| s["command"].clone());
    let fingerprints: Vec<_> = list(&workspace["sync"]["fingerprints"])
        .iter()
        .filter(|f| {
            text(&f["host"]).eq_ignore_ascii_case(text(&host["address"]))
                && f["port"] == host["port"]
        })
        .map(|f| {
            format!(
                "{} SHA256:{}",
                text(&f["type"]),
                text(&f["digest"]).trim_end_matches('=')
            )
        })
        .collect();
    json!({"id":id(),"host":host,"forward":forward,"fingerprints":fingerprints})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn host_groups_validate_names_membership_and_preserve_hosts_on_deletion() {
        let mut w = empty();
        let mut group = new_item("group".into()).unwrap();
        group["name"] = json!("  Production  ");
        w = edit_workspace(w, "group".into(), group.clone(), false).unwrap();
        assert_eq!(w["groups"][0]["name"], "Production");
        let mut duplicate = new_item("group".into()).unwrap();
        for name in ["production", "", "\n", "bad\nname"] {
            duplicate["name"] = json!(name);
            assert!(edit_workspace(w.clone(), "group".into(), duplicate.clone(), false).is_err());
        }
        duplicate["name"] = json!("x".repeat(101));
        assert!(edit_workspace(w.clone(), "group".into(), duplicate, false).is_err());
        let mut host = new_item("host".into()).unwrap();
        host["name"] = json!("Server");
        host["address"] = json!("example.com");
        host["groupId"] = json!("missing");
        assert!(edit_workspace(w.clone(), "host".into(), host.clone(), false).is_err());
        host["groupId"] = group["id"].clone();
        w = edit_workspace(w, "host".into(), host.clone(), false).unwrap();
        group["name"] = json!("Live");
        w = edit_workspace(w, "group".into(), group.clone(), false).unwrap();
        assert_eq!(w["groups"][0]["name"], "Live");
        assert_eq!(w["hosts"][0]["groupId"], group["id"]);
        w["forwards"] = json!([{"id":"forward","hostId":host["id"]}]);
        w = edit_workspace(w, "group".into(), group, true).unwrap();
        assert_eq!(w["groups"], json!([]));
        assert_eq!(w["hosts"][0]["id"], host["id"]);
        assert_eq!(w["hosts"][0]["groupId"], "");
        assert_eq!(w["forwards"][0]["hostId"], host["id"]);
    }

    #[test]
    fn old_workspaces_can_create_groups_and_keep_shortcut_overrides() {
        let mut w = empty();
        w.as_object_mut().unwrap().remove("groups");
        w["settings"]["shortcuts"] = json!({"newTab":"Ctrl+KeyN","closeTab":""});
        w = edit_workspace(
            w,
            "group".into(),
            json!({"id":"group","name":"Local"}),
            false,
        )
        .unwrap();
        assert_eq!(w["groups"][0]["name"], "Local");
        assert_eq!(w["settings"]["shortcuts"]["newTab"], "Ctrl+KeyN");
        assert_eq!(w["settings"]["shortcuts"]["closeTab"], "");
    }
    #[test]
    fn imports_shell_history() {
        assert_eq!(history("# comment\n: 123:0; ls\npwd\nls\n"), ["pwd", "ls"]);
        assert_eq!(
            history(&(0..600).map(|i| format!("echo {i}\n")).collect::<String>()).len(),
            500
        );
    }
    #[test]
    fn validates_and_cascades_deletions() {
        let mut w = empty();
        let mut h = new_item("host".into()).unwrap();
        h["name"] = json!("test");
        h["address"] = json!("-bad");
        assert!(validate_host(&h).is_err());
        h["address"] = json!("::1");
        w = edit_workspace(w, "host".into(), h.clone(), false).unwrap();
        w["forwards"] = json!([{"hostId":h["id"]}]);
        let w = edit_workspace(w, "host".into(), h, true).unwrap();
        assert_eq!(w["hosts"], json!([]));
        assert_eq!(w["forwards"], json!([]));
    }
}
#[cfg(test)]
mod regression_tests {
    use super::*;
    #[test]
    fn accepts_endpoints_and_rejects_invalid_ports_and_options() {
        let mut host = new_item("host".into()).unwrap();
        host["name"] = json!("Server");
        for address in ["server.example.com", "127.0.0.1", "2001:db8::1"] {
            host["address"] = json!(address);
            assert!(validate_host(&host).is_ok());
        }
        for address in [
            "-oProxyCommand=bad",
            "ssh://host",
            "user@host",
            "host;id",
            "host\nother",
        ] {
            host["address"] = json!(address);
            assert!(validate_host(&host).is_err());
        }
        host["address"] = json!("localhost");
        for port in [json!(0), json!(65536), json!(22.5), Value::Null] {
            host["port"] = port;
            assert!(validate_host(&host).is_err());
        }
    }
    #[test]
    fn workspace_defaults_upserts_and_key_deletion_are_independent() {
        let mut w = empty();
        w["settings"]["bell"] = json!(true);
        assert_eq!(empty()["settings"]["bell"], false);
        assert_eq!(w["hosts"], json!([]));
        for (id, name) in [("a", "First"), ("b", "Second"), ("a", "Updated")] {
            w = edit_workspace(
                w,
                "snippet".into(),
                json!({"id":id,"name":name,"command":"pwd"}),
                false,
            )
            .unwrap();
        }
        assert_eq!(w["snippets"][0]["name"], "Updated");
        assert_eq!(w["snippets"][1]["name"], "Second");
        w["keys"] = json!([{"id":"key"}]);
        w["hosts"] = json!([{"id":"host","keyId":"key"}]);
        let w = edit_workspace(w, "key".into(), json!({"id":"key"}), true).unwrap();
        assert_eq!(w["hosts"][0]["keyId"], "");
        assert_eq!(w["keys"], json!([]));
    }
    #[test]
    fn validates_forwarding_snippets_and_empty_history() {
        let w = empty();
        assert!(
            edit_workspace(
                w.clone(),
                "forward".into(),
                new_item("forward".into()).unwrap(),
                false
            )
            .is_err()
        );
        assert!(
            edit_workspace(
                w.clone(),
                "snippet".into(),
                new_item("snippet".into()).unwrap(),
                false
            )
            .is_err()
        );
        assert!(edit_workspace(w, "history".into(), json!("# comment\n "), false).is_err());
    }
}
