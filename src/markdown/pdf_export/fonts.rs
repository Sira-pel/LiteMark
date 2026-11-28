//! Bundled Ubuntu/Ubuntu Mono font loading, lazy system-font fallback, and
//! `rustybuzz` text shaping for PDF export.

use crate::{load_bundled_font_bytes, BundledFont};
use krilla::text::{Font as KrillaFont, GlyphId, KrillaGlyph};
use rustybuzz::{Direction, UnicodeBuffer};
use std::collections::HashMap;
use std::sync::Arc;

/// Logical font style requested by the PDF layout engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FontRole {
    Regular,
    Italic,
    Medium,
    Bold,
    BoldItalic,
    MonoRegular,
    MonoItalic,
    MonoBold,
    MonoBoldItalic,
}

impl FontRole {
    pub(crate) fn from_style(bold: bool, italic: bool, mono: bool) -> Self {
        match (mono, bold, italic) {
            (false, false, false) => Self::Regular,
            (false, false, true) => Self::Italic,
            (false, true, false) => Self::Bold,
            (false, true, true) => Self::BoldItalic,
            (true, false, false) => Self::MonoRegular,
            (true, false, true) => Self::MonoItalic,
            (true, true, false) => Self::MonoBold,
            (true, true, true) => Self::MonoBoldItalic,
        }
    }

    pub(crate) fn is_mono(self) -> bool {
        matches!(
            self,
            Self::MonoRegular | Self::MonoItalic | Self::MonoBold | Self::MonoBoldItalic
        )
    }

    pub(crate) fn is_bold(self) -> bool {
        matches!(
            self,
            Self::Bold | Self::BoldItalic | Self::MonoBold | Self::MonoBoldItalic
        )
    }

    pub(crate) fn is_italic(self) -> bool {
        matches!(
            self,
            Self::Italic | Self::BoldItalic | Self::MonoItalic | Self::MonoBoldItalic
        )
    }

    fn bundled(self) -> BundledFont {
        match self {
            Self::Regular => BundledFont::Regular,
            Self::Italic => BundledFont::Italic,
            Self::Medium => BundledFont::Medium,
            Self::Bold => BundledFont::Bold,
            Self::BoldItalic => BundledFont::BoldItalic,
            Self::MonoRegular => BundledFont::MonoRegular,
            Self::MonoItalic => BundledFont::MonoItalic,
            Self::MonoBold => BundledFont::MonoBold,
            Self::MonoBoldItalic => BundledFont::MonoBoldItalic,
        }
    }
}

/// One loaded font face shared between `rustybuzz` shaping and `krilla` output.
pub(crate) struct LoadedFace {
    pub(crate) krilla_font: KrillaFont,
    pub(crate) data: Arc<Vec<u8>>,
    pub(crate) index: u32,
    pub(crate) units_per_em: f32,
    pub(crate) ascent_ratio: f32,
    pub(crate) descent_ratio: f32,
}

impl LoadedFace {
    fn from_bytes(data: Arc<Vec<u8>>, index: u32) -> Option<Self> {
        let parsed = ttf_parser::Face::parse(&data, index).ok()?;
        let units_per_em = f32::from(parsed.units_per_em()).max(1.0);
        let ascent_ratio = (f32::from(parsed.ascender()) / units_per_em).clamp(0.5, 1.3);
        let descent_ratio = (f32::from(-parsed.descender()) / units_per_em).clamp(0.1, 0.6);
        let krilla_font = KrillaFont::new(data.clone().into(), index)?;
        Some(Self {
            krilla_font,
            data,
            index,
            units_per_em,
            ascent_ratio,
            descent_ratio,
        })
    }

    fn has_glyph(&self, ch: char) -> bool {
        let Ok(parsed) = ttf_parser::Face::parse(&self.data, self.index) else {
            return false;
        };
        parsed.glyph_index(ch).is_some_and(|gid| gid.0 != 0)
    }
}

/// One shaped run of text rendered with a single font face.
#[derive(Debug, Clone)]
pub(crate) struct ShapedFontRun {
    pub(crate) face_idx: usize,
    pub(crate) text: String,
    pub(crate) glyphs: Vec<KrillaGlyph>,
    pub(crate) width: f32,
}

/// Manages bundled fonts and lazy OS fallback fonts for a single PDF export.
pub(crate) struct FontBook {
    faces: Vec<LoadedFace>,
    bundled_map: HashMap<FontRole, usize>,
    system_db: Option<fontdb::Database>,
    /// System face IDs already loaded into `self.faces` (`None` = unloadable).
    system_loaded: HashMap<fontdb::ID, Option<usize>>,
    /// Cached face index for `(is_mono, is_bold, is_italic, ch)` fallback queries.
    char_fallback: HashMap<(bool, bool, bool, char), Option<usize>>,
    /// Ordered candidate system face IDs grouped by `(is_mono, is_bold, is_italic)` priority.
    system_candidates: HashMap<(bool, bool, bool), Vec<fontdb::ID>>,
}

impl FontBook {
    pub(crate) fn new() -> Result<Self, String> {
        let mut faces = Vec::with_capacity(9);
        let mut bundled_map = HashMap::with_capacity(9);
        let roles = [
            FontRole::Regular,
            FontRole::Italic,
            FontRole::Medium,
            FontRole::Bold,
            FontRole::BoldItalic,
            FontRole::MonoRegular,
            FontRole::MonoItalic,
            FontRole::MonoBold,
            FontRole::MonoBoldItalic,
        ];
        for role in roles {
            if let Some(bytes) = load_bundled_font_bytes(role.bundled()) {
                if let Some(face) = LoadedFace::from_bytes(Arc::new(bytes), 0) {
                    let idx = faces.len();
                    faces.push(face);
                    bundled_map.insert(role, idx);
                }
            }
        }
        if !bundled_map.contains_key(&FontRole::Regular) {
            return Err("Couldn't load bundled fonts for PDF export.".into());
        }
        Ok(Self {
            faces,
            bundled_map,
            system_db: None,
            system_loaded: HashMap::new(),
            char_fallback: HashMap::new(),
            system_candidates: HashMap::new(),
        })
    }

    pub(crate) fn face(&self, idx: usize) -> &LoadedFace {
        &self.faces[idx]
    }

    pub(crate) fn primary_face_idx(&self, role: FontRole) -> usize {
        self.bundled_map
            .get(&role)
            .copied()
            .or_else(|| self.bundled_map.get(&FontRole::Regular).copied())
            .unwrap_or(0)
    }

    pub(crate) fn ascent(&self, role: FontRole, font_size: f32) -> f32 {
        self.faces[self.primary_face_idx(role)].ascent_ratio * font_size
    }

    pub(crate) fn descent(&self, role: FontRole, font_size: f32) -> f32 {
        self.faces[self.primary_face_idx(role)].descent_ratio * font_size
    }

    /// Shape `text` with `role` at `font_size`, automatically splitting into
    /// fallback font runs and RTL/LTR direction runs when needed.
    pub(crate) fn shape_text(
        &mut self,
        text: &str,
        role: FontRole,
        font_size: f32,
    ) -> Vec<ShapedFontRun> {
        if text.is_empty() {
            return Vec::new();
        }
        let primary = self.primary_face_idx(role);

        // Fast path: pure ASCII is always covered by bundled Ubuntu / Ubuntu Mono
        // and is always LTR.
        if text.is_ascii() {
            let run = self.shape_single_run(primary, text, font_size, false);
            return vec![run];
        }

        // Partition into contiguous `(face_idx, is_rtl)` runs.
        let mut runs: Vec<(usize, bool, usize, usize)> = Vec::new();
        let mut prev_face = primary;
        let mut prev_rtl = false;

        for (byte_idx, ch) in text.char_indices() {
            let char_end = byte_idx + ch.len_utf8();
            let face_idx = if ch.is_control() || is_neutral_whitespace(ch) {
                // Keep spaces/controls in the primary face (or current run) so
                // words don't fragment unnecessarily.
                if self.faces[prev_face].has_glyph(ch) {
                    prev_face
                } else {
                    primary
                }
            } else if is_combining_or_joiner(ch) {
                // Keep combining marks / ZWJ / variation selectors with the
                // preceding base character's face when that face supports them.
                if self.faces[prev_face].has_glyph(ch) {
                    prev_face
                } else {
                    self.resolve_face_for_char(ch, role, primary)
                }
            } else {
                self.resolve_face_for_char(ch, role, primary)
            };

            let is_rtl = if is_strong_rtl(ch) {
                true
            } else if is_strong_ltr(ch) {
                false
            } else {
                prev_rtl
            };

            if let Some(last) = runs.last_mut() {
                if last.0 == face_idx && last.1 == is_rtl {
                    last.3 = char_end;
                    prev_face = face_idx;
                    prev_rtl = is_rtl;
                    continue;
                }
            }
            runs.push((face_idx, is_rtl, byte_idx, char_end));
            prev_face = face_idx;
            prev_rtl = is_rtl;
        }

        runs.into_iter()
            .map(|(face_idx, is_rtl, start, end)| {
                self.shape_single_run(face_idx, &text[start..end], font_size, is_rtl)
            })
            .collect()
    }

    pub(crate) fn measure_width(&mut self, text: &str, role: FontRole, font_size: f32) -> f32 {
        self.shape_text(text, role, font_size)
            .iter()
            .map(|r| r.width)
            .sum()
    }

    fn resolve_face_for_char(&mut self, ch: char, role: FontRole, primary: usize) -> usize {
        if self.faces[primary].has_glyph(ch) {
            return primary;
        }
        // Check bundled Regular as a cheap secondary fallback (e.g. if Bold/Mono
        // is missing a symbol that Ubuntu Regular has).
        let regular = self.primary_face_idx(FontRole::Regular);
        if regular != primary && self.faces[regular].has_glyph(ch) {
            return regular;
        }

        let key = (role.is_mono(), role.is_bold(), role.is_italic(), ch);
        if let Some(&cached) = self.char_fallback.get(&key) {
            return cached.unwrap_or(primary);
        }

        let found = self.find_system_fallback(ch, role);
        self.char_fallback.insert(key, found);
        found.unwrap_or(primary)
    }

    fn ensure_system_db(&mut self) {
        if self.system_db.is_none() {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            self.system_db = Some(db);
        }
    }

    fn find_system_fallback(&mut self, ch: char, role: FontRole) -> Option<usize> {
        self.ensure_system_db();
        let style_key = (role.is_mono(), role.is_bold(), role.is_italic());

        if !self.system_candidates.contains_key(&style_key) {
            let db = self.system_db.as_ref()?;
            let want_mono = role.is_mono();
            let want_bold = role.is_bold();
            let want_italic = role.is_italic();

            let mut scored: Vec<(i32, fontdb::ID)> = db
                .faces()
                .map(|info| {
                    let mut score = 0i32;
                    if info.monospaced == want_mono {
                        score += 10;
                    }
                    let face_bold = info.weight.0 >= 600;
                    if face_bold == want_bold {
                        score += 6;
                    }
                    let face_italic = info.style != fontdb::Style::Normal;
                    if face_italic == want_italic {
                        score += 4;
                    }
                    // Prefer standard proportional / readable system families
                    // (Segoe UI, Noto, Sarabun/Leelawadee/Tahoma, Yu Gothic, PingFang,
                    // DejaVu, Liberation, Arial, etc.) ahead of exotic display fonts.
                    let family = info.families.first().map(|(f, _)| f.as_str()).unwrap_or("");
                    if is_preferred_fallback_family(family) {
                        score += 8;
                    }
                    (score, info.id)
                })
                .collect();
            scored.sort_by_key(|a| std::cmp::Reverse(a.0));
            self.system_candidates
                .insert(style_key, scored.into_iter().map(|(_, id)| id).collect());
        }

        // First check system faces we've ALREADY loaded into `self.faces`!
        let candidates = self.system_candidates.get(&style_key)?.clone();
        for &id in &candidates {
            if let Some(&Some(face_idx)) = self.system_loaded.get(&id) {
                if self.faces[face_idx].has_glyph(ch) {
                    return Some(face_idx);
                }
            }
        }

        // Otherwise probe unloaded system faces until one covers `ch`.
        for &id in &candidates {
            if self.system_loaded.contains_key(&id) {
                continue;
            }
            let covers = self.system_db.as_ref()?.with_face_data(id, |data, index| {
                ttf_parser::Face::parse(data, index)
                    .ok()
                    .is_some_and(|f| f.glyph_index(ch).is_some_and(|g| g.0 != 0))
            })?;
            if !covers {
                continue;
            }
            // Load this face into `self.faces`.
            let loaded_bytes = self
                .system_db
                .as_ref()?
                .with_face_data(id, |data, index| (data.to_vec(), index));
            let Some((bytes, index)) = loaded_bytes else {
                self.system_loaded.insert(id, None);
                continue;
            };
            match LoadedFace::from_bytes(Arc::new(bytes), index) {
                Some(face) => {
                    let idx = self.faces.len();
                    self.faces.push(face);
                    self.system_loaded.insert(id, Some(idx));
                    return Some(idx);
                }
                None => {
                    self.system_loaded.insert(id, None);
                }
            }
        }
        None
    }

    fn shape_single_run(
        &self,
        face_idx: usize,
        text: &str,
        font_size: f32,
        is_rtl: bool,
    ) -> ShapedFontRun {
        let face = &self.faces[face_idx];
        let Some(rb_face) = rustybuzz::Face::from_slice(&face.data, face.index) else {
            return ShapedFontRun {
                face_idx,
                text: text.to_string(),
                glyphs: Vec::new(),
                width: 0.0,
            };
        };

        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        if is_rtl {
            buffer.set_direction(Direction::RightToLeft);
        } else {
            buffer.set_direction(Direction::LeftToRight);
        }
        let dir = buffer.direction();
        let output = rustybuzz::shape(&rb_face, &[], buffer);

        let positions = output.glyph_positions();
        let infos = output.glyph_infos();
        let upem = face.units_per_em;

        let mut glyphs = Vec::with_capacity(output.len());
        let mut total_advance = 0.0f32;

        for i in 0..output.len() {
            let pos = positions[i];
            let info = infos[i];
            let start = (info.cluster as usize).min(text.len());
            let end = if dir == Direction::LeftToRight {
                let mut e = i + 1;
                while e < infos.len() && infos[e].cluster == info.cluster {
                    e += 1;
                }
                infos
                    .get(e)
                    .map_or(text.len(), |next| (next.cluster as usize).min(text.len()))
            } else {
                let mut e = i;
                let mut next_cluster = None;
                while e > 0 {
                    e -= 1;
                    if infos[e].cluster != info.cluster {
                        next_cluster = Some((infos[e].cluster as usize).min(text.len()));
                        break;
                    }
                }
                next_cluster.unwrap_or(text.len())
            };
            let end = end.clamp(start, text.len());

            let x_adv = pos.x_advance as f32 / upem;
            let x_off = pos.x_offset as f32 / upem;
            let y_off = pos.y_offset as f32 / upem;
            let y_adv = pos.y_advance as f32 / upem;
            total_advance += x_adv * font_size;

            glyphs.push(KrillaGlyph::new(
                GlyphId::new(info.glyph_id),
                x_adv,
                x_off,
                y_off,
                y_adv,
                start..end,
                None,
            ));
        }

        ShapedFontRun {
            face_idx,
            text: text.to_string(),
            glyphs,
            width: total_advance,
        }
    }
}

fn is_neutral_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\u{00A0}')
}

fn is_combining_or_joiner(ch: char) -> bool {
    let cp = ch as u32;
    matches!(
        cp,
        0x0300..=0x036F   // Combining Diacritical Marks
        | 0x0591..=0x05BD // Hebrew marks
        | 0x064B..=0x065F // Arabic harakat
        | 0x0E31          // Thai Mai Han-Akat
        | 0x0E34..=0x0E3A // Thai combining vowels
        | 0x0E47..=0x0E4E // Thai tone/diacritic marks
        | 0x1AB0..=0x1AFF
        | 0x1DC0..=0x1DFF
        | 0x200C..=0x200D // ZWNJ / ZWJ
        | 0x20D0..=0x20FF
        | 0xFE00..=0xFE0F // Variation Selectors
        | 0xFE20..=0xFE2F
    )
}

fn is_strong_rtl(ch: char) -> bool {
    let cp = ch as u32;
    matches!(
        cp,
        0x0590..=0x05FF   // Hebrew
        | 0x0600..=0x06FF // Arabic
        | 0x0750..=0x077F // Arabic Supplement
        | 0x08A0..=0x08FF // Arabic Extended-A
        | 0xFB50..=0xFDFF // Arabic Presentation Forms-A
        | 0xFE70..=0xFEFF // Arabic Presentation Forms-B
    )
}

fn is_strong_ltr(ch: char) -> bool {
    ch.is_ascii_alphabetic()
        || matches!(
            ch as u32,
            0x00C0..=0x024F   // Latin Extended
            | 0x0370..=0x052F // Greek & Cyrillic
            | 0x0E00..=0x0E7F // Thai
            | 0x1E00..=0x1EFF // Vietnamese Latin Extended Additional
            | 0x3040..=0x30FF // Hiragana / Katakana
            | 0x4E00..=0x9FFF // CJK Unified Ideographs
            | 0xAC00..=0xD7AF // Hangul
        )
}

fn is_preferred_fallback_family(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("noto ")
        || lower.starts_with("segoe ui")
        || lower.starts_with("leelawadee")
        || lower.starts_with("sarabun")
        || lower.starts_with("tahoma")
        || lower.starts_with("microsoft yahei")
        || lower.starts_with("yu gothic")
        || lower.starts_with("malgun gothic")
        || lower.starts_with("pingfang")
        || lower.starts_with("hiragino")
        || lower.starts_with("apple sd gothic")
        || lower.starts_with("dejavu ")
        || lower.starts_with("liberation ")
        || lower.starts_with("arial")
        || lower.starts_with("sf pro")
        || lower.starts_with("helvetica")
}
