use crate::{
    Result, sync,
    tabby::{Document, configuration},
    vault::{Encryption, hex},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    server: String,
    token: String,
    configuration_id: u64,
}

impl Connection {
    fn endpoint(&self) -> Result<String> {
        if !(1..=9_007_199_254_740_991).contains(&self.configuration_id) {
            return Err("Select a configuration for each server.".into());
        }
        Ok(sync::endpoint(self.server.trim(), Some(self.configuration_id))?.to_string())
    }
}

trait Transport {
    async fn request(
        &self,
        connection: &Connection,
        content: Option<String>,
        etag: Option<String>,
    ) -> Result<sync::Response>;
}

struct NativeTransport;
impl Transport for NativeTransport {
    async fn request(
        &self,
        connection: &Connection,
        content: Option<String>,
        etag: Option<String>,
    ) -> Result<sync::Response> {
        sync::tabby_request(
            connection.server.trim().trim_end_matches('/').into(),
            connection.token.trim().into(),
            Some(connection.configuration_id),
            content,
            etag,
        )
        .await
    }
}

struct Snapshot {
    source: Value,
    destination: Value,
    destination_etag: Option<String>,
    fingerprint: String,
}

async fn read(
    transport: &impl Transport,
    source: &Connection,
    destination: &Connection,
) -> Result<Snapshot> {
    let source_response = transport.request(source, None, None).await?;
    let destination_response = transport.request(destination, None, None).await?;
    let source_content = configuration(&source_response, source.configuration_id)?;
    let destination_content = configuration(&destination_response, destination.configuration_id)?;
    // Bind review to both destinations and their versions, without retaining credentials.
    let fingerprint = hex(&Sha256::digest(
        serde_json::to_vec(&json!([
            source.endpoint()?,
            source_content["content"],
            source_content["modified_at"],
            source_response.etag,
            destination.endpoint()?,
            destination_content["content"],
            destination_content["modified_at"],
            destination_response.etag,
        ]))
        .map_err(|e| e.to_string())?,
    ));
    Ok(Snapshot {
        source: source_content,
        destination: destination_content,
        destination_etag: destination_response.etag,
        fingerprint,
    })
}

fn count(value: &Value) -> Result<usize> {
    if value.is_null() {
        Ok(0)
    } else {
        value
            .as_array()
            .map(Vec::len)
            .ok_or("Invalid configuration list.".into())
    }
}

async fn migrate(
    transport: &impl Transport,
    source: Connection,
    destination: Connection,
    source_password: String,
    password: String,
    fingerprint: Option<String>,
) -> Result<Value> {
    if source.endpoint()? == destination.endpoint()? {
        return Err(
            "Choose a different destination configuration. The source must be preserved.".into(),
        );
    }
    if fingerprint.is_some() && password.is_empty() {
        return Err("Enter a password for the new Tabius vault.".into());
    }
    let snapshot = read(transport, &source, &destination).await?;
    if fingerprint
        .as_ref()
        .is_some_and(|f| *f != snapshot.fingerprint)
    {
        return Err("A configuration changed since the preview. Preview the migration again. Nothing was uploaded.".into());
    }
    let mut document = Document::open(
        snapshot.source["content"].as_str().unwrap(),
        &source_password,
    )?;
    let preview = json!({
        "fingerprint":snapshot.fingerprint,
        "profiles":count(&document.config["profiles"])?,
        "secrets":count(&document.vault.as_ref().unwrap_or(&Value::Null)["secrets"])?,
        "fingerprints":count(&document.config["ssh"]["knownHosts"])?,
        "destinationHasContent":!snapshot.destination["content"].as_str().unwrap().trim().is_empty(),
    });
    if fingerprint.is_none() {
        return Ok(preview);
    }
    // Re-encrypt the entire document. Workspace projection would drop unsupported
    // profiles, saved passwords, templates, and other clients' configuration.
    let content = document.serialize(&password, Encryption::Modern)?;
    let latest = read(transport, &source, &destination).await?;
    if latest.fingerprint != snapshot.fingerprint {
        return Err(
            "A configuration changed during migration. Preview again. Nothing was uploaded.".into(),
        );
    }
    let expected_content = Sha256::digest(content.as_bytes());
    transport
        .request(&destination, Some(content), snapshot.destination_etag)
        .await?;
    const UNVERIFIED: &str = "The upload finished, but the destination copy could not be verified. The Tabby-Web source is unchanged. Preview again before retrying.";
    let uploaded = transport
        .request(&destination, None, None)
        .await
        .and_then(|response| configuration(&response, destination.configuration_id))
        .map_err(|_| UNVERIFIED)?;
    if Sha256::digest(uploaded["content"].as_str().unwrap().as_bytes()) != expected_content {
        return Err(UNVERIFIED.into());
    }
    Ok(json!({"migrated":true}))
}

#[tauri::command]
pub async fn migrate_configuration(
    source: Connection,
    destination: Connection,
    source_password: String,
    password: String,
    fingerprint: Option<String>,
) -> Result<Value> {
    migrate(
        &NativeTransport,
        source,
        destination,
        source_password,
        password,
        fingerprint,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault;
    use std::sync::Mutex;

    struct State {
        source: Value,
        destination: Value,
        reads: usize,
        change_on_read: Option<(usize, bool)>,
        fail_write: bool,
        discard_write: bool,
        fail_read: Option<usize>,
        writes: Vec<(String, Option<String>)>,
    }

    struct Fake(Mutex<State>);
    impl Fake {
        fn new(content: &str) -> Self {
            Self(Mutex::new(State {
                source: json!({"id":1,"content":content,"modified_at":"v1"}),
                destination: json!({"id":2,"content":"profiles: []","modified_at":"v1"}),
                reads: 0,
                change_on_read: None,
                fail_write: false,
                discard_write: false,
                fail_read: None,
                writes: vec![],
            }))
        }
    }

    impl Transport for Fake {
        async fn request(
            &self,
            connection: &Connection,
            content: Option<String>,
            etag: Option<String>,
        ) -> Result<sync::Response> {
            let mut state = self.0.lock().unwrap();
            let source = connection.configuration_id == 1;
            assert_eq!(
                connection.token,
                if source { "old-token" } else { "new-token" }
            );
            if let Some(content) = content {
                assert!(!source, "Migration must never write to the source");
                assert_eq!(connection.server, "https://new.example.com");
                state.writes.push((content.clone(), etag));
                if state.fail_write {
                    return Err("HTTP 412".into());
                }
                if !state.discard_write {
                    state.destination["content"] = json!(content);
                }
                return Ok(sync::Response {
                    body: Value::Null,
                    etag: None,
                });
            }
            state.reads += 1;
            if state.fail_read == Some(state.reads) {
                return Err("Cannot reach the destination.".into());
            }
            if let Some((read, change_source)) = state.change_on_read
                && state.reads == read
            {
                let target = if change_source {
                    &mut state.source
                } else {
                    &mut state.destination
                };
                target["modified_at"] = json!("v2");
            }
            let body = if source {
                state.source.clone()
            } else {
                state.destination.clone()
            };
            Ok(sync::Response {
                etag: Some(format!(
                    "{}-{}",
                    if source { "source" } else { "destination" },
                    body["modified_at"].as_str().unwrap()
                )),
                body,
            })
        }
    }

    fn connections() -> (Connection, Connection) {
        (
            Connection {
                server: "https://old.example.com".into(),
                token: "old-token".into(),
                configuration_id: 1,
            },
            Connection {
                server: "https://new.example.com".into(),
                token: "new-token".into(),
                configuration_id: 2,
            },
        )
    }

    fn run(
        fake: &Fake,
        source_password: &str,
        password: &str,
        fingerprint: Option<String>,
    ) -> Result<Value> {
        let (source, destination) = connections();
        tauri::async_runtime::block_on(migrate(
            fake,
            source,
            destination,
            source_password.into(),
            password.into(),
            fingerprint,
        ))
    }

    fn preview(fake: &Fake, source_password: &str) -> String {
        run(fake, source_password, "", None).unwrap()["fingerprint"]
            .as_str()
            .unwrap()
            .into()
    }

    fn content(config: &Value, secrets: &Value, encrypted: bool) -> String {
        let envelope = vault::encrypt(
            &json!({"config":config,"secrets":secrets,"customVaultField":"retain"}),
            "old password",
            Encryption::Tabby,
        )
        .unwrap();
        let root = if encrypted {
            json!({"encrypted":true,"vault":envelope})
        } else {
            let mut root = config.clone();
            root["vault"] = envelope;
            root
        };
        serde_yaml_ng::to_string(&root).unwrap()
    }

    #[test]
    fn migrates_both_legacy_forms_without_projecting_or_changing_the_source() {
        let config = json!({
            "profiles":[
                {"id":"ssh:one","type":"ssh","options":{"privateKeys":["vault://key"],"proxyCommand":"retained"}},
                {"id":"local:one","type":"local","isTemplate":true},
                {"id":"serial:one","type":"serial"}
            ],
            "ssh":{"knownHosts":[{"host":"example.com","digest":"retained"}]},
            "appearance":{"theme":"custom"},
            "pluginOptions":{"unknown":true}
        });
        let secrets = json!([
            {"type":"file","key":{"id":"key"},"value":"private-key-bytes"},
            {"type":"ssh:password","value":"saved-secret"},
            {"type":"ssh:key-passphrase","value":"saved-passphrase"},
            {"type":"custom:secret","value":"preserve"}
        ]);
        for encrypted in [false, true] {
            let original = content(&config, &secrets, encrypted);
            let fake = Fake::new(&original);
            let result = run(&fake, "old password", "", None).unwrap();
            assert_eq!(result["profiles"], 3);
            assert_eq!(result["secrets"], 4);
            assert_eq!(result["fingerprints"], 1);
            assert_eq!(result["destinationHasContent"], true);
            assert!(!result.to_string().contains("saved-secret"));
            assert!(fake.0.lock().unwrap().writes.is_empty());
            run(
                &fake,
                "old password",
                "new password",
                Some(result["fingerprint"].as_str().unwrap().into()),
            )
            .unwrap();
            let state = fake.0.lock().unwrap();
            assert_eq!(state.source["content"], original);
            assert_eq!(state.writes.len(), 1);
            assert_eq!(state.writes[0].1.as_deref(), Some("destination-v1"));
            let migrated = &state.writes[0].0;
            assert!(!migrated.contains("saved-secret"));
            assert!(!migrated.contains("pluginOptions"));
            assert!(Document::open(migrated, "old password").is_err());
            let document = Document::open(migrated, "new password").unwrap();
            assert_eq!(document.root["vault"]["version"], 2);
            assert_eq!(document.config, config);
            assert_eq!(document.vault.as_ref().unwrap()["secrets"], secrets);
            assert_eq!(
                document.vault.as_ref().unwrap()["customVaultField"],
                "retain"
            );
        }
    }

    #[test]
    fn migrates_plaintext_and_independent_legacy_fixture() {
        let envelope: Value =
            serde_json::from_str(include_str!("fixtures/tabby-vault-v1.json")).unwrap();
        let fixture =
            serde_yaml_ng::to_string(&json!({"encrypted":true,"vault":envelope})).unwrap();
        for (content, password) in [
            ("profiles: []\ncustom: retained", ""),
            (fixture.as_str(), "fixture-password"),
        ] {
            let original = Document::open(content, password).unwrap();
            let fake = Fake::new(content);
            let fingerprint = preview(&fake, password);
            run(&fake, password, "new password", Some(fingerprint)).unwrap();
            let state = fake.0.lock().unwrap();
            let migrated = Document::open(&state.writes[0].0, "new password").unwrap();
            assert_eq!(migrated.config, original.config);
            assert_eq!(
                migrated.vault.unwrap()["secrets"],
                original.vault.unwrap_or(json!({"secrets":[]}))["secrets"]
            );
        }
    }

    #[test]
    fn rejects_wrong_password_malformed_data_and_oversized_source_without_writes() {
        let encrypted = content(&json!({"profiles":[]}), &json!([]), true);
        let oversized = "a".repeat(8 * 1024 * 1024 + 1);
        for (content, password) in [
            (encrypted.as_str(), "wrong"),
            (encrypted.as_str(), ""),
            ("profiles: [", ""),
            ("- invalid", ""),
            ("profiles: invalid", ""),
            (oversized.as_str(), ""),
        ] {
            let fake = Fake::new(content);
            assert!(run(&fake, password, "new password", None).is_err());
            assert!(fake.0.lock().unwrap().writes.is_empty());
        }
    }

    #[test]
    fn rejects_missing_password_and_unreviewed_or_changed_configurations() {
        for source in [true, false] {
            let fake = Fake::new("profiles: []");
            let fingerprint = preview(&fake, "");
            assert!(run(&fake, "", "", Some(fingerprint.clone())).is_err());
            assert!(run(&fake, "", "pw", Some("invalid".into())).is_err());
            {
                let mut state = fake.0.lock().unwrap();
                let target = if source {
                    &mut state.source
                } else {
                    &mut state.destination
                };
                target["content"] = json!("profiles: []\nchanged: true");
            }
            assert!(
                run(&fake, "", "pw", Some(fingerprint))
                    .unwrap_err()
                    .contains("since the preview")
            );
            assert!(fake.0.lock().unwrap().writes.is_empty());
        }
    }

    #[test]
    fn rejects_changes_during_conversion_and_propagates_conditional_write_failure() {
        for source in [true, false] {
            let fake = Fake::new("profiles: []");
            let fingerprint = preview(&fake, "");
            fake.0.lock().unwrap().change_on_read = Some((if source { 5 } else { 6 }, source));
            assert!(
                run(&fake, "", "pw", Some(fingerprint))
                    .unwrap_err()
                    .contains("during migration")
            );
            assert!(fake.0.lock().unwrap().writes.is_empty());
        }
        let fake = Fake::new("profiles: []");
        let fingerprint = preview(&fake, "");
        fake.0.lock().unwrap().fail_write = true;
        assert_eq!(
            run(&fake, "", "pw", Some(fingerprint)).unwrap_err(),
            "HTTP 412"
        );
        assert_eq!(
            fake.0.lock().unwrap().destination["content"],
            "profiles: []"
        );
    }

    #[test]
    fn only_reports_success_after_verifying_the_stored_encrypted_copy() {
        for fail_read in [false, true] {
            let fake = Fake::new("profiles: []\ncustom: source");
            let fingerprint = preview(&fake, "");
            {
                let mut state = fake.0.lock().unwrap();
                state.discard_write = !fail_read;
                state.fail_read = fail_read.then_some(7);
            }
            let error = run(&fake, "", "new password", Some(fingerprint)).unwrap_err();
            assert!(error.contains("upload finished"));
            assert!(error.contains("could not be verified"));
            let state = fake.0.lock().unwrap();
            assert_eq!(state.writes.len(), 1);
            assert_eq!(state.reads, 7);
            assert_eq!(state.source["content"], "profiles: []\ncustom: source");
        }
    }

    #[test]
    fn validates_endpoints_and_response_ids_before_writing() {
        let fake = Fake::new("profiles: []");
        let (mut source, mut destination) = connections();
        source.server = "https://EXAMPLE.com:443/base/".into();
        destination.server = " https://example.com/base ".into();
        destination.configuration_id = source.configuration_id;
        assert!(
            tauri::async_runtime::block_on(migrate(
                &fake,
                source,
                destination,
                "".into(),
                "pw".into(),
                None
            ))
            .unwrap_err()
            .contains("different destination")
        );
        assert_eq!(fake.0.lock().unwrap().reads, 0);
        for server in [
            "http://example.com",
            "https://user:password@example.com",
            "https://example.com?token=secret",
        ] {
            let (mut source, _) = connections();
            source.server = server.into();
            assert!(source.endpoint().is_err());
        }
        for id in [0, 9_007_199_254_740_992] {
            let (mut source, _) = connections();
            source.configuration_id = id;
            assert!(source.endpoint().is_err());
        }
        for source in [true, false] {
            let fake = Fake::new("profiles: []");
            {
                let mut state = fake.0.lock().unwrap();
                let target = if source {
                    &mut state.source
                } else {
                    &mut state.destination
                };
                target["id"] = json!(999);
            }
            assert!(run(&fake, "", "pw", None).is_err());
            assert!(fake.0.lock().unwrap().writes.is_empty());
        }
    }
}
