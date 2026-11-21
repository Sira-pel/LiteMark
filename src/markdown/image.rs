//! Markdown image lines: which lines are only an image, and where the
//! picture lives. Pure text work, so the editor's layout can reuse it and
//! tests need no window.

use std::path::{Path, PathBuf};

use crate::services::link_policy::openable_link;

/// A line made of one Markdown image, `![alt](target "title")`, and nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageLine {
    pub alt: String,
    pub target: String,
    pub title: Option<String>,
    /// Width in pixels from `![alt|200](path)`: the alt text ends in `|` and digits.
    pub width: Option<u32>,
}

/// Parses `line` as an image line. Surrounding whitespace is allowed; any
/// other text before or after the image makes it an ordinary paragraph.
pub fn parse_image_line(line: &str) -> Option<ImageLine> {
    let line = line.trim();
    let rest = line.strip_prefix("![")?;
    let alt_end = alt_end(rest)?;
    let (alt_raw, width) = split_alt_width(&rest[..alt_end]);
    let alt = unescape(alt_raw);
    let after = rest[alt_end + 1..].strip_prefix('(')?;
    let after = after.trim_start();

    // Destination: `<...>` (may hold spaces) or a run without whitespace
    // whose parentheses balance.
    let (target, after) = if let Some(inner) = after.strip_prefix('<') {
        let end = inner.find(['>', '\n'])?;
        if inner.as_bytes()[end] != b'>' {
            return None;
        }
        (inner[..end].to_string(), &inner[end + 1..])
    } else {
        let mut depth = 0usize;
        let mut end = after.len();
        let mut chars = after.char_indices();
        while let Some((at, ch)) = chars.next() {
            match ch {
                '\\' => {
                    chars.next();
                }
                '(' => depth += 1,
                ')' if depth == 0 => {
                    end = at;
                    break;
                }
                ')' => depth -= 1,
                c if c.is_whitespace() => {
                    end = at;
                    break;
                }
                _ => {}
            }
        }
        (unescape(&after[..end]), &after[end..])
    };
    if target.is_empty() {
        return None;
    }

    // Optional title, then the closing parenthesis and the end of the line.
    let after = after.trim_start();
    let (title, after) = match after.chars().next()? {
        ')' => (None, after),
        open @ ('"' | '\'' | '(') => {
            let close = if open == '(' { ')' } else { open };
            let body = &after[1..];
            let mut end = None;
            let mut chars = body.char_indices();
            while let Some((at, ch)) = chars.next() {
                if ch == '\\' {
                    chars.next();
                } else if ch == close {
                    end = Some(at);
                    break;
                }
            }
            let end = end?;
            (Some(unescape(&body[..end])), body[end + 1..].trim_start())
        }
        _ => return None,
    };
    if after != ")" {
        return None;
    }
    Some(ImageLine {
        alt,
        target,
        title,
        width,
    })
}

/// Where the alt text of `rest` (the text after `![`) ends: the matching `]`,
/// allowing nested brackets and `\]`.
fn alt_end(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut depth = 0usize;
    let mut i = 0usize;
    loop {
        match *bytes.get(i)? {
            b'\\' => i += 1,
            b'[' => depth += 1,
            b']' if depth == 0 => return Some(i),
            b']' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
}

/// Splits raw alt text `alt|200` into the alt and its width. The part after
/// the last unescaped `|` counts only when it is 1 to 4 digits and not zero;
/// anything else stays part of the alt.
pub fn split_alt_width(raw: &str) -> (&str, Option<u32>) {
    let Some(bar) = raw.rfind('|') else {
        return (raw, None);
    };
    let digits = &raw[bar + 1..];
    let escaped = raw[..bar].ends_with('\\');
    let valid = (1..=4).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit());
    match digits.parse::<u32>() {
        Ok(width) if valid && !escaped && width > 0 => (&raw[..bar], Some(width)),
        _ => (raw, None),
    }
}

/// `line` (a whole image line) with its width set to `width`, or removed for
/// `None`. Only the alt text changes; the target and title stay as written.
pub fn set_image_width(line: &str, width: Option<u32>) -> Option<String> {
    parse_image_line(line)?;
    let start = line.len() - line.trim_start().len() + 2;
    let end = start + alt_end(&line[start..])?;
    let (base, _) = split_alt_width(&line[start..end]);
    let mut out = String::with_capacity(line.len() + 6);
    out.push_str(&line[..start]);
    out.push_str(base);
    if let Some(width) = width {
        out.push_str(&format!("|{width}"));
    }
    out.push_str(&line[end..]);
    Some(out)
}

/// Markdown for an image at `path`: `![alt](path)`. A path with spaces or
/// parentheses goes in `<...>` so it still reads as one target.
pub fn image_markdown(alt: &str, path: &str) -> String {
    let alt = alt.replace(['[', ']'], "");
    if path.contains(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '<' | '>')) {
        format!("![{alt}](<{}>)", path.replace(['<', '>'], ""))
    } else {
        format!("![{alt}]({path})")
    }
}

/// Drops the backslash from escaped ASCII punctuation.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some(next) if next.is_ascii_punctuation() => out.push(next),
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Where an image target points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageTarget {
    /// A file on this computer.
    Local(PathBuf),
    /// An `http(s)` URL. Never fetched by default.
    Remote(String),
    /// A relative path in a note that has no folder yet, a network path, or a
    /// scheme the editor does not load.
    Unavailable,
}

/// Resolves an image `target` written in a note stored in `base_dir`.
/// Relative paths are taken from `base_dir`; absolute paths and `file://`
/// URLs are used as they are. Network paths (`\\host\share`, `//host/x`,
/// `file://host/x`) are refused: reading one makes Windows send credentials.
pub fn resolve_image_target(target: &str, base_dir: Option<&Path>) -> ImageTarget {
    let target = target.trim();
    if target.is_empty() || target.chars().any(char::is_control) {
        return ImageTarget::Unavailable;
    }
    let lower = target.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return match openable_link(target) {
            Some(url) => ImageTarget::Remote(url.to_string()),
            None => ImageTarget::Unavailable,
        };
    }
    if lower.starts_with("file:") {
        return match file_url_path(target) {
            Some(path) => ImageTarget::Local(path),
            None => ImageTarget::Unavailable,
        };
    }
    if has_url_scheme(target) {
        return ImageTarget::Unavailable;
    }
    // Checked again after decoding: `%5C%5Chost` is `\\host` once decoded.
    let decoded = percent_decode(target);
    if is_network_path(target) || is_network_path(&decoded) || decoded.chars().any(char::is_control)
    {
        return ImageTarget::Unavailable;
    }
    // A name that really contains `%20` wins over its decoded form.
    let path = Path::new(&decoded);
    if path.is_absolute() || is_rooted(&decoded) {
        return ImageTarget::Local(PathBuf::from(decoded));
    }
    match base_dir {
        Some(base) => {
            let decoded_path = base.join(&decoded);
            if decoded != target && !decoded_path.exists() {
                let literal = base.join(target);
                if literal.exists() {
                    return ImageTarget::Local(literal);
                }
            }
            ImageTarget::Local(decoded_path)
        }
        None => ImageTarget::Unavailable,
    }
}

/// `/x`, `\x` and `C:\x` count as absolute on every platform: on Windows
/// `Path` says `/x` is relative to the current drive, and elsewhere it says
/// `C:\x` is relative, which would join it onto the note's folder.
fn is_rooted(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || path.starts_with('\\')
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}

/// Two leading separators in any mix (`\\host`, `//host`, `/\host`, `\\?\UNC`)
/// name a network share or a device path on Windows. Reading one makes
/// Windows connect to that host and send the user's credentials.
fn is_network_path(path: &str) -> bool {
    let mut chars = path.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some('/' | '\\'), Some('/' | '\\'))
    )
}

/// `scheme:` with two or more letters, so `C:\x.png` is a path, not a URL.
fn has_url_scheme(target: &str) -> bool {
    let Some((scheme, _)) = target.split_once(':') else {
        return false;
    };
    scheme.len() >= 2
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// The local path of a `file:` URL: `file:///C:/a%20b.png` is `C:/a b.png`.
fn file_url_path(url: &str) -> Option<PathBuf> {
    let rest = url.get("file:".len()..)?;
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let path = if let Some(after) = rest.strip_prefix("//") {
        // Only an empty host or `localhost` names this computer.
        let slash = after.find('/')?;
        let host = &after[..slash];
        if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
            return None;
        }
        &after[slash..]
    } else {
        rest
    };
    let decoded = percent_decode(path);
    // `/C:/dir` carries a leading slash that is not part of a Windows path.
    let bytes = decoded.as_bytes();
    let trimmed = if bytes.len() >= 3
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':'
    {
        &decoded[1..]
    } else {
        decoded.as_str()
    };
    if trimmed.is_empty() || is_network_path(trimmed) || trimmed.chars().any(char::is_control) {
        return None;
    }
    Some(PathBuf::from(trimmed))
}

fn percent_decode(text: &str) -> String {
    if !text.contains('%') {
        return text.to_string();
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(hi * 16 + lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

fn hex(byte: u8) -> Option<u8> {
    (byte as char).to_digit(16).map(|d| d as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Option<(String, String, Option<String>)> {
        parse_image_line(line).map(|i| (i.alt, i.target, i.title))
    }

    fn alt_and_width(line: &str) -> (String, Option<u32>) {
        let image = parse_image_line(line).unwrap();
        (image.alt, image.width)
    }

    #[test]
    fn a_bar_and_digits_in_the_alt_set_the_width() {
        assert_eq!(
            alt_and_width("![cat|200](c.png)"),
            ("cat".into(), Some(200))
        );
        assert_eq!(alt_and_width("![|120](c.png)"), ("".into(), Some(120)));
        assert_eq!(alt_and_width("![a|b|64](c.png)"), ("a|b".into(), Some(64)));
    }

    #[test]
    fn other_bars_stay_in_the_alt() {
        for (line, alt) in [
            ("![a|b](c.png)", "a|b"),
            ("![a|](c.png)", "a|"),
            ("![a|0](c.png)", "a|0"),
            ("![a|12345](c.png)", "a|12345"),
            ("![a|20x10](c.png)", "a|20x10"),
            (r"![a\|200](c.png)", "a|200"),
        ] {
            assert_eq!(alt_and_width(line), (alt.into(), None), "{line}");
        }
    }

    #[test]
    fn set_image_width_rewrites_only_the_alt() {
        let line = "  ![cat|200](<my cat.png> \"A cat\")";
        assert_eq!(
            set_image_width(line, Some(320)).unwrap(),
            "  ![cat|320](<my cat.png> \"A cat\")"
        );
        assert_eq!(
            set_image_width(line, None).unwrap(),
            "  ![cat](<my cat.png> \"A cat\")"
        );
        assert_eq!(
            set_image_width("![](a.png)", Some(64)).unwrap(),
            "![|64](a.png)"
        );
        assert_eq!(set_image_width("not an image", Some(64)), None);
    }

    #[test]
    fn plain_image_lines_parse() {
        assert_eq!(
            parse("![a cat](cat.png)"),
            Some(("a cat".into(), "cat.png".into(), None))
        );
        assert_eq!(
            parse("  ![](assets/x.png)  "),
            Some((String::new(), "assets/x.png".into(), None))
        );
    }

    #[test]
    fn titles_are_read_in_every_quote_style() {
        for line in [
            r#"![a](p.png "My title")"#,
            "![a](p.png 'My title')",
            "![a](p.png (My title))",
            r#"![a]( p.png   "My title" )"#,
        ] {
            assert_eq!(
                parse(line),
                Some(("a".into(), "p.png".into(), Some("My title".into()))),
                "{line}"
            );
        }
    }

    #[test]
    fn angle_brackets_allow_spaces_and_parens_balance() {
        assert_eq!(
            parse("![a](<my pics/cat 1.png>)"),
            Some(("a".into(), "my pics/cat 1.png".into(), None))
        );
        assert_eq!(
            parse("![a](pics/cat_(1).png)"),
            Some(("a".into(), "pics/cat_(1).png".into(), None))
        );
        assert_eq!(
            parse(r"![a \] b [x]](p.png)"),
            Some(("a ] b [x]".into(), "p.png".into(), None))
        );
    }

    #[test]
    fn image_markdown_round_trips_through_the_parser() {
        for path in [
            "assets/image-1.png",
            "assets/my photo.png",
            "assets/a (1).png",
        ] {
            let line = image_markdown("", path);
            assert_eq!(
                parse(&line),
                Some((String::new(), path.into(), None)),
                "{line}"
            );
        }
        assert_eq!(image_markdown("", "assets/a.png"), "![](assets/a.png)");
        assert_eq!(
            image_markdown("a [b]", "assets/x y.png"),
            "![a b](<assets/x y.png>)"
        );
    }

    #[test]
    fn lines_with_other_content_are_not_image_lines() {
        for line in [
            "text ![a](p.png)",
            "![a](p.png) text",
            "![a](p.png)![b](q.png)",
            "[a](p.png)",
            "![a]",
            "![a]()",
            "![a](p.png",
            "![a](p.png extra)",
            r#"![a](p.png "open)"#,
            "- ![a](p.png)",
            "",
        ] {
            assert_eq!(parse(line), None, "{line:?}");
        }
    }

    #[test]
    fn relative_paths_resolve_against_the_notes_folder() {
        let base = Path::new("notes");
        assert_eq!(
            resolve_image_target("assets/a.png", Some(base)),
            ImageTarget::Local(base.join("assets/a.png"))
        );
        assert_eq!(
            resolve_image_target("./a%20b.png", Some(base)),
            ImageTarget::Local(base.join("./a b.png"))
        );
        assert_eq!(
            resolve_image_target("../a.png", Some(base)),
            ImageTarget::Local(base.join("../a.png"))
        );
        assert_eq!(
            resolve_image_target("assets/a.png", None),
            ImageTarget::Unavailable
        );
    }

    #[test]
    fn absolute_paths_and_file_urls_ignore_the_notes_folder() {
        let base = Path::new("notes");
        assert_eq!(
            resolve_image_target("/pics/a.png", Some(base)),
            ImageTarget::Local(PathBuf::from("/pics/a.png"))
        );
        assert_eq!(
            resolve_image_target("C:\\pics\\a.png", Some(base)),
            ImageTarget::Local(PathBuf::from("C:\\pics\\a.png"))
        );
        assert_eq!(
            resolve_image_target("file:///C:/my%20pics/a.png", Some(base)),
            ImageTarget::Local(PathBuf::from("C:/my pics/a.png"))
        );
        assert_eq!(
            resolve_image_target("file:///home/me/a.png", None),
            ImageTarget::Local(PathBuf::from("/home/me/a.png"))
        );
        assert_eq!(
            resolve_image_target("file://localhost/home/me/a.png", None),
            ImageTarget::Local(PathBuf::from("/home/me/a.png"))
        );
    }

    #[test]
    fn web_urls_are_remote_and_network_paths_are_refused() {
        assert_eq!(
            resolve_image_target("https://example.com/a.png", None),
            ImageTarget::Remote("https://example.com/a.png".into())
        );
        assert_eq!(
            resolve_image_target("HTTP://example.com/a.png", None),
            ImageTarget::Remote("HTTP://example.com/a.png".into())
        );
        for target in [
            "\\\\host\\share\\a.png",
            "//host/share/a.png",
            "file://host/share/a.png",
            "file:////host/share/a.png",
            // Network paths hidden behind percent escapes or mixed slashes.
            "%5C%5Chost%5Cshare%5Ca.png",
            "%2F%2Fhost%2Fshare%2Fa.png",
            "%5C%2Fhost/share/a.png",
            "/\\host\\share\\a.png",
            "\\/host/share/a.png",
            "file:///%5C%5Chost/share/a.png",
            "file:%2F%2Fhost/share/a.png",
            "a%00.png",
            "ftp://example.com/a.png",
            "data:image/png;base64,AAAA",
            "https://",
            "",
        ] {
            assert_eq!(
                resolve_image_target(target, Some(Path::new("notes"))),
                ImageTarget::Unavailable,
                "{target:?}"
            );
        }
    }
}
