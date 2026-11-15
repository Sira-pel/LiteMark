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

/// The pre-release part of a version (`rc.1` in `1.2.3-rc.1+build`), if any.
fn pre_release(text: &str) -> Option<&str> {
    let text = text.trim();
    let without_build = text.split('+').next().unwrap_or(text);
    without_build.split_once('-').map(|(_, pre)| pre)
}

/// Semver precedence of two pre-release parts: identifiers are compared one
/// by one, numbers numerically and below words, and a longer list wins a tie.
fn compare_pre_release(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let mut a_ids = a.split('.');
    let mut b_ids = b.split('.');
    loop {
        match (a_ids.next(), b_ids.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let order = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

/// Is `latest` newer than `current`? A pre-release comes before its release:
/// `1.2.3-rc1` < `1.2.3`.
pub fn is_newer(current: &str, latest: &str) -> bool {
    use std::cmp::Ordering;
    let (Some(current_core), Some(latest_core)) = (parse_version(current), parse_version(latest))
    else {
        return false;
    };
    match latest_core.cmp(&current_core) {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => match (pre_release(current), pre_release(latest)) {
            (Some(_), None) => true,
            (Some(current_pre), Some(latest_pre)) => {
                compare_pre_release(latest_pre, current_pre) == Ordering::Greater
            }
            (None, _) => false,
        },
    }
}

/// Longest `tag_name` accepted. Real tags look like `v1.2.3`.
const MAX_TAG_LEN: usize = 64;

/// A tag that is safe to put in the release link as one path segment: a
/// version (`v1.2`, `v1.2.3-rc.1`) made only of letters, digits, `.` and `-`.
fn link_safe_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        && parse_version(tag).is_some()
}

/// Read the fields we need from a GitHub "latest release" response.
///
/// The version shown in the banner is rebuilt from the numbers, so a
/// tampered response can't put its own text there. The Download link uses
/// the release's real `tag_name` when it is a plain version (so `v1.2` links
/// to `tag/v1.2`, not a `tag/v1.2.0` that doesn't exist), and is rebuilt from
/// the numbers otherwise. `html_url` is never used, so the link can't point
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
    let url = if link_safe_tag(tag) {
        format!("{RELEASE_PAGE_PREFIX}tag/{tag}")
    } else {
        format!("{RELEASE_PAGE_PREFIX}tag/v{version}")
    };
    Some(ReleaseInfo { url, version })
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
    fn pre_releases_come_before_their_release() {
        assert!(is_newer("1.2.3-rc1", "1.2.3"));
        assert!(is_newer("1.2.3-rc.1", "v1.2.3"));
        assert!(!is_newer("1.2.3", "1.2.3-rc1"));
        assert!(is_newer("1.2.3-rc.1", "1.2.3-rc.2"));
        assert!(is_newer("1.2.3-rc.2", "1.2.3-rc.10"));
        assert!(is_newer("1.2.3-alpha", "1.2.3-beta"));
        assert!(is_newer("1.2.3-rc", "1.2.3-rc.1"));
        assert!(!is_newer("1.2.3-rc.1", "1.2.3-rc.1"));
        assert!(!is_newer("1.2.4-rc1", "1.2.3"));
        assert!(!is_newer("1.2.3+build5", "1.2.3"));
    }

    #[test]
    fn link_uses_the_real_tag_when_it_is_a_plain_version() {
        let info = parse_release(r#"{"tag_name":"v2.1"}"#).expect("release");
        assert_eq!(info.version, "2.1.0");
        assert_eq!(info.url, "https://github.com/Sira-pel/LiteMark/releases/tag/v2.1");
        let info = parse_release(r#"{"tag_name":"2.1.0"}"#).expect("release");
        assert_eq!(info.url, "https://github.com/Sira-pel/LiteMark/releases/tag/2.1.0");
        // Anything else in the tag: the link is rebuilt from the numbers.
        for tag in ["v2.1.0-x/../../evil", "v2.1.0-%2e%2e", "v2.1.0-now go", "v2.1.0-x?y=1"] {
            let body = serde_json::json!({ "tag_name": tag }).to_string();
            let info = parse_release(&body).expect("release");
            assert_eq!(info.url, "https://github.com/Sira-pel/LiteMark/releases/tag/v2.1.0", "{tag}");
        }
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
