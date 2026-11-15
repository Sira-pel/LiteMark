//! Which Markdown link targets the editor may hand to the OS.
//!
//! A link's label can say anything, and `open_url` passes the target to the
//! platform opener (`ShellExecuteW`, `xdg-open`, `open`). Left unchecked, a
//! note from an untrusted source could open a local program, a `\\host\share`
//! path (leaking NTLM credentials on Windows) or a custom protocol handler
//! such as `ms-msdt:`. Only web and email links are opened.

/// Schemes the editor opens on Ctrl/Cmd+click.
const ALLOWED_SCHEMES: [&str; 3] = ["http", "https", "mailto"];

/// The target to open, or `None` when the link must not be opened.
///
/// Accepts `http://` and `https://` URLs with a host, and `mailto:` links.
/// Rejects every other scheme, paths (absolute, relative, UNC), bare
/// fragments, and any target with whitespace, control characters or
/// backslashes, which platform openers interpret inconsistently.
pub fn openable_link(raw: &str) -> Option<&str> {
    let url = raw.trim();
    if url.is_empty()
        || url
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
    {
        return None;
    }
    let (scheme, rest) = url.split_once(':')?;
    let scheme_ok = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !scheme_ok {
        return None;
    }
    let scheme = scheme.to_ascii_lowercase();
    if !ALLOWED_SCHEMES.contains(&scheme.as_str()) {
        return None;
    }
    if scheme == "mailto" {
        return (!rest.is_empty()).then_some(url);
    }
    // http(s): require `//` followed by a non-empty host.
    let authority = rest.strip_prefix("//")?;
    let host_end = authority.find(['/', '?', '#']).unwrap_or(authority.len());
    let host = &authority[..host_end];
    let host = host.rsplit_once('@').map_or(host, |(_, host)| host);
    if host.is_empty() || host.starts_with(':') {
        return None;
    }
    Some(url)
}

/// A short, printable form of a blocked target for an error message.
pub fn describe_blocked_link(raw: &str) -> String {
    const MAX_CHARS: usize = 120;
    let shown: String = raw.trim().chars().take(MAX_CHARS).collect();
    let mut shown = shown.escape_debug().to_string();
    if raw.trim().chars().count() > MAX_CHARS {
        shown.push('…');
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_web_and_email_links() {
        for url in [
            "https://example.com",
            "http://example.com/path?q=1#frag",
            "HTTPS://Example.com",
            "https://user@example.com:8443/x",
            "https://[::1]/",
            "mailto:someone@example.com",
            "mailto:someone@example.com?subject=Hi",
            "  https://example.com  ",
        ] {
            assert_eq!(openable_link(url), Some(url.trim()), "{url}");
        }
    }

    #[test]
    fn blocks_other_schemes() {
        for url in [
            "file:///C:/Windows/System32/calc.exe",
            "file:///etc/passwd",
            "ms-msdt:/id PCWDiagnostic",
            "search-ms:query=x",
            "vscode://file/x",
            "javascript:alert(1)",
            "data:text/html,hi",
            "smb://host/share",
            "ftp://example.com",
            "c:/Windows/notepad.exe",
        ] {
            assert_eq!(openable_link(url), None, "{url}");
        }
    }

    #[test]
    fn blocks_paths_and_malformed_targets() {
        for url in [
            "",
            "   ",
            "\\\\attacker\\share\\x",
            "//attacker/share",
            "C:\\Windows\\notepad.exe",
            "/usr/bin/xterm",
            "../other.md",
            "notes.md",
            "#heading",
            "www.example.com",
            "http:example.com",
            "https://",
            "https:///path",
            "https://user@/path",
            "https://:80/",
            "https://example.com\\..\\x",
            "https://exa mple.com",
            "https://example.com/\nfile:///x",
            "https://example.com/\u{0}",
            "mailto:",
            "1http://example.com",
        ] {
            assert_eq!(openable_link(url), None, "{url:?}");
        }
    }

    #[test]
    fn blocked_description_is_short_and_printable() {
        assert_eq!(describe_blocked_link("file:///x\n"), "file:///x");
        assert_eq!(describe_blocked_link("a\u{7}b"), "a\\u{7}b");
        let long = "x".repeat(500);
        let shown = describe_blocked_link(&long);
        assert_eq!(shown.chars().count(), 121);
        assert!(shown.ends_with('…'));
    }
}
