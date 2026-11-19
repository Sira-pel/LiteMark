//! Sizes for images shown in notes: reading a picture's dimensions from its
//! file header, remembering them between frames, and fitting them to the
//! editor column. The picture itself is drawn by gpui's `img()`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use gpui::{px, Pixels};

/// Tallest an image block grows, before zoom.
pub const MAX_IMAGE_HEIGHT: Pixels = px(480.0);
/// Space above and below a picture, so neighbouring lines keep their distance.
pub const IMAGE_MARGIN_Y: Pixels = px(6.0);
/// Corner radius of the picture and of its placeholder.
pub const IMAGE_RADIUS: Pixels = px(6.0);

/// What a file looked like when its size was read.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

#[derive(Clone)]
struct Entry {
    stamp: Option<Stamp>,
    /// Pixel size, or `None` when the file is missing or isn't a picture.
    size: Option<(u32, u32)>,
}

/// Decoded sizes by path, so a frame never reads a file it has already seen.
/// A visible image is re-checked against the file's modified time and length
/// (one `stat`), so replacing a picture on disk shows the new one.
#[derive(Clone, Default)]
pub struct ImageSizeCache {
    entries: HashMap<PathBuf, Entry>,
}

impl ImageSizeCache {
    /// The size of the image at `path`. With `recheck`, a changed or newly
    /// created file is read again; without it a cached answer is trusted, so
    /// images far off screen cost no file access after the first read.
    pub fn size(&mut self, path: &Path, recheck: bool) -> Option<(u32, u32)> {
        if let Some(entry) = self.entries.get(path) {
            if !recheck {
                return entry.size;
            }
            if entry.stamp == stamp_of(path) {
                return entry.size;
            }
        }
        let stamp = stamp_of(path);
        let size = stamp.and_then(|_| read_image_size(path));
        self.entries
            .insert(path.to_path_buf(), Entry { stamp, size });
        size
    }

    /// Drops entries for paths that no longer appear in the note.
    pub fn retain_paths(&mut self, live: &std::collections::HashSet<PathBuf>) {
        if self.entries.len() > live.len() + 64 {
            self.entries.retain(|path, _| live.contains(path));
        }
    }

    pub fn has_entries(&self) -> bool {
        !self.entries.is_empty()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

fn stamp_of(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    meta.is_file().then(|| Stamp {
        modified: meta.modified().ok(),
        len: meta.len(),
    })
}

/// Width and height in pixels, read from the file header (SVG: its declared
/// size or `viewBox`).
pub fn read_image_size(path: &Path) -> Option<(u32, u32)> {
    let is_svg = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"));
    if is_svg {
        // The size sits in the opening tag, so a prefix is enough.
        use std::io::Read;
        let mut head = Vec::new();
        std::fs::File::open(path)
            .ok()?
            .take(64 * 1024)
            .read_to_end(&mut head)
            .ok()?;
        return svg_size(&String::from_utf8_lossy(&head));
    }
    let (w, h) = image::image_dimensions(path).ok()?;
    (w > 0 && h > 0).then_some((w, h))
}

/// The size an SVG asks for: `width`/`height`, else its `viewBox`.
pub fn svg_size(text: &str) -> Option<(u32, u32)> {
    let start = text.find("<svg")?;
    let tag = &text[start..];
    let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
    let attr = |name: &str| -> Option<f32> {
        let mut from = 0;
        while let Some(at) = tag[from..].find(name) {
            let at = from + at;
            from = at + name.len();
            let before_ok = tag[..at].ends_with(char::is_whitespace);
            let rest = tag[at + name.len()..].trim_start();
            let Some(rest) = rest.strip_prefix('=') else {
                continue;
            };
            let rest = rest.trim_start();
            let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''))?;
            if !before_ok {
                continue;
            }
            let body = &rest[1..];
            return Some(body[..body.find(quote)?].to_string()).and_then(|value| {
                let number: String = value
                    .trim()
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                // Percentages say nothing about the picture's own size.
                (!value.trim().ends_with('%'))
                    .then(|| number.parse::<f32>().ok())
                    .flatten()
            });
        }
        None
    };
    let positive = |v: f32| (v.is_finite() && v > 0.0).then_some(v);
    if let (Some(w), Some(h)) = (
        attr("width").and_then(positive),
        attr("height").and_then(positive),
    ) {
        return Some((w.round().max(1.0) as u32, h.round().max(1.0) as u32));
    }
    // viewBox="min-x min-y width height"
    let at = tag.find("viewBox")?;
    let rest = tag[at + "viewBox".len()..].trim_start().strip_prefix('=')?;
    let rest = rest.trim_start();
    let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    let body = &rest[1..];
    let body = &body[..body.find(quote)?];
    let nums: Vec<f32> = body
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    let (w, h) = (*nums.get(2)?, *nums.get(3)?);
    (positive(w).is_some() && positive(h).is_some())
        .then(|| (w.round().max(1.0) as u32, h.round().max(1.0) as u32))
}

/// The size to draw a `natural`-sized picture in a column `content_width`
/// wide: shrunk to fit the width and `max_height`, never enlarged, aspect
/// ratio kept.
pub fn fit_image(
    natural: (u32, u32),
    content_width: Pixels,
    max_height: Pixels,
) -> (Pixels, Pixels) {
    let (nw, nh) = (natural.0.max(1) as f32, natural.1.max(1) as f32);
    let scale = (f32::from(content_width) / nw)
        .min(f32::from(max_height) / nh)
        .clamp(0.0, 1.0);
    (px((nw * scale).max(1.0)), px((nh * scale).max(1.0)))
}

/// Height of the box shown instead of a picture that can't be drawn:
/// two text lines and padding.
pub fn placeholder_height(font_size: Pixels) -> Pixels {
    font_size * 1.65 * 2.0 + px(24.0)
}

/// Shortens `text` to at most `max_chars` characters, ending in `…`.
pub fn ellipsize(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let mut out: String = text.chars().take(keep).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_pictures_shrink_to_the_column() {
        let (w, h) = fit_image((2000, 1000), px(700.0), MAX_IMAGE_HEIGHT);
        assert_eq!((f32::from(w), f32::from(h)), (700.0, 350.0));
    }

    #[test]
    fn tall_pictures_are_capped_in_height_and_keep_their_ratio() {
        let (w, h) = fit_image((1000, 4000), px(700.0), MAX_IMAGE_HEIGHT);
        assert_eq!(f32::from(h), 480.0);
        assert_eq!(f32::from(w), 120.0);
    }

    #[test]
    fn small_pictures_are_not_enlarged() {
        let (w, h) = fit_image((64, 32), px(700.0), MAX_IMAGE_HEIGHT);
        assert_eq!((f32::from(w), f32::from(h)), (64.0, 32.0));
    }

    #[test]
    fn svg_size_reads_dimensions_then_viewbox() {
        assert_eq!(
            svg_size(r#"<?xml version="1.0"?><svg xmlns="x" width="120" height="60.5px">"#),
            Some((120, 61))
        );
        assert_eq!(
            svg_size(r#"<svg viewBox="0 0 300 150" width="100%">"#),
            Some((300, 150))
        );
        assert_eq!(svg_size("<svg>"), None);
        assert_eq!(svg_size("not svg"), None);
    }

    #[test]
    fn ellipsize_limits_length() {
        assert_eq!(ellipsize("short", 10), "short");
        assert_eq!(ellipsize("abcdefghij", 5), "abcd…");
    }

    #[test]
    fn sizes_are_cached_and_missing_files_are_remembered() {
        let dir = std::env::temp_dir().join(format!("litemark-img-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let svg = dir.join("a.svg");
        std::fs::write(&svg, r#"<svg width="10" height="20">"#).unwrap();
        let mut cache = ImageSizeCache::default();
        assert_eq!(cache.size(&svg, false), Some((10, 20)));
        // Deleted behind the cache's back: still answered without a read.
        std::fs::remove_file(&svg).unwrap();
        assert_eq!(cache.size(&svg, false), Some((10, 20)));
        // A visible image notices.
        assert_eq!(cache.size(&svg, true), None);
        let gone = dir.join("gone.png");
        assert_eq!(cache.size(&gone, true), None);
        assert_eq!(cache.len(), 2);
        // A file that appears later is found on the next visible check.
        std::fs::write(&gone, b"not an image").unwrap();
        assert_eq!(cache.size(&gone, true), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
