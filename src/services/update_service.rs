use std::time::Duration;

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/Sira-pel/LiteMark/releases/latest";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Minimum time between two network checks.
pub const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub version: String,
    pub url: String,
}

/// Parse `1.2.3`, `v1.2.3` or `1.2.3-rc.1` into `(major, minor, patch)`.
/// Missing minor/patch parts count as zero.
fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.trim().trim_start_matches('v');
    let core = core.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    let patch = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

pub fn is_newer(current: &str, latest: &str) -> bool {
    match (parse_version(current), parse_version(latest)) {
        (Some(current), Some(latest)) => latest > current,
        _ => false,
    }
}

/// Read the fields we need from a GitHub "latest release" response.
fn parse_release(body: &str) -> Option<ReleaseInfo> {
    let json: serde_json::Value = serde_json::from_str(body).ok()?;
    if json["draft"].as_bool().unwrap_or(false) || json["prerelease"].as_bool().unwrap_or(false) {
        return None;
    }
    let tag = json["tag_name"].as_str()?;
    parse_version(tag)?;
    let url = json["html_url"].as_str()?;
    if !url.starts_with("https://github.com/") {
        return None;
    }
    Some(ReleaseInfo {
        version: tag.trim_start_matches('v').to_string(),
        url: url.to_string(),
    })
}

/// Ask GitHub for the latest release. Any failure (offline, rate limited,
/// bad response) yields `None`. This blocks, so run it off the UI thread.
pub fn fetch_latest_release() -> Option<ReleaseInfo> {
    let agent = ureq::AgentBuilder::new()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(concat!("litemark/", env!("CARGO_PKG_VERSION")))
        .build();
    let body = agent
        .get(LATEST_RELEASE_URL)
        .set("Accept", "application/vnd.github+json")
        .call()
        .ok()?
        .into_string()
        .ok()?;
    parse_release(&body)
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Whether enough time has passed since the last check (unix seconds).
pub fn check_is_due(last_check: Option<u64>, now: u64) -> bool {
    match last_check {
        Some(last) => now.saturating_sub(last) >= CHECK_INTERVAL_SECS,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("1.0.0", "1.0.1"));
        assert!(is_newer("1.9.0", "1.10.0"));
        assert!(is_newer("1.0.0", "v2.0"));
        assert!(!is_newer("1.0.1", "1.0.1"));
        assert!(!is_newer("1.2.0", "1.1.9"));
    }

    #[test]
    fn unparseable_versions_are_never_newer() {
        assert!(!is_newer("1.0.0", "latest"));
        assert!(!is_newer("1.0.0", "1.0.0.1"));
        assert!(!is_newer("abc", "1.0.0"));
    }

    #[test]
    fn parses_a_release_response() {
        let body = r#"{"tag_name":"v1.2.0","html_url":"https://github.com/Sira-pel/LiteMark/releases/tag/v1.2.0","draft":false,"prerelease":false}"#;
        let info = parse_release(body).expect("release");
        assert_eq!(info.version, "1.2.0");
        assert!(info.url.ends_with("v1.2.0"));
    }

    #[test]
    fn ignores_drafts_prereleases_and_foreign_links() {
        let draft = r#"{"tag_name":"v2.0.0","html_url":"https://github.com/x/y","draft":true}"#;
        let pre = r#"{"tag_name":"v2.0.0","html_url":"https://github.com/x/y","prerelease":true}"#;
        let foreign = r#"{"tag_name":"v2.0.0","html_url":"https://evil.example/download"}"#;
        assert_eq!(parse_release(draft), None);
        assert_eq!(parse_release(pre), None);
        assert_eq!(parse_release(foreign), None);
        assert_eq!(parse_release("not json"), None);
    }

    #[test]
    fn checks_at_most_once_a_day() {
        assert!(check_is_due(None, 1_000));
        assert!(!check_is_due(Some(1_000), 1_000 + CHECK_INTERVAL_SECS - 1));
        assert!(check_is_due(Some(1_000), 1_000 + CHECK_INTERVAL_SECS));
        // A clock that moved backwards must not block checks forever.
        assert!(!check_is_due(Some(5_000), 1_000));
    }
}
