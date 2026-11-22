//! Tells the interface when a newer rightcommit release is published on GitHub.
//! The server asks GitHub at most once per cache period; the browser never calls GitHub.
use crate::{auth::ApiResult, model::User};
use axum::Json;
use serde_json::{Value, json};
use std::sync::OnceLock;

const LATEST_RELEASE: &str = "https://api.github.com/repos/rightcommit/rightcommit/releases/latest";
const RELEASES: &str = "https://github.com/rightcommit/rightcommit/releases/tag/";
/// A failed attempt is cached too, so a closed network costs one short request per period.
const CACHE_SECS: i64 = 6 * 3600;

/// `RIGHTCOMMIT_UPDATE_CHECK=0` turns the check off, for installations without Internet access.
fn enabled() -> bool {
    !std::env::var("RIGHTCOMMIT_UPDATE_CHECK").is_ok_and(|value| {
        ["0", "false", "off", "no"].contains(&value.trim().to_ascii_lowercase().as_str())
    })
}

/// Numeric `major.minor.patch` only: a pre-release or malformed tag is not a version to announce.
fn parse(version: &str) -> Option<[u64; 3]> {
    let mut parts = version.trim().trim_start_matches('v').split('.');
    let mut numbers = [0; 3];
    for number in &mut numbers {
        let part = parts.next()?;
        if part.is_empty() || part.len() > 9 || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        *number = part.parse().ok()?;
    }
    parts.next().is_none().then_some(numbers)
}

/// The published version, when the release is a regular one and its tag is a plain version.
fn release_version(release: &Value) -> Option<String> {
    if release["draft"].as_bool() == Some(true) || release["prerelease"].as_bool() == Some(true) {
        return None;
    }
    let [major, minor, patch] = parse(release["tag_name"].as_str()?)?;
    Some(format!("{major}.{minor}.{patch}"))
}

fn status(installed: &str, latest: Option<&str>) -> Value {
    let newer = latest
        .filter(|latest| parse(latest).zip(parse(installed)).is_some_and(|(latest, installed)| latest > installed));
    json!({
        "current": installed,
        "latest": newer,
        "available": newer.is_some(),
        "url": newer.map(|version| format!("{RELEASES}v{version}")),
    })
}

async fn fetch() -> Option<String> {
    let mut response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .ok()?
        .get(LATEST_RELEASE)
        // GitHub rejects requests without a User-Agent; it carries no account or installation data.
        .header("User-Agent", concat!("rightcommit/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len() + chunk.len() > 1024 * 1024 {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    release_version(&serde_json::from_slice(&bytes).ok()?)
}

async fn latest() -> Option<String> {
    static CACHE: OnceLock<tokio::sync::Mutex<(i64, Option<String>)>> = OnceLock::new();
    // Holding the lock across the request lets concurrent callers share one attempt.
    let mut cache = CACHE.get_or_init(|| tokio::sync::Mutex::new((0, None))).lock().await;
    let now = crate::model::now();
    if cache.0 == 0 || now < cache.0 || now - cache.0 >= CACHE_SECS {
        *cache = (now, fetch().await);
    }
    cache.1.clone()
}

pub async fn get(_user: User) -> ApiResult<Json<Value>> {
    let latest = if enabled() { latest().await } else { None };
    Ok(Json(status(env!("CARGO_PKG_VERSION"), latest.as_deref())))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_a_newer_regular_release_is_announced() {
        let release = |tag: &str| json!({"tag_name":tag,"draft":false,"prerelease":false});
        assert_eq!(release_version(&release("v1.7.0")).as_deref(), Some("1.7.0"));
        assert_eq!(release_version(&release("1.7.0")).as_deref(), Some("1.7.0"));
        // Drafts, pre-releases and tags that are not a plain version are ignored.
        assert!(release_version(&json!({"tag_name":"v1.7.0","draft":true})).is_none());
        assert!(release_version(&json!({"tag_name":"v1.7.0","prerelease":true})).is_none());
        for tag in ["v1.7.0-rc.1", "v1.7", "v1.7.0.1", "latest", "v1..0", "v1.7.x", "v-1.7.0", ""] {
            assert!(release_version(&release(tag)).is_none(), "{tag}");
        }
        assert!(release_version(&json!({"message":"Not Found"})).is_none());

        let newer = status("1.6.1", Some("1.7.0"));
        assert_eq!(newer["available"], true);
        assert_eq!(newer["current"], "1.6.1");
        assert_eq!(newer["latest"], "1.7.0");
        assert_eq!(newer["url"], "https://github.com/rightcommit/rightcommit/releases/tag/v1.7.0");
        // Versions compare as numbers, not as text.
        assert_eq!(status("1.9.0", Some("1.10.0"))["available"], true);
        assert_eq!(status("1.10.0", Some("1.9.9"))["available"], false);
        // The same version, a development build ahead of the release, and a failed check show nothing.
        for (installed, latest) in [("1.6.1", Some("1.6.1")), ("1.7.0", Some("1.6.1")), ("1.6.1", None), ("1.6.1", Some("soon"))] {
            let none = status(installed, latest);
            assert_eq!(none["available"], false);
            assert!(none["latest"].is_null() && none["url"].is_null());
        }
    }
}
