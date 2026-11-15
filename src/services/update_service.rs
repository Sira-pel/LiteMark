use std::time::Duration;

const RELEASE_PAGE_PREFIX: &str = "https://github.com/Sira-pel/LiteMark/releases/";
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

/// Longest `tag_name` accepted. Real tags look like `v1.2.3`.
const MAX_TAG_LEN: usize = 64;

/// Read the fields we need from a GitHub "latest release" response.
///
/// Only the version numbers are taken from the response. The version shown
/// in the banner is rebuilt from them, and so is the Download link, so a
/// tampered response can't put its own text in the banner or point the link
/// elsewhere on github.com (for example with `releases/../../other/repo`).
fn parse_release(body: &str) -> Option<ReleaseInfo> {
    let json: serde_json::Value = serde_json::from_str(body).ok()?;
    if json["draft"].as_bool().unwrap_or(false) || json["prerelease"].as_bool().unwrap_or(false) {
        return None;
    }
    let tag = json["tag_name"].as_str()?;
    if tag.len() > MAX_TAG_LEN {
        return None;
    }
    let (major, minor, patch) = parse_version(tag)?;
    let version = format!("{major}.{minor}.{patch}");
    Some(ReleaseInfo {
        url: format!("{RELEASE_PAGE_PREFIX}tag/v{version}"),
        version,
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
/// A last check in the future means the clock was changed, so check again.
pub fn check_is_due(last_check: Option<u64>, now: u64) -> bool {
    match last_check {
        Some(last) if last <= now => now - last >= CHECK_INTERVAL_SECS,
        _ => true,
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
    fn ignores_drafts_prereleases_and_bad_tags() {
        let draft = r#"{"tag_name":"v2.0.0","html_url":"https://github.com/Sira-pel/LiteMark/releases/tag/v2.0.0","draft":true}"#;
        let pre = r#"{"tag_name":"v2.0.0","html_url":"https://github.com/Sira-pel/LiteMark/releases/tag/v2.0.0","prerelease":true}"#;
        assert_eq!(parse_release(draft), None);
        assert_eq!(parse_release(pre), None);
        assert_eq!(parse_release(r#"{"tag_name":"latest"}"#), None);
        assert_eq!(
            parse_release(
                r#"{"html_url":"https://github.com/Sira-pel/LiteMark/releases/tag/v2.0.0"}"#
            ),
            None
        );
        assert_eq!(parse_release("not json"), None);
    }

    /// The link is built from the version, so whatever `html_url` says
    /// (another site, another repo, `..` or `%2e%2e` segments) is ignored.
    #[test]
    fn download_link_comes_from_the_version_not_html_url() {
        let expected = "https://github.com/Sira-pel/LiteMark/releases/tag/v2.0.0";
        for html_url in [
            "https://evil.example/download",
            "https://github.com/someone/else/releases/tag/v2.0.0",
            "https://github.com/Sira-pel/LiteMark/releases/../../../evil-org/evil-repo/releases/tag/v2.0.0",
            "https://github.com/Sira-pel/LiteMark/releases/%2e%2e/%2e%2e/%2e%2e/evil-org",
        ] {
            let body = serde_json::json!({ "tag_name": "v2.0.0", "html_url": html_url }).to_string();
            let info = parse_release(&body).expect("release");
            assert_eq!(info.url, expected, "html_url {html_url}");
        }
        // A missing html_url is fine too.
        let info = parse_release(r#"{"tag_name":"v2.0.0"}"#).expect("release");
        assert_eq!(info.url, expected);
    }

    /// Only the numbers reach the banner: no words or newlines after them,
    /// and no huge tags.
    #[test]
    fn banner_version_is_only_the_numbers() {
        let body = serde_json::json!({
            "tag_name": "v9.9.9-URGENT: your copy is infected.\nGo to evil.example now",
        })
        .to_string();
        let info = parse_release(&body).expect("release");
        assert_eq!(info.version, "9.9.9");
        assert_eq!(
            info.url,
            "https://github.com/Sira-pel/LiteMark/releases/tag/v9.9.9"
        );

        let info = parse_release(r#"{"tag_name":"v2"}"#).expect("release");
        assert_eq!(info.version, "2.0.0");

        let huge = format!("v1.0.0-{}", "x".repeat(5_000_000));
        let body = serde_json::json!({ "tag_name": huge }).to_string();
        assert_eq!(parse_release(&body), None);
    }

    #[test]
    fn checks_at_most_once_a_day() {
        assert!(check_is_due(None, 1_000));
        assert!(!check_is_due(Some(1_000), 1_000 + CHECK_INTERVAL_SECS - 1));
        assert!(check_is_due(Some(1_000), 1_000 + CHECK_INTERVAL_SECS));
        // A clock that moved backwards must not block checks forever.
        assert!(check_is_due(Some(5_000), 1_000));
    }
}
