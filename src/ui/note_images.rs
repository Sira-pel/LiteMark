//! Sizes for images shown in notes: reading a picture's dimensions from its
//! file header, remembering them between frames, and fitting them to the
//! editor column. The picture itself is drawn by gpui's `img()`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use gpui::{point, px, size, Bounds, Pixels};

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
        self.size_checked(path, recheck).0
    }

    /// [`Self::size`], and whether the file changed since it was last read,
    /// so the caller can drop the stale picture from gpui's image cache.
    pub fn size_checked(&mut self, path: &Path, recheck: bool) -> (Option<(u32, u32)>, bool) {
        let stamp = match self.entries.get(path) {
            Some(entry) if !recheck => return (entry.size, false),
            Some(entry) => {
                let stamp = stamp_of(path);
                if entry.stamp == stamp {
                    return (entry.size, false);
                }
                stamp
            }
            None => stamp_of(path),
        };
        let changed = self.entries.contains_key(path);
        let size = stamp.and_then(|_| read_image_size(path));
        self.entries
            .insert(path.to_path_buf(), Entry { stamp, size });
        (size, changed)
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

/// The size for a picture given an explicit `width`: that width (enlarging
/// the picture if need be, but never past `content_width`), aspect ratio
/// kept. The height limit of `fit_image` does not apply.
pub fn fit_image_width(natural: (u32, u32), width: u32, content_width: Pixels) -> (Pixels, Pixels) {
    let (nw, nh) = (natural.0.max(1) as f32, natural.1.max(1) as f32);
    let w = (width as f32).min(f32::from(content_width)).max(1.0);
    (px(w), px((nh * w / nw).max(1.0)))
}

/// The size for a picture with an explicit `width` and, optionally, `height`
/// (`|200x100`): both as written, so the picture is stretched to fit. Never
/// wider than `content_width`; the height shrinks with the width then. Without
/// a height this is [`fit_image_width`].
pub fn fit_image_size(
    natural: (u32, u32),
    width: u32,
    height: Option<u32>,
    content_width: Pixels,
) -> (Pixels, Pixels) {
    let Some(height) = height else {
        return fit_image_width(natural, width, content_width);
    };
    let width = width.max(1) as f32;
    let w = width.min(f32::from(content_width)).max(1.0);
    (px(w), px((height.max(1) as f32 * w / width).max(1.0)))
}

/// Height of a thumbnail in a row of images, before zoom.
pub const THUMB_HEIGHT: Pixels = px(160.0);
/// Space between thumbnails.
pub const THUMB_GAP: Pixels = px(8.0);

/// The size of one thumbnail: its own `width` when it has one (as for a lone
/// picture), otherwise `THUMB_HEIGHT` tall with the picture's aspect ratio,
/// or its natural height when that is smaller (a small picture is not
/// enlarged). Never wider than the column. A picture of unknown size is a
/// square.
pub fn thumbnail_size(
    natural: Option<(u32, u32)>,
    width: Option<u32>,
    height: Option<u32>,
    content_width: Pixels,
) -> (Pixels, Pixels) {
    let column = f32::from(content_width).max(1.0);
    let unknown = f32::from(THUMB_HEIGHT);
    let (nw, nh) = natural.map_or((unknown, unknown), |(w, h)| {
        (w.max(1) as f32, h.max(1) as f32)
    });
    let w = match width {
        Some(w) => w as f32,
        None => f32::from(THUMB_HEIGHT).min(nh) * nw / nh,
    }
    .clamp(1.0, column);
    match (width, height) {
        (Some(wanted), Some(h)) => (
            px(w),
            px((h.max(1) as f32 * w / wanted.max(1) as f32).max(1.0)),
        ),
        _ => (px(w), px((nh * w / nw).max(1.0))),
    }
}

/// Smallest width a picture can be dragged to.
pub const MIN_DRAG_WIDTH: f32 = 24.0;
/// Largest width written to a note: the `|width` syntax takes up to 4 digits.
pub const MAX_WIDTH: f32 = 9999.0;
/// Side of the square handle drawn on the selected picture's corner.
pub const HANDLE_SIZE: Pixels = px(10.0);
/// Side of the area that grabs the handle, larger than what is drawn.
pub const HANDLE_GRAB: Pixels = px(18.0);

/// How far the pointer must travel before a press on a handle is a drag, so a
/// click that wobbles by a pixel does not write a size.
pub const DRAG_SLOP: f32 = 3.0;

/// Which edge of a picture a drag holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragAxis {
    /// The bottom-right corner: width and height together, ratio kept.
    Corner,
    /// The right edge: the width alone.
    Right,
    /// The bottom edge: the height alone.
    Bottom,
}

/// A picture being resized by a handle. The note is untouched until the drag
/// ends; `width` and `height` are what is drawn meanwhile.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageDrag {
    /// Source range of the picture (or the one image of a row) being dragged.
    pub range: std::ops::Range<usize>,
    /// The tab the picture is in, so a drag never lands in another note.
    pub tab_id: String,
    /// The picture's Markdown when the drag began. The size is written only
    /// if the note still holds exactly this there when the drag ends.
    pub source: String,
    /// The pointer has moved far enough to count as a drag, not a click.
    pub active: bool,
    pub axis: DragAxis,
    pub start_x: Pixels,
    pub start_y: Pixels,
    pub start_width: Pixels,
    pub start_height: Pixels,
    /// Width of the editor column: the most a picture can be dragged to.
    pub column: Pixels,
    pub width: Pixels,
    pub height: Pixels,
    /// The picture already had a height (`|200x100`) when the drag began.
    pub had_height: bool,
}

impl ImageDrag {
    /// The size to write for the picture when the drag ends: a corner drag of
    /// a picture that only had a width stays width-only (its natural ratio),
    /// everything else writes both numbers.
    pub fn result(&self) -> (u32, Option<u32>) {
        let w = f32::from(self.width).round().clamp(1.0, MAX_WIDTH) as u32;
        let h = f32::from(self.height).round().clamp(1.0, MAX_WIDTH) as u32;
        if self.axis == DragAxis::Corner && !self.had_height {
            (w, None)
        } else {
            (w, Some(h))
        }
    }

    /// True when the drag changed the size from where it began.
    pub fn moved(&self) -> bool {
        self.width != self.start_width || self.height != self.start_height
    }

    /// The drag moved by (`dx`, `dy`) from where it was grabbed.
    pub fn update(&mut self, dx: Pixels, dy: Pixels) {
        if !self.active {
            if f32::from(dx).abs().max(f32::from(dy).abs()) < DRAG_SLOP {
                return;
            }
            self.active = true;
        }
        let (w, h) = drag_size(
            self.axis,
            (self.start_width, self.start_height),
            (dx, dy),
            self.column,
        );
        self.width = w;
        self.height = h;
    }
}

/// The size for a handle dragged by (`dx`, `dy`) from where it was grabbed:
/// at least `MIN_DRAG_WIDTH` either way, at most the column wide and
/// `MAX_WIDTH` either way, whole pixels. A corner keeps the ratio, following
/// whichever direction moved further.
pub fn drag_size(
    axis: DragAxis,
    start: (Pixels, Pixels),
    (dx, dy): (Pixels, Pixels),
    column: Pixels,
) -> (Pixels, Pixels) {
    let most_w = f32::from(column).clamp(MIN_DRAG_WIDTH, MAX_WIDTH);
    let (sw, sh) = (f32::from(start.0).max(1.0), f32::from(start.1).max(1.0));
    let (dx, dy) = (f32::from(dx), f32::from(dy));
    let min_h = MIN_DRAG_WIDTH;
    match axis {
        DragAxis::Right => (
            px((sw + dx).clamp(MIN_DRAG_WIDTH, most_w).round()),
            px(sh.clamp(1.0, MAX_WIDTH)),
        ),
        DragAxis::Bottom => (
            px(sw.clamp(1.0, MAX_WIDTH)),
            px((sh + dy).clamp(min_h, MAX_WIDTH).round()),
        ),
        DragAxis::Corner => {
            let ratio = sh / sw;
            // The scale that follows the pointer along its larger move.
            let by_x = (sw + dx) / sw;
            let by_y = (sh + dy) / sh;
            let scale = if dx.abs() * ratio >= dy.abs() {
                by_x
            } else {
                by_y
            };
            let lowest = (MIN_DRAG_WIDTH.min(sw) / sw).max(min_h.min(sh) / sh);
            let highest = (most_w / sw).min(MAX_WIDTH / sh);
            let scale = scale.clamp(lowest.min(highest), highest);
            (
                px((sw * scale).round().clamp(1.0, most_w)),
                px((sh * scale).round().clamp(1.0, MAX_WIDTH)),
            )
        }
    }
}

/// The area of `picture` that grabs its corner handle: the bottom-right corner.
pub fn handle_grab_area(picture: Bounds<Pixels>) -> Bounds<Pixels> {
    let side = HANDLE_GRAB.min(picture.size.width).min(picture.size.height);
    Bounds::new(
        point(picture.right() - side, picture.bottom() - side),
        size(side, side),
    )
}

/// The strip along the right edge that grabs the width alone, clear of the
/// corner handle.
pub fn right_edge_area(picture: Bounds<Pixels>) -> Bounds<Pixels> {
    let corner = handle_grab_area(picture);
    let grab = px(8.0).min(picture.size.width);
    Bounds::new(
        point(picture.right() - grab / 2.0, picture.top()),
        size(
            grab,
            (picture.size.height - corner.size.height).max(px(0.0)),
        ),
    )
}

/// The strip along the bottom edge that grabs the height alone, clear of the
/// corner handle.
pub fn bottom_edge_area(picture: Bounds<Pixels>) -> Bounds<Pixels> {
    let corner = handle_grab_area(picture);
    let grab = px(8.0).min(picture.size.height);
    Bounds::new(
        point(picture.left(), picture.bottom() - grab / 2.0),
        size((picture.size.width - corner.size.width).max(px(0.0)), grab),
    )
}

/// Which handle of `picture` is at `at`, if any. The corner wins where areas meet.
pub fn handle_at(picture: Bounds<Pixels>, at: &gpui::Point<Pixels>) -> Option<DragAxis> {
    if handle_grab_area(picture).contains(at) {
        Some(DragAxis::Corner)
    } else if right_edge_area(picture).contains(at) {
        Some(DragAxis::Right)
    } else if bottom_edge_area(picture).contains(at) {
        Some(DragAxis::Bottom)
    } else {
        None
    }
}

/// Where each thumbnail goes when they flow left to right and wrap to a new
/// line when the next one would pass `max_width`: each one's top-left corner
/// (relative to the block) and the block's total height. A line is as tall as
/// its tallest thumbnail.
pub fn flow_thumbnails(
    sizes: &[(Pixels, Pixels)],
    max_width: Pixels,
    gap: Pixels,
) -> (Vec<(Pixels, Pixels)>, Pixels) {
    let mut spots = Vec::with_capacity(sizes.len());
    let (mut x, mut y, mut line_h) = (px(0.0), px(0.0), px(0.0));
    for &(w, h) in sizes {
        if x > px(0.0) && x + w > max_width {
            y += line_h + gap;
            x = px(0.0);
            line_h = px(0.0);
        }
        spots.push((x, y));
        x += w + gap;
        line_h = line_h.max(h);
    }
    (spots, y + line_h)
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
    fn dragging_the_handle_clamps_to_the_minimum_the_column_and_four_digits() {
        let width = |start: f32, dx: f32, column: f32| {
            let (w, _) = drag_size(
                DragAxis::Right,
                (px(start), px(100.0)),
                (px(dx), px(0.0)),
                px(column),
            );
            f32::from(w)
        };
        assert_eq!(width(200.0, 50.0, 700.0), 250.0);
        assert_eq!(width(200.0, -500.0, 700.0), 24.0);
        assert_eq!(width(200.0, 900.0, 700.0), 700.0);
        // A column wider than four digits still stops at 9999.
        assert_eq!(width(200.0, 20000.0, 30000.0), 9999.0);
        assert_eq!(width(200.4, 0.2, 700.0), 201.0);
    }

    #[test]
    fn edge_drags_change_one_side_and_the_corner_keeps_the_ratio() {
        let start = (px(200.0), px(100.0));
        let size = |axis, dx: f32, dy: f32| {
            let (w, h) = drag_size(axis, start, (px(dx), px(dy)), px(700.0));
            (f32::from(w), f32::from(h))
        };
        assert_eq!(size(DragAxis::Right, 50.0, 30.0), (250.0, 100.0));
        assert_eq!(size(DragAxis::Bottom, 50.0, 30.0), (200.0, 130.0));
        assert_eq!(size(DragAxis::Bottom, 0.0, -500.0), (200.0, 24.0));
        // The corner follows the larger move and keeps 2:1.
        assert_eq!(size(DragAxis::Corner, 100.0, 0.0), (300.0, 150.0));
        assert_eq!(size(DragAxis::Corner, 0.0, 100.0), (400.0, 200.0));
        // Held at the column and the minimum, still 2:1.
        assert_eq!(size(DragAxis::Corner, 900.0, 0.0), (700.0, 350.0));
        let (w, h) = size(DragAxis::Corner, -900.0, -900.0);
        assert!(
            w >= 24.0 && h >= 24.0 && (w / h - 2.0).abs() < 0.1,
            "{w}x{h}"
        );
    }

    #[test]
    fn a_wobble_under_the_slop_is_not_a_drag() {
        let mut d = ImageDrag {
            range: 0..0,
            tab_id: String::new(),
            source: String::new(),
            active: false,
            axis: DragAxis::Right,
            start_x: px(0.0),
            start_y: px(0.0),
            start_width: px(200.0),
            start_height: px(100.0),
            column: px(700.0),
            width: px(200.0),
            height: px(100.0),
            had_height: false,
        };
        d.update(px(2.0), px(-2.0));
        assert!(!d.moved());
        d.update(px(10.0), px(0.0));
        assert!(d.moved());
        // Once going, coming back near the start still follows the pointer.
        d.update(px(1.0), px(0.0));
        assert_eq!(f32::from(d.width), 201.0);
    }

    #[test]
    fn a_drag_writes_only_what_it_changed() {
        let drag = |axis, had_height| {
            let mut d = ImageDrag {
                range: 0..0,
                tab_id: String::new(),
                source: String::new(),
                active: false,
                axis,
                start_x: px(0.0),
                start_y: px(0.0),
                start_width: px(200.0),
                start_height: px(100.0),
                column: px(700.0),
                width: px(200.0),
                height: px(100.0),
                had_height,
            };
            d.update(px(40.0), px(10.0));
            d
        };
        assert_eq!(drag(DragAxis::Corner, false).result().1, None);
        assert!(drag(DragAxis::Corner, true).result().1.is_some());
        assert_eq!(drag(DragAxis::Right, false).result(), (240, Some(100)));
        assert_eq!(drag(DragAxis::Bottom, false).result(), (200, Some(110)));
        assert!(drag(DragAxis::Right, false).moved());
    }

    #[test]
    fn a_width_and_height_stretch_the_picture_and_shrink_together_in_a_narrow_column() {
        let (w, h) = fit_image_size((400, 400), 200, Some(50), px(700.0));
        assert_eq!((f32::from(w), f32::from(h)), (200.0, 50.0));
        let (w, h) = fit_image_size((400, 400), 200, Some(50), px(100.0));
        assert_eq!((f32::from(w), f32::from(h)), (100.0, 25.0));
        let (w, h) = fit_image_size((400, 200), 100, None, px(700.0));
        assert_eq!((f32::from(w), f32::from(h)), (100.0, 50.0));
        let (w, h) = thumbnail_size(Some((400, 200)), Some(100), Some(80), px(700.0));
        assert_eq!((f32::from(w), f32::from(h)), (100.0, 80.0));
        // A zero wanted width does not divide by zero.
        let (w, h) = thumbnail_size(Some((400, 200)), Some(0), Some(80), px(700.0));
        assert!(f32::from(w).is_finite() && f32::from(h).is_finite());
    }

    #[test]
    fn extreme_aspect_ratios_never_drag_past_the_column_or_four_digits() {
        // Wide thin banner: corner drag stays within the column and 4-digit cap.
        let (w, h) = drag_size(
            DragAxis::Corner,
            (px(700.0), px(14.0)),
            (px(10.0), px(10.0)),
            px(700.0),
        );
        assert!(f32::from(w) <= 700.0 && f32::from(h) <= MAX_WIDTH);
        let (w, h) = drag_size(
            DragAxis::Corner,
            (px(700.0), px(1.0)),
            (px(1.0), px(0.0)),
            px(700.0),
        );
        assert!(f32::from(w) <= 700.0 && f32::from(h) <= MAX_WIDTH);
        // Very tall image: right-edge drag clamps height to 4 digits too.
        let (w, h) = drag_size(
            DragAxis::Right,
            (px(200.0), px(20000.0)),
            (px(50.0), px(0.0)),
            px(700.0),
        );
        assert_eq!((f32::from(w), f32::from(h)), (250.0, MAX_WIDTH));
    }

    #[test]
    fn the_edge_areas_sit_on_the_edges_clear_of_the_corner() {
        let p = Bounds::new(point(px(10.0), px(20.0)), size(px(100.0), px(80.0)));
        assert_eq!(
            handle_at(p, &point(px(109.0), px(99.0))),
            Some(DragAxis::Corner)
        );
        assert_eq!(
            handle_at(p, &point(px(109.0), px(40.0))),
            Some(DragAxis::Right)
        );
        assert_eq!(
            handle_at(p, &point(px(40.0), px(99.0))),
            Some(DragAxis::Bottom)
        );
        assert_eq!(handle_at(p, &point(px(50.0), px(50.0))), None);
    }

    #[test]
    fn the_grab_area_is_the_bottom_right_corner_and_never_bigger_than_the_picture() {
        let area = handle_grab_area(Bounds::new(
            point(px(10.0), px(20.0)),
            size(px(100.0), px(50.0)),
        ));
        assert_eq!(f32::from(area.right()), 110.0);
        assert_eq!(f32::from(area.bottom()), 70.0);
        assert_eq!(f32::from(area.size.width), 18.0);
        let tiny = handle_grab_area(Bounds::new(
            point(px(0.0), px(0.0)),
            size(px(12.0), px(40.0)),
        ));
        assert_eq!(f32::from(tiny.size.width), 12.0);
    }

    #[test]
    fn thumbnails_are_a_fixed_height_unless_they_have_a_width() {
        let (w, h) = thumbnail_size(Some((400, 200)), None, None, px(700.0));
        assert_eq!((f32::from(w), f32::from(h)), (320.0, 160.0));
        let (w, h) = thumbnail_size(Some((400, 200)), Some(100), None, px(700.0));
        assert_eq!((f32::from(w), f32::from(h)), (100.0, 50.0));
        // Never past the column.
        let (w, h) = thumbnail_size(Some((4000, 200)), None, None, px(700.0));
        assert_eq!((f32::from(w), f32::from(h)), (700.0, 35.0));
        // A small picture is not enlarged.
        let (w, h) = thumbnail_size(Some((40, 30)), None, None, px(700.0));
        assert_eq!((f32::from(w), f32::from(h)), (40.0, 30.0));
        // Unknown size: a square.
        let (w, h) = thumbnail_size(None, None, None, px(700.0));
        assert_eq!((f32::from(w), f32::from(h)), (160.0, 160.0));
    }

    #[test]
    fn thumbnails_wrap_to_a_new_line_and_lines_take_the_tallest() {
        let sizes = [
            (px(300.0), px(100.0)),
            (px(300.0), px(160.0)),
            (px(300.0), px(50.0)),
        ];
        let (spots, total) = flow_thumbnails(&sizes, px(700.0), px(8.0));
        let spots: Vec<_> = spots
            .iter()
            .map(|&(x, y)| (f32::from(x), f32::from(y)))
            .collect();
        assert_eq!(spots, vec![(0.0, 0.0), (308.0, 0.0), (0.0, 168.0)]);
        assert_eq!(f32::from(total), 218.0);
        // One wider than the column still gets a line to itself.
        let (spots, total) = flow_thumbnails(&[(px(900.0), px(40.0))], px(700.0), px(8.0));
        assert_eq!((f32::from(spots[0].0), f32::from(total)), (0.0, 40.0));
    }

    #[test]
    fn an_explicit_width_enlarges_but_stays_in_the_column() {
        let (w, h) = fit_image_width((100, 50), 300, px(700.0));
        assert_eq!((f32::from(w), f32::from(h)), (300.0, 150.0));
        // The height limit does not apply to a tall picture.
        let (w, h) = fit_image_width((100, 800), 400, px(700.0));
        assert_eq!((f32::from(w), f32::from(h)), (400.0, 3200.0));
        let (w, _) = fit_image_width((100, 50), 900, px(700.0));
        assert_eq!(f32::from(w), 700.0);
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
    fn a_replaced_picture_is_reported_as_changed_once() {
        let dir = std::env::temp_dir().join(format!("litemark-img-change-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let svg = dir.join("a.svg");
        std::fs::write(&svg, r#"<svg width="10" height="20">"#).unwrap();
        let mut cache = ImageSizeCache::default();
        assert_eq!(
            cache.size_checked(&svg, true),
            (Some((10, 20)), false),
            "first read"
        );
        assert_eq!(
            cache.size_checked(&svg, true),
            (Some((10, 20)), false),
            "unchanged"
        );
        // Replaced with a different size (and so a different length).
        std::fs::write(&svg, r#"<svg width="300" height="40">"#).unwrap();
        assert_eq!(cache.size_checked(&svg, true), (Some((300, 40)), true));
        assert_eq!(cache.size_checked(&svg, true), (Some((300, 40)), false));
        let _ = std::fs::remove_dir_all(&dir);
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
