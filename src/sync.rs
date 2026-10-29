use reqwest::{Client, Url, redirect::Policy};
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

const MAX_RESPONSE: usize = 12 * 1024 * 1024;
#[derive(Serialize)]
pub struct Response {
    body: Value,
    etag: Option<String>,
}
fn endpoint(server: &str, id: Option<u64>) -> Result<Url, String> {
    let mut url = Url::parse(server).map_err(|_| "Enter an HTTPS Tabby-Web server URL.")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Use an HTTPS server URL without credentials, query, or fragment.".into());
    }
    if id == Some(0) {
        return Err("Invalid configuration ID.".into());
    }
    let base = url.path().trim_end_matches('/');
    let path = match id {
        Some(id) => format!("{base}/api/1/configs/{id}"),
        None => format!("{base}/api/1/configs"),
    };
    url.set_path(&path);
    Ok(url)
}
#[tauri::command]
pub async fn tabby_request(
    server: String,
    token: String,
    configuration_id: Option<u64>,
    content: Option<String>,
    etag: Option<String>,
) -> Result<Response, String> {
    let url = endpoint(&server, configuration_id)?;
    if token.trim().is_empty() || token.len() > 8192 {
        return Err("Enter a synchronization token.".into());
    }
    if content.is_some() && configuration_id.is_none() {
        return Err("Select a configuration before syncing.".into());
    }
    if content.as_ref().is_some_and(|s| s.len() > 8 * 1024 * 1024) {
        return Err("Configuration exceeds 8 MB.".into());
    }
    let client = Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| "Cannot initialize HTTPS client.")?;
    let mut request = if let Some(content) = content {
        client
            .patch(url)
            .json(&serde_json::json!({"content":content}))
    } else {
        client.get(url)
    };
    request = request
        .bearer_auth(token)
        .header("Accept", "application/json");
    if let Some(etag) = etag {
        request = request.header("If-Match", etag);
    }
    // Do not include reqwest errors or remote bodies: they can contain tokens or vault content.
    let mut response = request.send().await.map_err(
        |_| "Cannot reach Tabby-Web. Check the URL, TLS certificate, and network connection.",
    )?;
    match response.status().as_u16() {
        200..=299 => (),
        401 | 403 => return Err("The synchronization token was rejected.".into()),
        409 | 412 => return Err("The remote configuration changed. Sync again.".into()),
        code => return Err(format!("Tabby-Web returned HTTP {code}.")),
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_RESPONSE as u64)
    {
        return Err("Tabby-Web response exceeds 12 MB.".into());
    }
    let etag = response
        .headers()
        .get("ETag")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Could not read the Tabby-Web response.")?
    {
        if bytes.len() + chunk.len() > MAX_RESPONSE {
            return Err("Tabby-Web response exceeds 12 MB.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).map_err(|_| "Tabby-Web returned invalid JSON.")?
    };
    Ok(Response { body, etag })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_subpath_and_validates_destination() {
        assert_eq!(
            endpoint("https://example.com/tabby/", Some(3))
                .unwrap()
                .as_str(),
            "https://example.com/tabby/api/1/configs/3"
        );
        for url in [
            "http://example.com",
            "https://user:pass@example.com",
            "https://example.com?token=secret",
            "https://example.com/#fragment",
        ] {
            assert!(endpoint(url, None).is_err());
        }
        assert!(endpoint("https://example.com", Some(0)).is_err());
    }
}
