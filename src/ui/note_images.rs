//! Sizes for images shown in notes: reading a picture's dimensions from its
//! file header, remembering them between frames, and fitting them to the
//! editor column. The picture itself is drawn by gpui's `img()`.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use gpui::{
    point, px, size, App, AppContext, Asset, AssetLogger, Bounds, Context, ElementId, Entity,
    ImageAssetLoader, ImageCacheError, Pixels, RenderImage, Resource, SharedString, Window,
};

/// Tallest an image block grows, before zoom.
pub const MAX_IMAGE_HEIGHT: Pixels = px(480.0);
/// Space above and below a picture, so neighbouring lines keep their distance.
pub const IMAGE_MARGIN_Y: Pixels = px(6.0);
/// Corner radius of the picture and of its placeholder.
pub const IMAGE_RADIUS: Pixels = px(6.0);
/// Minimum interval between disk `stat()` re-checks for a cached visible image.
pub const IMAGE_RECHECK_TTL: Duration = Duration::from_secs(4);

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
    /// Incremented each time this path is loaded or reloaded after changing on disk.
    generation: u64,
    checked_at: Instant,
    checked_epoch: u64,
}

/// Decoded sizes by path, so a frame never reads a file it has already seen.
/// A visible image is re-checked against the file's modified time and length
/// (one `stat`), throttled by [`IMAGE_RECHECK_TTL`] unless [`Self::mark_recheck_due`]
/// was called (e.g. window activation or tab reload).
#[derive(Clone, Default)]
pub struct ImageSizeCache {
    entries: HashMap<PathBuf, Entry>,
    next_generation: u64,
    recheck_epoch: u64,
}

/// Element ID for a rendered image slot. Hashing the slot, resolved file path,
/// and reload generation ensures GPUI never reuses a multi-frame animation's
/// `ImgState { frame_index }` across a different or replaced image.
pub fn image_element_id(
    kind: &'static str,
    line_idx: usize,
    slot: usize,
    path: &Path,
    generation: u64,
) -> ElementId {
    let mut hasher = DefaultHasher::new();
    line_idx.hash(&mut hasher);
    slot.hash(&mut hasher);
    path.hash(&mut hasher);
    generation.hash(&mut hasher);
    ElementId::NamedInteger(SharedString::new_static(kind), hasher.finish())
}

impl ImageSizeCache {
    /// Cached size without any filesystem access, even on a cache miss.
    #[cfg(test)]
    pub fn cached_size(&self, path: &Path) -> Option<(u32, u32)> {
        self.entries.get(path).and_then(|e| e.size)
    }

    /// Force the next visible check of any cached image to re-stat on disk
    /// (e.g. when the window regains focus or a tab is reloaded).
    pub fn mark_recheck_due(&mut self) {
        self.recheck_epoch = self.recheck_epoch.wrapping_add(1);
    }

    /// Throttled [`Self::size_checked`]: skips disk `stat()` calls for cached
    /// entries checked within [`IMAGE_RECHECK_TTL`] in the current epoch.
    pub fn size_checked_throttled(
        &mut self,
        path: &Path,
        now: Instant,
    ) -> (Option<(u32, u32)>, bool) {
        if let Some(entry) = self.entries.get(path) {
            if entry.checked_epoch == self.recheck_epoch
                && now.saturating_duration_since(entry.checked_at) < IMAGE_RECHECK_TTL
            {
                return (entry.size, false);
            }
        }
        self.size_checked_at(path, true, now)
    }

    /// The size of the image at `path`. With `recheck`, a changed or newly
    /// created file is read again; without it a cached answer is trusted, so
    /// images far off screen cost no file access after the first read.
    pub fn size(&mut self, path: &Path, recheck: bool) -> Option<(u32, u32)> {
        self.size_checked(path, recheck).0
    }

    /// [`Self::size`], and whether the file changed since it was last read,
    /// so the caller can drop the stale picture from gpui's image cache.
    pub fn size_checked(&mut self, path: &Path, recheck: bool) -> (Option<(u32, u32)>, bool) {
        self.size_checked_at(path, recheck, Instant::now())
    }

    fn size_checked_at(
        &mut self,
        path: &Path,
        recheck: bool,
        now: Instant,
    ) -> (Option<(u32, u32)>, bool) {
        let epoch = self.recheck_epoch;
        let stamp = match self.entries.get_mut(path) {
            Some(entry) if !recheck => return (entry.size, false),
            Some(entry) => {
                let stamp = stamp_of(path);
                if entry.stamp == stamp {
                    entry.checked_at = now;
                    entry.checked_epoch = epoch;
                    return (entry.size, false);
                }
                stamp
            }
            None => stamp_of(path),
        };
        let changed = self.entries.contains_key(path);
        let size = stamp.and_then(|_| read_image_size(path));
        self.next_generation = self.next_generation.wrapping_add(1);
        let generation = self.next_generation;
        self.entries.insert(
            path.to_path_buf(),
            Entry {
                stamp,
                size,
                generation,
                checked_at: now,
                checked_epoch: epoch,
            },
        );
        (size, changed)
    }

    /// Current reload generation for `path`, or `0` if not cached yet.
    pub fn generation(&self, path: &Path) -> u64 {
        self.entries.get(path).map_or(0, |e| e.generation)
    }

    /// Drops entries for paths that no longer appear in the note.
    pub fn retain_paths(&mut self, live: &std::collections::HashSet<PathBuf>) {
        if self.entries.len() > live.len() + 64 {
            self.entries.retain(|path, _| live.contains(path));
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Default decoded-pixel budget for [`NoteImageCache`] (64 MiB of BGRA frames).
pub const DEFAULT_IMAGE_CACHE_BUDGET_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct CacheKey {
    path: PathBuf,
    /// Longest decoded pixel dimension bucket (`0` for animated/SVG/unscaled).
    max_dim: u32,
}

enum CacheEntryState {
    Loading { load_gen: u64 },
    Loaded(Result<Arc<RenderImage>, ImageCacheError>),
}

struct CacheEntry {
    state: CacheEntryState,
    bytes: usize,
    last_used: u64,
    /// Editor frame ([`NoteImageCache::record_tab_paths`]) this entry was last
    /// drawn in. Entries drawn in the current or previous frame are on screen
    /// and never evicted for budget.
    last_frame: u64,
}

/// Bounded LRU image cache that downscales large static rasters on a background
/// thread to about 2x display size (coarse buckets, capped at 2048 px) and
/// explicitly calls `cx.drop_image` on eviction, disk replacement, tab close and
/// cache release so GPU atlas memory stays bounded.
///
/// The budget is soft for pictures on screen: they are never evicted, so a
/// note whose visible pictures exceed it doesn't reload them in a loop.
pub struct NoteImageCache {
    entries: HashMap<CacheKey, CacheEntry>,
    total_bytes: usize,
    budget_bytes: usize,
    tick: u64,
    next_load_gen: u64,
    /// Bumped once per editor frame by [`Self::record_tab_paths`].
    frame: u64,
    /// Local image paths each open tab's note references, as last painted.
    /// A cached picture stays while any open tab references it.
    tab_paths: HashMap<String, HashSet<PathBuf>>,
}

/// Coarse pixel-dimension bucket (`<= 2x` display size at the window's scale
/// factor) so resizing doesn't re-decode on every pixel while thumbnails stay
/// small in RAM and VRAM.
pub fn bucket_max_dim(width: Pixels, height: Pixels, scale_factor: f32) -> u32 {
    let longest = f32::from(width).max(f32::from(height)).max(1.0) * scale_factor.max(1.0) * 2.0;
    if longest <= 256.0 {
        256
    } else if longest <= 512.0 {
        512
    } else if longest <= 1024.0 {
        1024
    } else {
        2048
    }
}

/// True for formats that are always single-frame static rasters and safe to
/// decode + downscale with `image`. Animated GIF/WebP and SVG stay on GPUI's
/// native `ImageAssetLoader`.
fn is_static_raster_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "bmp"
            )
        })
}

/// Decode a static raster image from `path`, downscaling it to fit inside
/// `max_dim x max_dim` when larger, and convert RGBA pixels to GPUI's BGRA
/// layout.
pub fn decode_downscaled_raster(
    path: &Path,
    max_dim: u32,
) -> Result<Arc<RenderImage>, ImageCacheError> {
    let reader = image::ImageReader::open(path).map_err(|e| ImageCacheError::Io(Arc::new(e)))?;
    let reader = reader
        .with_guessed_format()
        .map_err(|e| ImageCacheError::Io(Arc::new(e)))?;
    let mut img = reader.decode().map_err(|e| {
        ImageCacheError::Io(Arc::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e.to_string(),
        )))
    })?;
    if max_dim > 0 && (img.width() > max_dim || img.height() > max_dim) {
        img = img.resize(max_dim, max_dim, image::imageops::FilterType::Triangle);
    }
    let mut rgba = img.to_rgba8();
    // GPUI RenderImage frames use BGRA pixel ordering.
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(rgba)])))
}

fn render_image_bytes(img: &RenderImage) -> usize {
    (0..img.frame_count())
        .map(|i| {
            let sz = img.size(i);
            (sz.width.0.max(0) as usize)
                .saturating_mul(sz.height.0.max(0) as usize)
                .saturating_mul(4)
        })
        .sum()
}

impl NoteImageCache {
    pub fn new(cx: &mut App) -> Entity<Self> {
        Self::with_budget(DEFAULT_IMAGE_CACHE_BUDGET_BYTES, cx)
    }

    pub fn with_budget(budget_bytes: usize, cx: &mut App) -> Entity<Self> {
        let entity = cx.new(|_cx| Self {
            entries: HashMap::new(),
            total_bytes: 0,
            budget_bytes: budget_bytes.max(1),
            tick: 0,
            next_load_gen: 0,
            frame: 0,
            tab_paths: HashMap::new(),
        });
        cx.observe_release(&entity, |cache, cx| {
            cache.clear(None, cx);
        })
        .detach();
        entity
    }

    fn bump_tick(&mut self) -> u64 {
        self.tick = self.tick.wrapping_add(1);
        self.tick
    }

    /// Drawn in the current or the previous editor frame.
    fn on_screen(&self, entry: &CacheEntry) -> bool {
        entry.last_frame.saturating_add(1) >= self.frame
    }

    /// Load `path` downscaled to at most `max_dim` on the longest edge (for
    /// static rasters). While that size decodes in the background, another
    /// already-decoded size of the same picture is shown instead (resize,
    /// zoom, monitor change), so the picture never blanks; `None` only when no
    /// size of it is ready yet.
    pub fn load_sized(
        &mut self,
        path: &Path,
        max_dim: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let static_raster = is_static_raster_path(path);
        let key = CacheKey {
            path: path.to_path_buf(),
            max_dim: if static_raster { max_dim } else { 0 },
        };
        let now = self.bump_tick();
        let frame = self.frame;
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used = now;
            entry.last_frame = frame;
            if let CacheEntryState::Loaded(res) = &entry.state {
                return Some(res.clone());
            }
            return self.fallback_size(&key, now);
        }

        self.next_load_gen = self.next_load_gen.wrapping_add(1);
        let load_gen = self.next_load_gen;
        self.entries.insert(
            key.clone(),
            CacheEntry {
                state: CacheEntryState::Loading { load_gen },
                bytes: 0,
                last_used: now,
                last_frame: frame,
            },
        );

        let path_owned = path.to_path_buf();
        let bg_task = if static_raster {
            cx.background_executor()
                .spawn(async move { decode_downscaled_raster(&path_owned, max_dim) })
        } else {
            let fut =
                AssetLogger::<ImageAssetLoader>::load(Resource::Path(Arc::from(path_owned)), cx);
            cx.background_executor().spawn(fut)
        };

        let fallback = self.fallback_size(&key, now);
        let cache_weak = cx.entity().downgrade();
        let view = window.current_view();
        window
            .spawn(cx, async move |cx| {
                let result = bg_task.await;
                let _ = cx.update(|window, cx| {
                    let _ = cache_weak.update(cx, |cache, cx| {
                        cache.finish_load(&key, load_gen, result, Some(window), cx);
                    });
                    cx.notify(view);
                });
            })
            .detach();

        fallback
    }

    /// The decoded size of `key.path` nearest to `key.max_dim`, marked as
    /// drawn this frame, to stand in while `key` itself loads.
    fn fallback_size(
        &mut self,
        key: &CacheKey,
        now: u64,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let frame = self.frame;
        let (_, entry) = self
            .entries
            .iter_mut()
            .filter(|(k, e)| {
                k.path == key.path
                    && k.max_dim != key.max_dim
                    && matches!(e.state, CacheEntryState::Loaded(Ok(_)))
            })
            .min_by_key(|(k, _)| k.max_dim.abs_diff(key.max_dim))?;
        entry.last_used = now;
        entry.last_frame = frame;
        match &entry.state {
            CacheEntryState::Loaded(res) => Some(res.clone()),
            CacheEntryState::Loading { .. } => None,
        }
    }

    fn finish_load(
        &mut self,
        key: &CacheKey,
        load_gen: u64,
        result: Result<Arc<RenderImage>, ImageCacheError>,
        window: Option<&mut Window>,
        cx: &mut App,
    ) {
        let now = self.bump_tick();
        let Some(entry) = self.entries.get_mut(key) else {
            // Entry was invalidated or cleared while decoding: drop immediately.
            if let Ok(img) = result {
                cx.drop_image(img, window);
            }
            return;
        };
        match entry.state {
            CacheEntryState::Loading { load_gen: cur } if cur == load_gen => {}
            _ => {
                if let Ok(img) = result {
                    cx.drop_image(img, window);
                }
                return;
            }
        }
        let bytes = result
            .as_ref()
            .map(|img| render_image_bytes(img))
            .unwrap_or(0);
        entry.state = CacheEntryState::Loaded(result);
        entry.bytes = bytes;
        entry.last_used = now;
        self.total_bytes = self.total_bytes.saturating_add(bytes);
        self.evict_to_budget(window, cx);
    }

    /// Evict least-recently-used pictures that aren't on screen until the
    /// budget fits (or only on-screen pictures are left).
    fn evict_to_budget(&mut self, mut window: Option<&mut Window>, cx: &mut App) {
        while self.total_bytes > self.budget_bytes {
            let victim = self
                .entries
                .iter()
                .filter(|(_, e)| {
                    !self.on_screen(e) && matches!(e.state, CacheEntryState::Loaded(_))
                })
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| k.clone());
            let Some(victim_key) = victim else {
                break;
            };
            self.remove_entry(&victim_key, window.as_deref_mut(), cx);
        }
    }

    fn remove_entry(&mut self, key: &CacheKey, window: Option<&mut Window>, cx: &mut App) {
        if let Some(removed) = self.entries.remove(key) {
            self.total_bytes = self.total_bytes.saturating_sub(removed.bytes);
            if let CacheEntryState::Loaded(Ok(img)) = removed.state {
                cx.drop_image(img, window);
            }
        }
    }

    fn remove_where(
        &mut self,
        mut pred: impl FnMut(&CacheKey, &CacheEntry) -> bool,
        mut window: Option<&mut Window>,
        cx: &mut App,
    ) {
        let keys: Vec<CacheKey> = self
            .entries
            .iter()
            .filter(|(k, e)| pred(k, e))
            .map(|(k, _)| k.clone())
            .collect();
        for key in keys {
            self.remove_entry(&key, window.as_deref_mut(), cx);
        }
    }

    /// Drop every cached size bucket for `path` (e.g. after disk replacement)
    /// and release its GPU textures via `cx.drop_image`.
    pub fn invalidate_path(&mut self, path: &Path, window: Option<&mut Window>, cx: &mut App) {
        self.remove_where(|k, _| k.path == path, window, cx);
    }

    fn live_paths(&self) -> HashSet<PathBuf> {
        self.tab_paths.values().flatten().cloned().collect()
    }

    /// Called once per editor frame with the local image paths the painted
    /// tab's note references. Starts a new frame, releases sizes of a picture
    /// that a newer size replaced on screen, and, once usage passes half the
    /// budget, drops pictures no open tab references any more.
    pub fn record_tab_paths(
        &mut self,
        tab_id: &str,
        paths: &HashSet<PathBuf>,
        mut window: Option<&mut Window>,
        cx: &mut App,
    ) {
        if self.tab_paths.get(tab_id) != Some(paths) {
            self.tab_paths.insert(tab_id.to_string(), paths.clone());
        }
        self.frame = self.frame.wrapping_add(1);

        // A size not drawn for two frames while another size of the same
        // picture is on screen was superseded (resize / zoom): release it.
        let shown: HashSet<PathBuf> = self
            .entries
            .iter()
            .filter(|(_, e)| self.on_screen(e))
            .map(|(k, _)| k.path.clone())
            .collect();
        let frame = self.frame;
        self.remove_where(
            |k, e| {
                e.last_frame.saturating_add(1) < frame
                    && matches!(e.state, CacheEntryState::Loaded(_))
                    && shown.contains(&k.path)
            },
            window.as_deref_mut(),
            cx,
        );

        if self.total_bytes > self.budget_bytes / 2 {
            let live = self.live_paths();
            self.remove_where(|k, _| !live.contains(&k.path), window, cx);
        }
    }

    /// Forget closed tabs and release every cached picture no remaining open
    /// tab references. Pictures of the tabs still open are kept.
    pub fn forget_tabs(&mut self, tab_ids: &[String], window: Option<&mut Window>, cx: &mut App) {
        for id in tab_ids {
            self.tab_paths.remove(id);
        }
        let live = self.live_paths();
        self.remove_where(|k, _| !live.contains(&k.path), window, cx);
    }

    /// Drop every cached image and release its GPU textures.
    pub fn clear(&mut self, mut window: Option<&mut Window>, cx: &mut App) {
        for (_, entry) in self.entries.drain() {
            if let CacheEntryState::Loaded(Ok(img)) = entry.state {
                cx.drop_image(img, window.as_deref_mut());
            }
        }
        self.total_bytes = 0;
    }

    #[cfg(test)]
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    #[cfg(test)]
    pub fn entry_count(&self) -> usize {
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

    #[test]
    fn image_element_id_changes_with_path_slot_and_reload_generation() {
        let dir = std::env::temp_dir().join(format!("litemark-img-gen-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let gif = dir.join("anim.svg");
        let png = dir.join("static.svg");
        std::fs::write(&gif, r#"<svg width="20" height="20">"#).unwrap();
        std::fs::write(&png, r#"<svg width="20" height="20">"#).unwrap();

        let mut cache = ImageSizeCache::default();
        let _ = cache.size_checked(&gif, true);
        let gen1 = cache.generation(&gif);
        assert!(gen1 > 0);
        let id_gif = image_element_id("note-image", 0, 0, &gif, gen1);
        let id_png = image_element_id("note-image", 0, 0, &png, gen1);
        let id_slot1 = image_element_id("note-thumb", 0, 1, &gif, gen1);
        let id_line1 = image_element_id("note-image", 1, 0, &gif, gen1);
        assert_ne!(id_gif, id_png, "different path must get a new ElementId");
        assert_ne!(id_gif, id_slot1, "different slot must get a new ElementId");
        assert_ne!(id_gif, id_line1, "different line must get a new ElementId");

        // Replacing the file on disk increments generation and rotates ElementId.
        std::fs::write(&gif, r#"<svg width="100" height="50">"#).unwrap();
        let (_, changed) = cache.size_checked(&gif, true);
        assert!(changed);
        let gen2 = cache.generation(&gif);
        assert_ne!(gen1, gen2);
        let id_gif_reloaded = image_element_id("note-image", 0, 0, &gif, gen2);
        assert_ne!(
            id_gif, id_gif_reloaded,
            "disk replacement must rotate ElementId"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cached_size_misses_without_io_and_throttled_recheck_respects_ttl_and_epoch() {
        let dir = std::env::temp_dir().join(format!("litemark-img-ttl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let svg = dir.join("a.svg");
        std::fs::write(&svg, r#"<svg width="10" height="20">"#).unwrap();

        let mut cache = ImageSizeCache::default();
        assert_eq!(cache.cached_size(&svg), None);
        assert_eq!(cache.len(), 0);

        let t0 = Instant::now();
        assert_eq!(
            cache.size_checked_throttled(&svg, t0),
            (Some((10, 20)), false)
        );
        assert_eq!(cache.cached_size(&svg), Some((10, 20)));

        // Overwrite on disk: within TTL, throttled check returns cached size without statting.
        std::fs::write(&svg, r#"<svg width="300" height="40">"#).unwrap();
        assert_eq!(
            cache.size_checked_throttled(&svg, t0 + Duration::from_secs(1)),
            (Some((10, 20)), false)
        );

        // mark_recheck_due forces immediate re-stat even within TTL.
        cache.mark_recheck_due();
        let t1 = t0 + Duration::from_secs(1);
        assert_eq!(
            cache.size_checked_throttled(&svg, t1),
            (Some((300, 40)), true)
        );

        // Past TTL, throttled check also notices disk changes. A different
        // length, so the change shows even when the write lands in the same
        // file-time tick as the one above (coarse mtimes on CI).
        std::fs::write(&svg, r#"<svg width="4000" height="50">"#).unwrap();
        assert_eq!(
            cache.size_checked_throttled(&svg, t1 + IMAGE_RECHECK_TTL),
            (Some((4000, 50)), true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bucket_max_dim_rounds_into_coarse_tiers() {
        assert_eq!(bucket_max_dim(px(100.0), px(80.0), 1.0), 256);
        assert_eq!(bucket_max_dim(px(160.0), px(120.0), 1.0), 512);
        assert_eq!(bucket_max_dim(px(160.0), px(120.0), 2.0), 1024);
        assert_eq!(bucket_max_dim(px(900.0), px(480.0), 2.0), 2048);
    }

    #[test]
    fn decode_downscaled_raster_shrinks_oversized_png_and_converts_bgra() {
        let dir =
            std::env::temp_dir().join(format!("litemark-img-downscale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("big.png");
        let img = image::RgbaImage::from_pixel(400, 200, image::Rgba([255, 10, 20, 255]));
        img.save(&png).unwrap();

        let rendered = decode_downscaled_raster(&png, 100).expect("decode png");
        let sz = rendered.size(0);
        assert_eq!(sz.width.0, 100);
        assert_eq!(sz.height.0, 50);
        let bytes = rendered.as_bytes(0).expect("frame bytes");
        // RGBA [255, 10, 20, 255] must be stored as BGRA [20, 10, 255, 255].
        assert_eq!(&bytes[0..4], &[20, 10, 255, 255]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn note_image_cache_evicts_lru_entries_when_budget_exceeded_and_invalidates_path() {
        let dir = std::env::temp_dir().join(format!("litemark-img-lru-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p1 = dir.join("one.png");
        let p2 = dir.join("two.png");
        let p3 = dir.join("three.png");
        for p in [&p1, &p2, &p3] {
            let img = image::RgbaImage::from_pixel(32, 32, image::Rgba([1, 2, 3, 255]));
            img.save(p).unwrap();
        }
        // Each 32x32 BGRA image is 4096 bytes; budget 9000 bytes holds at most 2.
        let cx = gpui::TestAppContext::single();
        let cache = cx.update(|cx| NoteImageCache::with_budget(9000, cx));
        let (k1, k2, k3) = (
            CacheKey {
                path: p1.clone(),
                max_dim: 256,
            },
            CacheKey {
                path: p2.clone(),
                max_dim: 256,
            },
            CacheKey {
                path: p3.clone(),
                max_dim: 256,
            },
        );
        let r1 = decode_downscaled_raster(&p1, 256);
        let r2 = decode_downscaled_raster(&p2, 256);
        let r3 = decode_downscaled_raster(&p3, 256);
        cx.update(|cx| {
            cache.update(cx, |c, cx| {
                // Nothing here was drawn recently, so all of it is evictable.
                c.frame = 10;
                for (k, r) in [(&k1, r1), (&k2, r2)] {
                    c.next_load_gen += 1;
                    let g = c.next_load_gen;
                    let t = c.bump_tick();
                    c.entries.insert(
                        k.clone(),
                        CacheEntry {
                            state: CacheEntryState::Loading { load_gen: g },
                            bytes: 0,
                            last_used: t,
                            last_frame: 0,
                        },
                    );
                    c.finish_load(k, g, r, None, cx);
                }
                assert_eq!(c.entry_count(), 2);
                assert_eq!(c.total_bytes(), 8192);

                // Touch k1 so k2 becomes the LRU victim.
                let t = c.bump_tick();
                c.entries.get_mut(&k1).unwrap().last_used = t;

                c.next_load_gen += 1;
                let g = c.next_load_gen;
                let t = c.bump_tick();
                c.entries.insert(
                    k3.clone(),
                    CacheEntry {
                        state: CacheEntryState::Loading { load_gen: g },
                        bytes: 0,
                        last_used: t,
                        last_frame: 0,
                    },
                );
                c.finish_load(&k3, g, r3, None, cx);

                assert_eq!(c.entry_count(), 2);
                assert_eq!(c.total_bytes(), 8192);
                assert!(c.entries.contains_key(&k1));
                assert!(!c.entries.contains_key(&k2), "LRU entry must be evicted");
                assert!(c.entries.contains_key(&k3));

                c.invalidate_path(&p1, None, cx);
                assert_eq!(c.entry_count(), 1);
                assert_eq!(c.total_bytes(), 4096);
            });
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Fills `cache` with one decoded 32x32 (4096-byte) entry per key, last
    /// drawn in `frame`.
    fn put_loaded(c: &mut NoteImageCache, key: &CacheKey, frame: u64, cx: &mut App) {
        let r = decode_downscaled_raster(&key.path, 256);
        c.next_load_gen += 1;
        let g = c.next_load_gen;
        let t = c.bump_tick();
        c.entries.insert(
            key.clone(),
            CacheEntry {
                state: CacheEntryState::Loading { load_gen: g },
                bytes: 0,
                last_used: t,
                last_frame: frame,
            },
        );
        c.finish_load(key, g, r, None, cx);
    }

    fn png_fixture(tag: &str, names: &[&str]) -> (PathBuf, Vec<PathBuf>) {
        let dir = std::env::temp_dir().join(format!("litemark-img-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let paths = names
            .iter()
            .map(|n| {
                let p = dir.join(n);
                image::RgbaImage::from_pixel(32, 32, image::Rgba([1, 2, 3, 255]))
                    .save(&p)
                    .unwrap();
                p
            })
            .collect();
        (dir, paths)
    }

    #[test]
    fn pictures_on_screen_are_never_evicted_even_over_budget() {
        let (dir, p) = png_fixture("onscreen", &["a.png", "b.png", "c.png"]);
        let cx = gpui::TestAppContext::single();
        // Room for two 4096-byte pictures, but three are on screen.
        let cache = cx.update(|cx| NoteImageCache::with_budget(9000, cx));
        cx.update(|cx| {
            cache.update(cx, |c, cx| {
                c.frame = 5;
                for path in &p {
                    let key = CacheKey {
                        path: path.clone(),
                        max_dim: 256,
                    };
                    put_loaded(c, &key, 5, cx);
                }
                assert_eq!(c.entry_count(), 3, "no on-screen picture is evicted");
                assert_eq!(c.total_bytes(), 3 * 4096);

                // Two frames later nothing was drawn: the budget applies again.
                c.frame = 7;
                c.evict_to_budget(None, cx);
                assert_eq!(c.entry_count(), 2);
            });
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_size_shows_the_old_size_until_it_loads_then_replaces_it() {
        let (dir, p) = png_fixture("resize", &["a.png"]);
        let cx = gpui::TestAppContext::single();
        let cache = cx.update(|cx| NoteImageCache::with_budget(1 << 20, cx));
        cx.update(|cx| {
            cache.update(cx, |c, cx| {
                let small = CacheKey {
                    path: p[0].clone(),
                    max_dim: 256,
                };
                let big = CacheKey {
                    path: p[0].clone(),
                    max_dim: 1024,
                };
                c.frame = 1;
                put_loaded(c, &small, 1, cx);
                // The bigger size is still decoding.
                c.entries.insert(
                    big.clone(),
                    CacheEntry {
                        state: CacheEntryState::Loading { load_gen: 99 },
                        bytes: 0,
                        last_used: 0,
                        last_frame: 1,
                    },
                );
                let t = c.bump_tick();
                assert!(
                    matches!(c.fallback_size(&big, t), Some(Ok(_))),
                    "the loaded small size stands in for the loading big one"
                );

                // The big size lands and is drawn; the small one stops being
                // drawn and is released two frames later.
                let r = decode_downscaled_raster(&big.path, 1024);
                c.finish_load(&big, 99, r, None, cx);
                let live: HashSet<PathBuf> = [p[0].clone()].into_iter().collect();
                for frame in 2..=4 {
                    c.entries.get_mut(&big).unwrap().last_frame = frame;
                    c.record_tab_paths("tab-1", &live, None, cx);
                }
                assert!(c.entries.contains_key(&big));
                assert!(!c.entries.contains_key(&small), "superseded size released");
            });
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_tabs_keep_their_pictures_and_closing_frees_only_unshared_ones() {
        let (dir, p) = png_fixture("tabs", &["a.png", "b.png", "shared.png"]);
        let cx = gpui::TestAppContext::single();
        // Tiny budget: the unreferenced sweep runs on every frame.
        let cache = cx.update(|cx| NoteImageCache::with_budget(100, cx));
        cx.update(|cx| {
            cache.update(cx, |c, cx| {
                let key = |i: usize| CacheKey {
                    path: p[i].clone(),
                    max_dim: 256,
                };
                let set = |is: &[usize]| -> HashSet<PathBuf> {
                    is.iter().map(|&i| p[i].clone()).collect()
                };
                // Each tab paints (recording its paths), then its pictures load.
                c.record_tab_paths("tab-a", &set(&[0, 2]), None, cx);
                put_loaded(c, &key(0), c.frame, cx);
                put_loaded(c, &key(2), c.frame, cx);
                c.record_tab_paths("tab-b", &set(&[1, 2]), None, cx);
                put_loaded(c, &key(1), c.frame, cx);
                // Switching back and forth keeps both tabs' pictures.
                for _ in 0..3 {
                    c.record_tab_paths("tab-a", &set(&[0, 2]), None, cx);
                    c.record_tab_paths("tab-b", &set(&[1, 2]), None, cx);
                }
                assert_eq!(c.entry_count(), 3);

                c.forget_tabs(&["tab-a".to_string()], None, cx);
                assert!(
                    !c.entries.contains_key(&key(0)),
                    "closed tab's own picture freed"
                );
                assert!(c.entries.contains_key(&key(1)));
                assert!(
                    c.entries.contains_key(&key(2)),
                    "picture shared with an open tab kept"
                );
            });
        });
        let _ = std::fs::remove_dir_all(&dir);
    }
}
