//! Pagination, line breaking, table layout, and display-list construction for PDF export.

use super::fonts::{FontBook, FontRole, ShapedFontRun};
use super::model::{
    InlineSpan, InlineStyle, LinkTarget, ListItem, PdfBlock, PdfDocumentModel, PdfImage,
    PdfImageSource, TableAlign,
};
use crate::model::{PdfExportSettings, PdfTheme};
use krilla::image::Image as KrillaImage;
use unicode_segmentation::UnicodeSegmentation;

/// Maximum number of pages produced by one PDF export, keeping memory and CPU
/// bounded even on pathological documents.
pub(crate) const MAX_PDF_EXPORT_PAGES: usize = 2_000;

/// CSS pixels to PDF points conversion (`72 pt / 96 px`).
const PX_TO_PT: f32 = 0.75;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rgb(pub u8, pub u8, pub u8);

#[derive(Debug, Clone, Copy)]
pub(crate) struct ThemePalette {
    pub(crate) page_bg: Option<Rgb>,
    pub(crate) text: Rgb,
    pub(crate) heading: Rgb,
    pub(crate) heading_muted: Rgb,
    pub(crate) muted: Rgb,
    pub(crate) link: Rgb,
    pub(crate) highlight_bg: Rgb,
    pub(crate) inline_code_bg: Rgb,
    pub(crate) inline_code_text: Rgb,
    pub(crate) code_block_bg: Rgb,
    pub(crate) code_block_text: Rgb,
    pub(crate) code_block_lang: Rgb,
    pub(crate) quote_bar: Rgb,
    pub(crate) quote_text: Rgb,
    pub(crate) table_header_bg: Rgb,
    pub(crate) table_stripe_bg: Rgb,
    pub(crate) table_border: Rgb,
    pub(crate) rule: Rgb,
    pub(crate) checkbox_checked_bg: Rgb,
    pub(crate) checkbox_check: Rgb,
    pub(crate) placeholder_bg: Rgb,
    pub(crate) placeholder_border: Rgb,
}

impl ThemePalette {
    pub(crate) fn for_theme(theme: PdfTheme) -> Self {
        match theme {
            PdfTheme::Light => Self {
                page_bg: None,
                text: Rgb(34, 34, 34),
                heading: Rgb(17, 17, 17),
                heading_muted: Rgb(92, 99, 112),
                muted: Rgb(106, 115, 125),
                link: Rgb(9, 105, 218),
                highlight_bg: Rgb(254, 240, 138),
                inline_code_bg: Rgb(242, 244, 247),
                inline_code_text: Rgb(36, 41, 47),
                code_block_bg: Rgb(40, 44, 52),
                code_block_text: Rgb(203, 211, 224),
                code_block_lang: Rgb(142, 152, 168),
                quote_bar: Rgb(176, 184, 196),
                quote_text: Rgb(87, 96, 106),
                table_header_bg: Rgb(244, 246, 248),
                table_stripe_bg: Rgb(250, 251, 252),
                table_border: Rgb(216, 222, 228),
                rule: Rgb(220, 225, 231),
                checkbox_checked_bg: Rgb(9, 105, 218),
                checkbox_check: Rgb(255, 255, 255),
                placeholder_bg: Rgb(246, 248, 250),
                placeholder_border: Rgb(208, 215, 222),
            },
            PdfTheme::Sepia => Self {
                page_bg: Some(Rgb(248, 241, 227)),
                text: Rgb(59, 47, 32),
                heading: Rgb(36, 26, 16),
                heading_muted: Rgb(120, 104, 85),
                muted: Rgb(126, 110, 90),
                link: Rgb(154, 75, 22),
                highlight_bg: Rgb(245, 220, 122),
                inline_code_bg: Rgb(237, 227, 206),
                inline_code_text: Rgb(62, 48, 32),
                code_block_bg: Rgb(45, 38, 31),
                code_block_text: Rgb(230, 218, 200),
                code_block_lang: Rgb(168, 152, 132),
                quote_bar: Rgb(188, 168, 142),
                quote_text: Rgb(106, 90, 70),
                table_header_bg: Rgb(238, 228, 208),
                table_stripe_bg: Rgb(244, 236, 220),
                table_border: Rgb(215, 200, 174),
                rule: Rgb(218, 203, 177),
                checkbox_checked_bg: Rgb(154, 75, 22),
                checkbox_check: Rgb(248, 241, 227),
                placeholder_bg: Rgb(241, 232, 214),
                placeholder_border: Rgb(207, 192, 166),
            },
            PdfTheme::Dark => Self {
                page_bg: Some(Rgb(30, 30, 30)),
                text: Rgb(228, 228, 231),
                heading: Rgb(250, 250, 250),
                heading_muted: Rgb(161, 161, 170),
                muted: Rgb(161, 161, 170),
                link: Rgb(96, 165, 250),
                highlight_bg: Rgb(120, 90, 10),
                inline_code_bg: Rgb(44, 45, 49),
                inline_code_text: Rgb(228, 228, 231),
                code_block_bg: Rgb(20, 21, 24),
                code_block_text: Rgb(212, 212, 216),
                code_block_lang: Rgb(134, 140, 150),
                quote_bar: Rgb(90, 94, 104),
                quote_text: Rgb(176, 178, 186),
                table_header_bg: Rgb(39, 39, 44),
                table_stripe_bg: Rgb(34, 34, 38),
                table_border: Rgb(66, 68, 76),
                rule: Rgb(66, 68, 76),
                checkbox_checked_bg: Rgb(96, 165, 250),
                checkbox_check: Rgb(24, 24, 27),
                placeholder_bg: Rgb(37, 37, 42),
                placeholder_border: Rgb(70, 72, 80),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DrawRect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
    pub(crate) radius: f32,
    pub(crate) fill: Option<Rgb>,
    pub(crate) stroke: Option<(Rgb, f32)>,
}

#[derive(Debug, Clone)]
pub(crate) struct DrawLine {
    pub(crate) x1: f32,
    pub(crate) y1: f32,
    pub(crate) x2: f32,
    pub(crate) y2: f32,
    pub(crate) color: Rgb,
    pub(crate) width: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct DrawTextRun {
    pub(crate) x: f32,
    pub(crate) baseline_y: f32,
    pub(crate) font_size: f32,
    pub(crate) color: Rgb,
    pub(crate) run: ShapedFontRun,
}

#[derive(Debug, Clone)]
pub(crate) struct DrawImage {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
    pub(crate) image: KrillaImage,
}

#[derive(Debug, Clone)]
pub(crate) struct DrawCheckbox {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) size: f32,
    pub(crate) checked: bool,
    pub(crate) border: Rgb,
    pub(crate) checked_bg: Rgb,
    pub(crate) check_color: Rgb,
}

#[derive(Debug, Clone)]
pub(crate) enum DrawCmd {
    Rect(DrawRect),
    Line(DrawLine),
    Text(DrawTextRun),
    Image(DrawImage),
    Checkbox(DrawCheckbox),
}

#[derive(Debug, Clone)]
pub(crate) struct PageLink {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
    pub(crate) target: LinkTarget,
    pub(crate) alt: String,
}

#[derive(Debug, Clone)]
pub(crate) struct HeadingAnchor {
    pub(crate) level: u8,
    pub(crate) title: String,
    pub(crate) slug: String,
    pub(crate) page_idx: usize,
    pub(crate) y_pt: f32,
}

#[derive(Debug, Default)]
pub(crate) struct LayoutPage {
    pub(crate) cmds: Vec<DrawCmd>,
    pub(crate) links: Vec<PageLink>,
}

pub(crate) struct LaidOutDocument {
    pub(crate) page_w: f32,
    pub(crate) page_h: f32,
    pub(crate) margin: f32,
    pub(crate) body_pt: f32,
    pub(crate) palette: ThemePalette,
    pub(crate) pages: Vec<LayoutPage>,
    pub(crate) headings: Vec<HeadingAnchor>,
    pub(crate) truncated: bool,
}

/// Lay out `doc` into pages according to `settings`.
pub(crate) fn layout_document(
    doc: &PdfDocumentModel,
    settings: PdfExportSettings,
    fonts: &mut FontBook,
) -> LaidOutDocument {
    let (page_w, page_h) = settings.page_dimensions_pt();
    let margin = settings.margins.pt();
    let body_pt = settings.font_size.body_pt();
    let palette = ThemePalette::for_theme(settings.theme);

    let bottom_reserve = if settings.page_numbers {
        margin.max(36.0)
    } else {
        margin
    };

    let mut engine = LayoutEngine {
        page_w,
        page_h,
        margin,
        content_top: margin,
        content_bottom: (page_h - bottom_reserve).max(margin + 72.0),
        body_pt,
        palette,
        pages: vec![LayoutPage::default()],
        page_idx: 0,
        y: margin,
        headings: Vec::new(),
        truncated: false,
    };

    engine.layout_blocks(&doc.blocks, margin, page_w - 2.0 * margin, false, 0, fonts);

    LaidOutDocument {
        page_w: engine.page_w,
        page_h: engine.page_h,
        margin: engine.margin,
        body_pt: engine.body_pt,
        palette: engine.palette,
        pages: engine.pages,
        headings: engine.headings,
        truncated: engine.truncated,
    }
}

struct LayoutEngine {
    page_w: f32,
    page_h: f32,
    margin: f32,
    content_top: f32,
    content_bottom: f32,
    body_pt: f32,
    palette: ThemePalette,
    pages: Vec<LayoutPage>,
    page_idx: usize,
    y: f32,
    headings: Vec<HeadingAnchor>,
    truncated: bool,
}

#[derive(Debug, Clone)]
struct StyledLineSegment {
    style: InlineStyle,
    font_size: f32,
    color: Rgb,
    runs: Vec<ShapedFontRun>,
    inline_image: Option<(KrillaImage, f32, f32, String)>,
    width: f32,
    pad_x: f32,
    ascent: f32,
    descent: f32,
}

#[derive(Debug, Clone)]
struct WrappedLine {
    segments: Vec<StyledLineSegment>,
    width: f32,
    ascent: f32,
    descent: f32,
    height: f32,
}

impl LayoutEngine {
    fn at_page_top(&self) -> bool {
        (self.y - self.content_top).abs() < 0.5
    }

    fn remaining_height(&self) -> f32 {
        (self.content_bottom - self.y).max(0.0)
    }

    fn content_height(&self) -> f32 {
        (self.content_bottom - self.content_top).max(72.0)
    }

    fn new_page(&mut self) -> bool {
        if self.pages.len() >= MAX_PDF_EXPORT_PAGES {
            self.truncated = true;
            return false;
        }
        self.pages.push(LayoutPage::default());
        self.page_idx = self.pages.len() - 1;
        self.y = self.content_top;
        true
    }

    fn ensure_space(&mut self, needed: f32) -> bool {
        if self.truncated {
            return false;
        }
        if self.y + needed <= self.content_bottom || self.at_page_top() {
            return true;
        }
        self.new_page()
    }

    fn push_cmd(&mut self, cmd: DrawCmd) {
        if let Some(page) = self.pages.get_mut(self.page_idx) {
            page.cmds.push(cmd);
        }
    }

    fn push_link(&mut self, link: PageLink) {
        if let Some(page) = self.pages.get_mut(self.page_idx) {
            page.links.push(link);
        }
    }

    fn add_gap(&mut self, gap: f32) {
        if !self.at_page_top() && !self.truncated {
            self.y += gap;
        }
    }

    fn layout_blocks(
        &mut self,
        blocks: &[PdfBlock],
        left: f32,
        width: f32,
        in_quote: bool,
        list_depth: usize,
        fonts: &mut FontBook,
    ) {
        for (idx, block) in blocks.iter().enumerate() {
            if self.truncated {
                return;
            }
            let has_next = idx + 1 < blocks.len();
            match block {
                PdfBlock::Heading {
                    level,
                    spans,
                    plain_text,
                    slug,
                } => {
                    self.layout_heading(
                        *level, spans, plain_text, slug, left, width, has_next, fonts,
                    );
                }
                PdfBlock::Paragraph(spans) => {
                    let color = if in_quote {
                        self.palette.quote_text
                    } else {
                        self.palette.text
                    };
                    self.layout_paragraph(spans, left, width, self.body_pt, 1.52, color, fonts);
                    self.add_gap(self.body_pt * 0.65);
                }
                PdfBlock::CodeBlock { lang, code } => {
                    self.layout_code_block(lang.as_deref(), code, left, width, fonts);
                    self.add_gap(self.body_pt * 0.7);
                }
                PdfBlock::PlainCodeLines(lines) => {
                    self.layout_plain_code_lines(lines, left, width, fonts);
                }
                PdfBlock::BlockQuote(inner) => {
                    self.layout_blockquote(inner, left, width, list_depth, fonts);
                    self.add_gap(self.body_pt * 0.65);
                }
                PdfBlock::List {
                    ordered_start,
                    items,
                } => {
                    self.layout_list(
                        *ordered_start,
                        items,
                        left,
                        width,
                        in_quote,
                        list_depth,
                        fonts,
                    );
                    if list_depth == 0 {
                        self.add_gap(self.body_pt * 0.55);
                    }
                }
                PdfBlock::Table {
                    alignments,
                    header,
                    rows,
                } => {
                    self.layout_table(alignments, header, rows, left, width, fonts);
                    self.add_gap(self.body_pt * 0.75);
                }
                PdfBlock::Image(img) => {
                    self.layout_image(img, left, width, fonts);
                    self.add_gap(self.body_pt * 0.75);
                }
                PdfBlock::HorizontalRule => {
                    self.layout_hr(left, width);
                }
                PdfBlock::FootnoteSection(entries) => {
                    self.layout_footnotes(entries, left, width, fonts);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_heading(
        &mut self,
        level: u8,
        spans: &[InlineSpan],
        plain_text: &str,
        slug: &str,
        left: f32,
        width: f32,
        has_next: bool,
        fonts: &mut FontBook,
    ) {
        let scale = match level {
            1 => 1.85,
            2 => 1.48,
            3 => 1.24,
            4 => 1.10,
            5 => 1.00,
            _ => 0.94,
        };
        let font_size = self.body_pt * scale;
        let top_gap = if self.at_page_top() {
            0.0
        } else {
            self.body_pt
                * match level {
                    1 => 1.2,
                    2 => 1.05,
                    _ => 0.85,
                }
        };
        let color = if level >= 6 {
            self.palette.heading_muted
        } else {
            self.palette.heading
        };

        let bold_spans: Vec<InlineSpan> = spans
            .iter()
            .map(|s| {
                let mut st = s.style.clone();
                st.bold = true;
                InlineSpan {
                    text: s.text.clone(),
                    style: st,
                    image: s.image.clone(),
                }
            })
            .collect();

        let lines = self.wrap_inline_spans(&bold_spans, width, font_size, 1.32, color, fonts);
        let text_h: f32 = lines.iter().map(|l| l.height).sum();
        let underline_extra = if level <= 2 { 6.0 } else { 0.0 };
        let after_gap = self.body_pt * 0.45;
        // Keep heading attached to the start of the following block (no orphan headings).
        let follow_reserve = if has_next { self.body_pt * 3.2 } else { 0.0 };
        let total_needed = top_gap + text_h + underline_extra + after_gap + follow_reserve;

        if !self.at_page_top() && self.y + total_needed > self.content_bottom {
            if !self.new_page() {
                return;
            }
        } else {
            self.add_gap(top_gap);
        }

        // Record heading destination for both the PDF outline and internal `#slug` links.
        let anchor_y = (self.y - 6.0).max(0.0);
        self.headings.push(HeadingAnchor {
            level,
            title: plain_text.to_string(),
            slug: slug.to_string(),
            page_idx: self.page_idx,
            y_pt: anchor_y,
        });

        for line in &lines {
            if !self.ensure_space(line.height) {
                return;
            }
            self.emit_wrapped_line(line, left, width, TableAlign::Left);
            self.y += line.height;
        }

        if level <= 2 {
            self.y += 2.5;
            self.push_cmd(DrawCmd::Line(DrawLine {
                x1: left,
                y1: self.y,
                x2: left + width,
                y2: self.y,
                color: self.palette.rule,
                width: if level == 1 { 0.85 } else { 0.55 },
            }));
            self.y += 3.5;
        }

        self.add_gap(after_gap);
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_paragraph(
        &mut self,
        spans: &[InlineSpan],
        left: f32,
        width: f32,
        font_size: f32,
        line_mult: f32,
        color: Rgb,
        fonts: &mut FontBook,
    ) {
        let lines = self.wrap_inline_spans(spans, width, font_size, line_mult, color, fonts);
        self.emit_lines(&lines, left, width);
    }

    /// Emit already-wrapped lines top to bottom, breaking pages as needed.
    fn emit_lines(&mut self, lines: &[WrappedLine], left: f32, width: f32) {
        for line in lines {
            if !self.ensure_space(line.height) {
                return;
            }
            self.emit_wrapped_line(line, left, width, TableAlign::Left);
            self.y += line.height;
        }
    }

    fn layout_code_block(
        &mut self,
        lang: Option<&str>,
        code: &str,
        left: f32,
        width: f32,
        fonts: &mut FontBook,
    ) {
        let font_size = (self.body_pt * 0.84).max(7.5);
        let line_h = font_size * 1.42;
        let pad_x = 10.0;
        let pad_y = 8.0;
        let inner_w = (width - 2.0 * pad_x).max(40.0);

        let wrapped = self.wrap_monospace_lines(
            code.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)),
            inner_w,
            font_size,
            line_h,
            self.palette.code_block_text,
            fonts,
        );
        if wrapped.is_empty() {
            return;
        }

        let lang_run = lang.filter(|s| !s.is_empty()).map(|l| {
            let sz = (font_size * 0.85).max(7.0);
            let runs = fonts.shape_text(l, FontRole::MonoRegular, sz);
            let w: f32 = runs.iter().map(|r| r.width).sum();
            (runs, w, sz)
        });

        let mut cursor = 0usize;
        let mut first_slice = true;
        while cursor < wrapped.len() {
            if self.truncated {
                return;
            }
            let min_one = pad_y * 2.0 + line_h;
            if !self.ensure_space(min_one) {
                return;
            }
            let avail = self.remaining_height();
            let max_lines = (((avail - 2.0 * pad_y) / line_h).floor() as usize).max(1);
            let end = (cursor + max_lines).min(wrapped.len());
            let slice = &wrapped[cursor..end];
            let box_h = 2.0 * pad_y + slice.len() as f32 * line_h;
            let box_y = self.y;

            self.push_cmd(DrawCmd::Rect(DrawRect {
                x: left,
                y: box_y,
                w: width,
                h: box_h,
                radius: 4.5,
                fill: Some(self.palette.code_block_bg),
                stroke: None,
            }));

            if first_slice {
                if let Some((ref runs, lw, lsz)) = lang_run {
                    let lx = (left + width - pad_x - lw).max(left + pad_x);
                    let ly = box_y + pad_y + fonts.ascent(FontRole::MonoRegular, lsz);
                    let mut x = lx;
                    for r in runs {
                        let rw = r.width;
                        self.push_cmd(DrawCmd::Text(DrawTextRun {
                            x,
                            baseline_y: ly,
                            font_size: lsz,
                            color: self.palette.code_block_lang,
                            run: r.clone(),
                        }));
                        x += rw;
                    }
                }
            }

            // Each line is drawn at `self.y`, so set it before (not after)
            // emitting; otherwise the first line ignores the top padding.
            let mut cur_y = box_y + pad_y;
            for line in slice {
                self.y = cur_y;
                self.emit_wrapped_line(line, left + pad_x, inner_w, TableAlign::Left);
                cur_y += line_h;
            }
            self.y = box_y + box_h;

            cursor = end;
            first_slice = false;
            if cursor < wrapped.len() && !self.new_page() {
                return;
            }
        }
    }

    fn layout_plain_code_lines(
        &mut self,
        lines: &[String],
        left: f32,
        width: f32,
        fonts: &mut FontBook,
    ) {
        let font_size = (self.body_pt * 0.85).max(7.5);
        let line_h = font_size * 1.42;
        let wrapped = self.wrap_monospace_lines(
            lines.iter().map(String::as_str),
            width,
            font_size,
            line_h,
            self.palette.text,
            fonts,
        );
        for line in &wrapped {
            if !self.ensure_space(line.height) {
                return;
            }
            self.emit_wrapped_line(line, left, width, TableAlign::Left);
            self.y += line.height;
        }
    }

    fn layout_blockquote(
        &mut self,
        inner: &[PdfBlock],
        left: f32,
        width: f32,
        list_depth: usize,
        fonts: &mut FontBook,
    ) {
        let bar_w = 3.0;
        let indent = 13.0;
        let start_page = self.page_idx;
        let start_y = self.y;

        let inner_w = (width - indent).max(60.0);
        self.layout_blocks(inner, left + indent, inner_w, true, list_depth, fonts);

        let end_page = self.page_idx;
        let end_y = self.y;
        let bar_color = self.palette.quote_bar;

        // Draw the vertical left bar on every page the blockquote spanned.
        for p in start_page..=end_page {
            let seg_top = if p == start_page {
                start_y
            } else {
                self.content_top
            };
            let seg_bot = if p == end_page {
                (end_y - self.body_pt * 0.4).max(seg_top + self.body_pt)
            } else {
                self.content_bottom
            };
            if seg_bot > seg_top + 1.0 {
                if let Some(page) = self.pages.get_mut(p) {
                    page.cmds.push(DrawCmd::Rect(DrawRect {
                        x: left + 1.0,
                        y: seg_top,
                        w: bar_w,
                        h: seg_bot - seg_top,
                        radius: 1.5,
                        fill: Some(bar_color),
                        stroke: None,
                    }));
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_list(
        &mut self,
        ordered_start: Option<u64>,
        items: &[ListItem],
        left: f32,
        width: f32,
        in_quote: bool,
        depth: usize,
        fonts: &mut FontBook,
    ) {
        let indent = if ordered_start.is_some() { 20.0 } else { 16.0 };
        let marker_color = if in_quote {
            self.palette.quote_text
        } else {
            self.palette.muted
        };
        let ascent = fonts.ascent(FontRole::Regular, self.body_pt);
        let line_h = self.body_pt * 1.52;
        let inner_w = (width - indent).max(50.0);
        let para_color = if in_quote {
            self.palette.quote_text
        } else {
            self.palette.text
        };

        for (idx, item) in items.iter().enumerate() {
            if self.truncated {
                return;
            }
            // Wrap a leading paragraph up front so the marker is only drawn
            // where its first line also fits, never orphaned on the page before.
            let first_para_lines = match item.blocks.first() {
                Some(PdfBlock::Paragraph(spans)) => Some(self.wrap_inline_spans(
                    spans,
                    inner_w,
                    self.body_pt,
                    1.52,
                    para_color,
                    fonts,
                )),
                _ => None,
            };
            let first_h = match (&first_para_lines, item.blocks.first()) {
                (Some(lines), _) => lines.first().map_or(line_h, |l| l.height.max(line_h)),
                (None, None) => line_h,
                // Room for the start of a code block, table, quote, etc.
                (None, Some(_)) => (self.body_pt * 3.2).max(line_h),
            };
            if !self.ensure_space(first_h) {
                return;
            }
            let item_y = self.y;
            let baseline_y = item_y
                + ascent
                + (line_h - ascent - fonts.descent(FontRole::Regular, self.body_pt)) * 0.35;

            if let Some(checked) = item.task {
                let box_sz = (self.body_pt * 0.85).clamp(7.5, 11.0);
                let bx = left + 2.0;
                let by = baseline_y - box_sz + 1.0;
                self.push_cmd(DrawCmd::Checkbox(DrawCheckbox {
                    x: bx,
                    y: by,
                    size: box_sz,
                    checked,
                    border: self.palette.muted,
                    checked_bg: self.palette.checkbox_checked_bg,
                    check_color: self.palette.checkbox_check,
                }));
            } else if let Some(start) = ordered_start {
                let num_text = format!("{}.", start + idx as u64);
                let runs = fonts.shape_text(&num_text, FontRole::Regular, self.body_pt);
                let mut x = left;
                for r in runs {
                    let rw = r.width;
                    self.push_cmd(DrawCmd::Text(DrawTextRun {
                        x,
                        baseline_y,
                        font_size: self.body_pt,
                        color: marker_color,
                        run: r,
                    }));
                    x += rw;
                }
            } else {
                let bullet = match depth % 3 {
                    0 => "\u{2022}",
                    1 => "\u{25E6}",
                    _ => "\u{25AA}",
                };
                let runs = fonts.shape_text(bullet, FontRole::Regular, self.body_pt);
                let mut x = left + 4.0;
                for r in runs {
                    let rw = r.width;
                    self.push_cmd(DrawCmd::Text(DrawTextRun {
                        x,
                        baseline_y,
                        font_size: self.body_pt,
                        color: marker_color,
                        run: r,
                    }));
                    x += rw;
                }
            }

            if item.blocks.is_empty() {
                self.y += line_h;
            } else {
                // Tighten paragraph gaps slightly inside list items.
                for (bi, child) in item.blocks.iter().enumerate() {
                    if self.truncated {
                        return;
                    }
                    match child {
                        PdfBlock::Paragraph(_) if bi == 0 && first_para_lines.is_some() => {
                            if let Some(lines) = &first_para_lines {
                                self.emit_lines(lines, left + indent, inner_w);
                            }
                            if bi + 1 < item.blocks.len() {
                                self.add_gap(self.body_pt * 0.35);
                            }
                        }
                        PdfBlock::Paragraph(spans) => {
                            self.layout_paragraph(
                                spans,
                                left + indent,
                                inner_w,
                                self.body_pt,
                                1.52,
                                para_color,
                                fonts,
                            );
                            if bi + 1 < item.blocks.len() {
                                self.add_gap(self.body_pt * 0.35);
                            }
                        }
                        other => {
                            self.layout_blocks(
                                std::slice::from_ref(other),
                                left + indent,
                                inner_w,
                                in_quote,
                                depth + 1,
                                fonts,
                            );
                        }
                    }
                }
            }
            self.add_gap(self.body_pt * 0.28);
        }
    }

    fn layout_table(
        &mut self,
        alignments: &[TableAlign],
        header: &[Vec<InlineSpan>],
        rows: &[Vec<Vec<InlineSpan>>],
        left: f32,
        width: f32,
        fonts: &mut FontBook,
    ) {
        let num_cols = alignments
            .len()
            .max(header.len())
            .max(rows.iter().map(Vec::len).max().unwrap_or(0));
        if num_cols == 0 {
            return;
        }

        let cell_pt = (self.body_pt * 0.92).max(8.0);
        let pad_x = 6.5;
        let pad_y = 5.0;

        // Measure natural column widths from header & body cells.
        let mut desired = vec![24.0f32; num_cols];
        let max_cell_img_h = (self.content_height() * 0.5).max(cell_pt * 4.0);
        let mut measure_row = |cells: &[Vec<InlineSpan>], is_head: bool| {
            for (c, cell) in cells.iter().enumerate().take(num_cols) {
                let mut w = 0.0f32;
                for span in cell {
                    if let Some(img) = &span.image {
                        if let PdfImageSource::Embedded {
                            pixel_width,
                            pixel_height,
                            ..
                        } = &img.source
                        {
                            let (iw, _) = fit_image_size(
                                img,
                                *pixel_width,
                                *pixel_height,
                                width * 0.6,
                                max_cell_img_h,
                            );
                            w += iw;
                            continue;
                        }
                    }
                    let role = FontRole::from_style(
                        is_head || span.style.bold,
                        span.style.italic,
                        span.style.code,
                    );
                    let sz = if span.style.code {
                        cell_pt * 0.88
                    } else {
                        cell_pt
                    };
                    w += fonts.measure_width(&span.text, role, sz);
                }
                desired[c] = desired[c].max(w + 2.0 * pad_x + 4.0);
            }
        };
        measure_row(header, true);
        for row in rows {
            measure_row(row, false);
        }

        let col_widths = distribute_column_widths(&desired, width);

        // Pre-wrap header cells once so we can repeat the header on page breaks.
        let wrapped_header: Vec<Vec<WrappedLine>> = (0..num_cols)
            .map(|c| {
                let spans = header.get(c).map(Vec::as_slice).unwrap_or(&[]);
                let bold_spans: Vec<InlineSpan> = spans
                    .iter()
                    .map(|s| {
                        let mut st = s.style.clone();
                        st.bold = true;
                        InlineSpan {
                            text: s.text.clone(),
                            style: st,
                            image: s.image.clone(),
                        }
                    })
                    .collect();
                let inner_w = (col_widths[c] - 2.0 * pad_x).max(16.0);
                self.wrap_inline_spans(
                    &bold_spans,
                    inner_w,
                    cell_pt,
                    1.38,
                    self.palette.heading,
                    fonts,
                )
            })
            .collect();
        let header_h = table_row_height(&wrapped_header, cell_pt, pad_y);

        // Pre-wrap body rows.
        let wrapped_rows: Vec<(Vec<Vec<WrappedLine>>, f32)> = rows
            .iter()
            .map(|row| {
                let cells: Vec<Vec<WrappedLine>> = (0..num_cols)
                    .map(|c| {
                        let spans = row.get(c).map(Vec::as_slice).unwrap_or(&[]);
                        let inner_w = (col_widths[c] - 2.0 * pad_x).max(16.0);
                        self.wrap_inline_spans(
                            spans,
                            inner_w,
                            cell_pt,
                            1.38,
                            self.palette.text,
                            fonts,
                        )
                    })
                    .collect();
                let h = table_row_height(&cells, cell_pt, pad_y);
                (cells, h)
            })
            .collect();

        // Avoid stranding the header row alone at the bottom of a page.
        let first_body_h = wrapped_rows.first().map_or(0.0, |(_, h)| *h);
        if !self.at_page_top()
            && self.y + header_h + first_body_h > self.content_bottom
            && !self.new_page()
        {
            return;
        }

        // Emit header row.
        if !header.is_empty() {
            if !self.ensure_space(header_h) {
                return;
            }
            self.emit_table_row(
                &wrapped_header,
                &col_widths,
                alignments,
                left,
                header_h,
                pad_x,
                pad_y,
                Some(self.palette.table_header_bg),
            );
            self.y += header_h;
        }

        // Starts a new page and repeats the header row on it.
        let table_page_break = |this: &mut Self| -> bool {
            if !this.new_page() {
                return false;
            }
            if !header.is_empty() {
                this.emit_table_row(
                    &wrapped_header,
                    &col_widths,
                    alignments,
                    left,
                    header_h,
                    pad_x,
                    pad_y,
                    Some(this.palette.table_header_bg),
                );
                this.y += header_h;
            }
            true
        };
        let header_reserve = if header.is_empty() { 0.0 } else { header_h };

        // Emit body rows, repeating the header row whenever a page break occurs.
        for (row_idx, (cells, row_h)) in wrapped_rows.iter().enumerate() {
            if self.truncated {
                return;
            }
            let bg = if row_idx % 2 == 1 {
                Some(self.palette.table_stripe_bg)
            } else {
                None
            };

            // A row taller than a whole page (below the repeated header) is
            // split line by line across pages instead of running off the bottom.
            if *row_h > self.content_height() - header_reserve {
                let mut rest: Vec<&[WrappedLine]> = cells.iter().map(Vec::as_slice).collect();
                let mut fresh_page = false;
                while rest.iter().any(|lines| !lines.is_empty()) {
                    if self.truncated {
                        return;
                    }
                    let avail = self.content_bottom - self.y - 2.0 * pad_y;
                    let mut chunk: Vec<Vec<WrappedLine>> = Vec::with_capacity(rest.len());
                    for lines in rest.iter_mut() {
                        let mut used = 0.0f32;
                        let mut take = 0usize;
                        for line in lines.iter() {
                            if used + line.height > avail && !(fresh_page && take == 0) {
                                break;
                            }
                            used += line.height;
                            take += 1;
                        }
                        chunk.push(lines[..take].to_vec());
                        *lines = &lines[take..];
                    }
                    if chunk.iter().all(Vec::is_empty) {
                        // Nothing fits in what's left of this page.
                        if !table_page_break(self) {
                            return;
                        }
                        fresh_page = true;
                        continue;
                    }
                    let chunk_h = table_row_height(&chunk, cell_pt, pad_y);
                    self.emit_table_row(
                        &chunk,
                        &col_widths,
                        alignments,
                        left,
                        chunk_h,
                        pad_x,
                        pad_y,
                        bg,
                    );
                    self.y += chunk_h;
                    if rest.iter().any(|lines| !lines.is_empty()) {
                        if !table_page_break(self) {
                            return;
                        }
                        fresh_page = true;
                    }
                }
                continue;
            }

            if !self.at_page_top()
                && self.y + *row_h > self.content_bottom
                && !table_page_break(self)
            {
                return;
            }
            self.emit_table_row(
                cells,
                &col_widths,
                alignments,
                left,
                *row_h,
                pad_x,
                pad_y,
                bg,
            );
            self.y += *row_h;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_table_row(
        &mut self,
        cells: &[Vec<WrappedLine>],
        col_widths: &[f32],
        alignments: &[TableAlign],
        left: f32,
        row_h: f32,
        pad_x: f32,
        pad_y: f32,
        bg: Option<Rgb>,
    ) {
        let total_w: f32 = col_widths.iter().sum();
        let row_y = self.y;
        self.push_cmd(DrawCmd::Rect(DrawRect {
            x: left,
            y: row_y,
            w: total_w,
            h: row_h,
            radius: 0.0,
            fill: bg,
            stroke: Some((self.palette.table_border, 0.55)),
        }));

        let mut col_x = left;
        for (c, &cw) in col_widths.iter().enumerate() {
            if c > 0 {
                self.push_cmd(DrawCmd::Line(DrawLine {
                    x1: col_x,
                    y1: row_y,
                    x2: col_x,
                    y2: row_y + row_h,
                    color: self.palette.table_border,
                    width: 0.55,
                }));
            }
            let align = alignments.get(c).copied().unwrap_or(TableAlign::Left);
            let inner_w = (cw - 2.0 * pad_x).max(1.0);
            let mut cy = row_y + pad_y;
            if let Some(lines) = cells.get(c) {
                for line in lines {
                    let saved_y = self.y;
                    self.y = cy;
                    self.emit_wrapped_line(line, col_x + pad_x, inner_w, align);
                    self.y = saved_y;
                    cy += line.height;
                }
            }
            col_x += cw;
        }
    }

    fn layout_image(&mut self, img: &PdfImage, left: f32, width: f32, fonts: &mut FontBook) {
        match &img.source {
            PdfImageSource::Embedded {
                image,
                pixel_width,
                pixel_height,
            } => {
                let max_h = self.content_height() * 0.85;
                let (draw_w, draw_h) =
                    fit_image_size(img, *pixel_width, *pixel_height, width, max_h);

                if !self.ensure_space(draw_h) {
                    return;
                }
                self.push_cmd(DrawCmd::Image(DrawImage {
                    x: left,
                    y: self.y,
                    w: draw_w,
                    h: draw_h,
                    image: image.clone(),
                }));
                self.y += draw_h;
            }
            PdfImageSource::Placeholder { remote_uri } => {
                let label = if !img.alt.trim().is_empty() {
                    format!("Image: {}", img.alt.trim())
                } else if let Some(uri) = remote_uri {
                    format!("Image: {uri}")
                } else {
                    "Image unavailable".to_string()
                };
                let font_size = (self.body_pt * 0.9).max(8.5);
                let pad_x = 10.0;
                let pad_y = 8.0;
                let inner_w = (width - 2.0 * pad_x).max(40.0);
                let style = InlineStyle {
                    italic: true,
                    link: remote_uri.clone().map(LinkTarget::Uri),
                    ..Default::default()
                };
                let color = if remote_uri.is_some() {
                    self.palette.link
                } else {
                    self.palette.muted
                };
                let lines = self.wrap_inline_spans(
                    &[InlineSpan::text(label.clone(), style)],
                    inner_w,
                    font_size,
                    1.4,
                    color,
                    fonts,
                );
                let text_h: f32 = lines.iter().map(|l| l.height).sum();
                let box_h = text_h + 2.0 * pad_y;
                if !self.ensure_space(box_h) {
                    return;
                }
                let box_y = self.y;
                self.push_cmd(DrawCmd::Rect(DrawRect {
                    x: left,
                    y: box_y,
                    w: width,
                    h: box_h,
                    radius: 4.0,
                    fill: Some(self.palette.placeholder_bg),
                    stroke: Some((self.palette.placeholder_border, 0.7)),
                }));
                let mut cy = box_y + pad_y;
                for line in &lines {
                    let saved = self.y;
                    self.y = cy;
                    self.emit_wrapped_line(line, left + pad_x, inner_w, TableAlign::Left);
                    self.y = saved;
                    cy += line.height;
                }
                if let Some(uri) = remote_uri {
                    self.push_link(PageLink {
                        x: left,
                        y: box_y,
                        w: width,
                        h: box_h,
                        target: LinkTarget::Uri(uri.clone()),
                        alt: label,
                    });
                }
                self.y = box_y + box_h;
            }
        }
    }

    fn layout_hr(&mut self, left: f32, width: f32) {
        let gap = self.body_pt * 0.8;
        if !self.ensure_space(gap * 2.0 + 1.0) {
            return;
        }
        self.add_gap(gap);
        self.push_cmd(DrawCmd::Line(DrawLine {
            x1: left,
            y1: self.y,
            x2: left + width,
            y2: self.y,
            color: self.palette.rule,
            width: 0.75,
        }));
        self.y += gap;
    }

    fn layout_footnotes(
        &mut self,
        entries: &[super::model::FootnoteEntry],
        left: f32,
        width: f32,
        fonts: &mut FontBook,
    ) {
        if entries.is_empty() {
            return;
        }
        self.add_gap(self.body_pt * 1.1);
        if !self.ensure_space(self.body_pt * 3.0) {
            return;
        }
        self.push_cmd(DrawCmd::Line(DrawLine {
            x1: left,
            y1: self.y,
            x2: left + (width * 0.35).min(160.0),
            y2: self.y,
            color: self.palette.rule,
            width: 0.7,
        }));
        self.y += self.body_pt * 0.7;

        let fn_pt = (self.body_pt * 0.88).max(8.0);
        for entry in entries {
            if self.truncated {
                return;
            }
            let prefix = InlineSpan::text(
                format!("[{}] ", entry.label),
                InlineStyle {
                    bold: true,
                    ..Default::default()
                },
            );
            if let Some(PdfBlock::Paragraph(first)) = entry.blocks.first() {
                let mut combined = Vec::with_capacity(first.len() + 1);
                combined.push(prefix);
                combined.extend(first.iter().cloned());
                self.layout_paragraph(
                    &combined,
                    left,
                    width,
                    fn_pt,
                    1.45,
                    self.palette.muted,
                    fonts,
                );
                self.add_gap(fn_pt * 0.45);
                if entry.blocks.len() > 1 {
                    self.layout_blocks(
                        &entry.blocks[1..],
                        left + 14.0,
                        (width - 14.0).max(50.0),
                        true,
                        0,
                        fonts,
                    );
                }
            } else {
                self.layout_paragraph(
                    &[prefix],
                    left,
                    width,
                    fn_pt,
                    1.45,
                    self.palette.muted,
                    fonts,
                );
                self.layout_blocks(
                    &entry.blocks,
                    left + 14.0,
                    (width - 14.0).max(50.0),
                    true,
                    0,
                    fonts,
                );
            }
        }
    }

    /// Wrap rich inline spans into shaped lines ready to paint.
    fn wrap_inline_spans(
        &self,
        spans: &[InlineSpan],
        max_width: f32,
        base_font_size: f32,
        line_mult: f32,
        base_color: Rgb,
        fonts: &mut FontBook,
    ) -> Vec<WrappedLine> {
        #[derive(Clone)]
        enum Token {
            Newline,
            Space {
                text: String,
                style: InlineStyle,
            },
            Piece {
                text: String,
                style: InlineStyle,
            },
            InlineImage {
                image: Box<PdfImage>,
                style: InlineStyle,
            },
        }

        #[derive(Clone)]
        enum LineItem {
            Text(String, InlineStyle),
            Image {
                image: KrillaImage,
                w: f32,
                h: f32,
                alt: String,
                style: InlineStyle,
            },
        }

        // Tokenize into whitespace, hard newlines, inline images, and breakable word/CJK pieces.
        let mut tokens: Vec<Token> = Vec::new();
        for span in spans {
            if let Some(img) = &span.image {
                tokens.push(Token::InlineImage {
                    image: img.clone(),
                    style: span.style.clone(),
                });
                continue;
            }
            let mut rem = span.text.as_str();
            while !rem.is_empty() {
                if let Some(nl) = rem.find('\n') {
                    let before = &rem[..nl];
                    tokenize_text_into(before, &span.style, &mut tokens);
                    tokens.push(Token::Newline);
                    rem = &rem[nl + 1..];
                } else {
                    tokenize_text_into(rem, &span.style, &mut tokens);
                    break;
                }
            }
        }

        fn tokenize_text_into(text: &str, style: &InlineStyle, out: &mut Vec<Token>) {
            let mut i = 0usize;
            let bytes = text.as_bytes();
            while i < bytes.len() {
                let ch = text[i..].chars().next().unwrap();
                if ch == ' ' || ch == '\t' {
                    let start = i;
                    while i < text.len() {
                        let c = text[i..].chars().next().unwrap();
                        if c == ' ' || c == '\t' {
                            i += c.len_utf8();
                        } else {
                            break;
                        }
                    }
                    out.push(Token::Space {
                        text: text[start..i].replace('\t', "    "),
                        style: style.clone(),
                    });
                } else if is_cjk_breakable(ch) {
                    // CJK ideographs can wrap cleanly between characters.
                    let end = i + ch.len_utf8();
                    out.push(Token::Piece {
                        text: text[i..end].to_string(),
                        style: style.clone(),
                    });
                    i = end;
                } else {
                    let start = i;
                    while i < text.len() {
                        let c = text[i..].chars().next().unwrap();
                        if c == ' ' || c == '\t' || is_cjk_breakable(c) {
                            break;
                        }
                        i += c.len_utf8();
                    }
                    out.push(Token::Piece {
                        text: text[start..i].to_string(),
                        style: style.clone(),
                    });
                }
            }
        }

        // Pack tokens into lines, coalescing adjacent same-style text before final shaping.
        let mut raw_lines: Vec<Vec<LineItem>> = Vec::new();
        let mut cur_line: Vec<LineItem> = Vec::new();
        let mut cur_w = 0.0f32;
        let max_w = max_width.max(16.0);
        let max_inline_img_h = (self.content_height() * 0.65).max(base_font_size * 4.0);

        let measure_piece = |text: &str, style: &InlineStyle, fonts: &mut FontBook| -> f32 {
            let role = FontRole::from_style(style.bold, style.italic, style.code);
            let sz = if style.code {
                base_font_size * 0.88
            } else {
                base_font_size
            };
            fonts.measure_width(text, role, sz)
        };

        // Inline code is drawn with 2.2pt of padding on each side once per
        // coalesced segment, so only a piece that starts a new code segment
        // on the line pays for it.
        const CODE_PAD: f32 = 4.4;
        let segment_pad = |line: &[LineItem], style: &InlineStyle| -> f32 {
            let continues = matches!(line.last(), Some(LineItem::Text(_, last)) if last == style);
            if style.code && !continues {
                CODE_PAD
            } else {
                0.0
            }
        };
        let fresh_pad = |style: &InlineStyle| if style.code { CODE_PAD } else { 0.0 };

        let push_to_line = |line: &mut Vec<LineItem>, text: &str, style: &InlineStyle| {
            if text.is_empty() {
                return;
            }
            if let Some(LineItem::Text(last_text, last_style)) = line.last_mut() {
                if last_style == style {
                    last_text.push_str(text);
                    return;
                }
            }
            line.push(LineItem::Text(text.to_string(), style.clone()));
        };

        let trim_line_trailing_space = |line: &mut Vec<LineItem>| {
            while let Some(LineItem::Text(text, _)) = line.last_mut() {
                let trimmed_len = text.trim_end_matches(' ').len();
                if trimmed_len == 0 {
                    line.pop();
                } else {
                    text.truncate(trimmed_len);
                    break;
                }
            }
        };

        // Leading spaces are kept at the start of the text and after a hard
        // newline (indentation in plain text), but dropped on soft-wrapped lines.
        let mut keep_leading_space = true;

        for token in tokens {
            match token {
                Token::Newline => {
                    raw_lines.push(std::mem::take(&mut cur_line));
                    cur_w = 0.0;
                    keep_leading_space = true;
                }
                Token::Space { text, style } => {
                    if cur_line.is_empty() && !keep_leading_space {
                        continue;
                    }
                    let w = measure_piece(&text, &style, fonts) + segment_pad(&cur_line, &style);
                    if cur_w + w <= max_w {
                        push_to_line(&mut cur_line, &text, &style);
                        cur_w += w;
                    } else {
                        trim_line_trailing_space(&mut cur_line);
                        if !cur_line.is_empty() {
                            raw_lines.push(std::mem::take(&mut cur_line));
                        }
                        cur_w = 0.0;
                        keep_leading_space = false;
                    }
                }
                Token::InlineImage { image, style } => {
                    if let PdfImageSource::Embedded {
                        image: krilla_img,
                        pixel_width,
                        pixel_height,
                    } = &image.source
                    {
                        let (draw_w, draw_h) = fit_image_size(
                            &image,
                            *pixel_width,
                            *pixel_height,
                            max_w,
                            max_inline_img_h,
                        );
                        if !cur_line.is_empty() && cur_w + draw_w > max_w {
                            trim_line_trailing_space(&mut cur_line);
                            if !cur_line.is_empty() {
                                raw_lines.push(std::mem::take(&mut cur_line));
                            }
                            cur_w = 0.0;
                            keep_leading_space = false;
                        }
                        cur_line.push(LineItem::Image {
                            image: krilla_img.clone(),
                            w: draw_w,
                            h: draw_h,
                            alt: image.alt.clone(),
                            style,
                        });
                        cur_w += draw_w;
                    }
                }
                Token::Piece { text, style } => {
                    let text_w = measure_piece(&text, &style, fonts);
                    let w = text_w + segment_pad(&cur_line, &style);
                    // Width when the piece starts a fresh line instead.
                    let w_alone = text_w + fresh_pad(&style);
                    if cur_w + w <= max_w {
                        push_to_line(&mut cur_line, &text, &style);
                        cur_w += w;
                    } else if w_alone <= max_w && !cur_line.is_empty() {
                        // Trim trailing space on wrapped line.
                        trim_line_trailing_space(&mut cur_line);
                        if !cur_line.is_empty() {
                            raw_lines.push(std::mem::take(&mut cur_line));
                        }
                        keep_leading_space = false;
                        push_to_line(&mut cur_line, &text, &style);
                        cur_w = w_alone;
                    } else {
                        // Word itself exceeds `max_w`: split cleanly on grapheme boundaries.
                        if !cur_line.is_empty() {
                            trim_line_trailing_space(&mut cur_line);
                            if !cur_line.is_empty() {
                                raw_lines.push(std::mem::take(&mut cur_line));
                            }
                            cur_w = 0.0;
                        }
                        keep_leading_space = false;
                        let mut chunk = String::new();
                        let mut chunk_w = fresh_pad(&style);
                        for g in text.graphemes(true) {
                            let gw = measure_piece(g, &style, fonts);
                            if !chunk.is_empty() && chunk_w + gw > max_w {
                                push_to_line(&mut cur_line, &chunk, &style);
                                raw_lines.push(std::mem::take(&mut cur_line));
                                chunk.clear();
                                chunk_w = fresh_pad(&style);
                            }
                            chunk.push_str(g);
                            chunk_w += gw;
                        }
                        if !chunk.is_empty() {
                            push_to_line(&mut cur_line, &chunk, &style);
                            cur_w = chunk_w;
                        }
                    }
                }
            }
        }
        if !cur_line.is_empty() || raw_lines.is_empty() {
            trim_line_trailing_space(&mut cur_line);
            raw_lines.push(cur_line);
        }

        // Shape each coalesced line.
        let default_ascent = fonts.ascent(FontRole::Regular, base_font_size);
        let default_descent = fonts.descent(FontRole::Regular, base_font_size);
        let base_line_h = base_font_size * line_mult;

        raw_lines
            .into_iter()
            .map(|items| {
                let mut segments = Vec::with_capacity(items.len());
                let mut total_w = 0.0f32;
                let mut max_ascent = default_ascent;
                let mut max_descent = default_descent;

                for item in items {
                    match item {
                        LineItem::Text(text, style) => {
                            let role = FontRole::from_style(style.bold, style.italic, style.code);
                            let font_size = if style.code {
                                base_font_size * 0.88
                            } else {
                                base_font_size
                            };
                            let color = if style.link.is_some() {
                                self.palette.link
                            } else if style.code {
                                self.palette.inline_code_text
                            } else {
                                base_color
                            };
                            let runs = fonts.shape_text(&text, role, font_size);
                            let text_w: f32 = runs.iter().map(|r| r.width).sum();
                            let pad_x = if style.code { 2.2 } else { 0.0 };
                            let seg_w = text_w + 2.0 * pad_x;
                            let asc = fonts.ascent(role, font_size);
                            let desc = fonts.descent(role, font_size);
                            max_ascent = max_ascent.max(asc);
                            max_descent = max_descent.max(desc);
                            total_w += seg_w;

                            segments.push(StyledLineSegment {
                                style,
                                font_size,
                                color,
                                runs,
                                inline_image: None,
                                width: seg_w,
                                pad_x,
                                ascent: asc,
                                descent: desc,
                            });
                        }
                        LineItem::Image {
                            image,
                            w,
                            h,
                            alt,
                            style,
                        } => {
                            max_ascent = max_ascent.max(h);
                            total_w += w;
                            segments.push(StyledLineSegment {
                                style,
                                font_size: base_font_size,
                                color: base_color,
                                runs: Vec::new(),
                                inline_image: Some((image, w, h, alt)),
                                width: w,
                                pad_x: 0.0,
                                ascent: h,
                                descent: 0.0,
                            });
                        }
                    }
                }

                let height = base_line_h.max((max_ascent + max_descent) * 1.18);
                WrappedLine {
                    segments,
                    width: total_w,
                    ascent: max_ascent,
                    descent: max_descent,
                    height,
                }
            })
            .collect()
    }

    /// Wrap monospace lines while preserving leading indentation and whitespace.
    fn wrap_monospace_lines<'a>(
        &self,
        lines: impl Iterator<Item = &'a str>,
        max_width: f32,
        font_size: f32,
        line_h: f32,
        color: Rgb,
        fonts: &mut FontBook,
    ) -> Vec<WrappedLine> {
        let ascent = fonts.ascent(FontRole::MonoRegular, font_size);
        let descent = fonts.descent(FontRole::MonoRegular, font_size);
        let max_w = max_width.max(24.0);
        let mut out = Vec::new();

        for raw in lines {
            let expanded = raw.replace('\t', "    ");
            if expanded.is_empty() {
                out.push(WrappedLine {
                    segments: Vec::new(),
                    width: 0.0,
                    ascent,
                    descent,
                    height: line_h,
                });
                continue;
            }
            let full_runs = fonts.shape_text(&expanded, FontRole::MonoRegular, font_size);
            let full_w: f32 = full_runs.iter().map(|r| r.width).sum();
            if full_w <= max_w {
                out.push(WrappedLine {
                    segments: vec![StyledLineSegment {
                        style: InlineStyle::default(),
                        font_size,
                        color,
                        runs: full_runs,
                        inline_image: None,
                        width: full_w,
                        pad_x: 0.0,
                        ascent,
                        descent,
                    }],
                    width: full_w,
                    ascent,
                    descent,
                    height: line_h,
                });
                continue;
            }

            // Soft-wrap long code lines cleanly on grapheme boundaries.
            let mut chunk = String::new();
            let mut chunk_w = 0.0f32;
            for g in expanded.graphemes(true) {
                let gw = fonts.measure_width(g, FontRole::MonoRegular, font_size);
                if !chunk.is_empty() && chunk_w + gw > max_w {
                    let runs = fonts.shape_text(&chunk, FontRole::MonoRegular, font_size);
                    let w: f32 = runs.iter().map(|r| r.width).sum();
                    out.push(WrappedLine {
                        segments: vec![StyledLineSegment {
                            style: InlineStyle::default(),
                            font_size,
                            color,
                            runs,
                            inline_image: None,
                            width: w,
                            pad_x: 0.0,
                            ascent,
                            descent,
                        }],
                        width: w,
                        ascent,
                        descent,
                        height: line_h,
                    });
                    chunk.clear();
                    chunk_w = 0.0;
                }
                chunk.push_str(g);
                chunk_w += gw;
            }
            if !chunk.is_empty() {
                let runs = fonts.shape_text(&chunk, FontRole::MonoRegular, font_size);
                let w: f32 = runs.iter().map(|r| r.width).sum();
                out.push(WrappedLine {
                    segments: vec![StyledLineSegment {
                        style: InlineStyle::default(),
                        font_size,
                        color,
                        runs,
                        inline_image: None,
                        width: w,
                        pad_x: 0.0,
                        ascent,
                        descent,
                    }],
                    width: w,
                    ascent,
                    descent,
                    height: line_h,
                });
            }
        }
        out
    }

    fn emit_wrapped_line(
        &mut self,
        line: &WrappedLine,
        left: f32,
        avail_width: f32,
        align: TableAlign,
    ) {
        let x_offset = match align {
            TableAlign::Left => 0.0,
            TableAlign::Center => ((avail_width - line.width) * 0.5).max(0.0),
            TableAlign::Right => (avail_width - line.width).max(0.0),
        };
        let baseline_y =
            self.y + line.ascent + ((line.height - line.ascent - line.descent).max(0.0)) * 0.35;
        let mut x = left + x_offset;

        for seg in &line.segments {
            if let Some((image, img_w, img_h, alt)) = &seg.inline_image {
                let img_y = baseline_y - *img_h;
                self.push_cmd(DrawCmd::Image(DrawImage {
                    x,
                    y: img_y,
                    w: *img_w,
                    h: *img_h,
                    image: image.clone(),
                }));
                if let Some(target) = &seg.style.link {
                    self.push_link(PageLink {
                        x,
                        y: img_y,
                        w: *img_w,
                        h: *img_h,
                        target: target.clone(),
                        alt: alt.clone(),
                    });
                }
                x += seg.width;
                continue;
            }

            let seg_top = baseline_y - seg.ascent - 1.0;
            let seg_h = seg.ascent + seg.descent + 2.0;

            if seg.style.code {
                self.push_cmd(DrawCmd::Rect(DrawRect {
                    x,
                    y: seg_top,
                    w: seg.width,
                    h: seg_h,
                    radius: 2.5,
                    fill: Some(self.palette.inline_code_bg),
                    stroke: None,
                }));
            }
            if seg.style.highlight {
                self.push_cmd(DrawCmd::Rect(DrawRect {
                    x,
                    y: seg_top,
                    w: seg.width,
                    h: seg_h,
                    radius: 2.0,
                    fill: Some(self.palette.highlight_bg),
                    stroke: None,
                }));
            }

            let mut rx = x + seg.pad_x;
            for run in &seg.runs {
                let rw = run.width;
                self.push_cmd(DrawCmd::Text(DrawTextRun {
                    x: rx,
                    baseline_y,
                    font_size: seg.font_size,
                    color: seg.color,
                    run: run.clone(),
                }));
                rx += rw;
            }

            if seg.style.underline || seg.style.link.is_some() {
                let uy = baseline_y + seg.descent * 0.35;
                self.push_cmd(DrawCmd::Line(DrawLine {
                    x1: x + seg.pad_x,
                    y1: uy,
                    x2: x + seg.width - seg.pad_x,
                    y2: uy,
                    color: seg.color,
                    width: (seg.font_size * 0.055).clamp(0.45, 1.1),
                }));
            }

            if seg.style.strikethrough {
                let sy = baseline_y - seg.ascent * 0.34;
                self.push_cmd(DrawCmd::Line(DrawLine {
                    x1: x + seg.pad_x,
                    y1: sy,
                    x2: x + seg.width - seg.pad_x,
                    y2: sy,
                    color: seg.color,
                    width: (seg.font_size * 0.055).clamp(0.45, 1.1),
                }));
            }

            if let Some(target) = &seg.style.link {
                let alt: String = seg.runs.iter().map(|r| r.text.as_str()).collect();
                self.push_link(PageLink {
                    x,
                    y: seg_top,
                    w: seg.width,
                    h: seg_h,
                    target: target.clone(),
                    alt,
                });
            }

            x += seg.width;
        }
    }
}

fn fit_image_size(
    img: &PdfImage,
    pixel_width: u32,
    pixel_height: u32,
    max_w: f32,
    max_h: f32,
) -> (f32, f32) {
    let nat_w = (pixel_width as f32 * PX_TO_PT).max(1.0);
    let nat_h = (pixel_height as f32 * PX_TO_PT).max(1.0);
    let (mut draw_w, mut draw_h) = match (img.requested_width, img.requested_height) {
        (Some(rw), Some(rh)) => (rw as f32 * PX_TO_PT, rh as f32 * PX_TO_PT),
        (Some(rw), None) => {
            let w = rw as f32 * PX_TO_PT;
            (w, w * (nat_h / nat_w))
        }
        (None, Some(rh)) => {
            let h = rh as f32 * PX_TO_PT;
            (h * (nat_w / nat_h), h)
        }
        (None, None) => (nat_w, nat_h),
    };
    if draw_w > max_w {
        let s = max_w / draw_w;
        draw_w *= s;
        draw_h *= s;
    }
    if draw_h > max_h {
        let s = max_h / draw_h;
        draw_w *= s;
        draw_h *= s;
    }
    (draw_w.max(1.0), draw_h.max(1.0))
}

fn is_cjk_breakable(ch: char) -> bool {
    let cp = ch as u32;
    matches!(
        cp,
        0x3000..=0x30FF   // CJK Symbols, Hiragana, Katakana
        | 0x3400..=0x4DBF // CJK Extension A
        | 0x4E00..=0x9FFF // CJK Unified Ideographs
        | 0xAC00..=0xD7AF // Hangul Syllables
        | 0xF900..=0xFAFF // CJK Compatibility Ideographs
        | 0xFF00..=0xFFEF // Halfwidth and Fullwidth Forms
    )
}

fn table_row_height(cells: &[Vec<WrappedLine>], cell_pt: f32, pad_y: f32) -> f32 {
    let max_text_h = cells
        .iter()
        .map(|lines| lines.iter().map(|l| l.height).sum::<f32>())
        .fold(cell_pt * 1.38, f32::max);
    max_text_h + 2.0 * pad_y
}

fn distribute_column_widths(desired: &[f32], total_width: f32) -> Vec<f32> {
    let n = desired.len();
    if n == 0 {
        return Vec::new();
    }
    let min_col = (total_width / n as f32).min(42.0);
    let max_col = total_width * 0.65;
    let mut clamped: Vec<f32> = desired.iter().map(|&w| w.clamp(min_col, max_col)).collect();
    let sum: f32 = clamped.iter().sum();
    if sum <= 0.0 {
        return vec![total_width / n as f32; n];
    }
    let scale = total_width / sum;
    for w in &mut clamped {
        *w *= scale;
    }
    // Enforce minimum width floor after scaling, trimming widest columns if needed.
    let mut deficit = 0.0f32;
    for w in &mut clamped {
        if *w < min_col {
            deficit += min_col - *w;
            *w = min_col;
        }
    }
    if deficit > 0.0 {
        let slack_sum: f32 = clamped.iter().map(|&w| (w - min_col).max(0.0)).sum();
        if slack_sum > 0.0 {
            for w in &mut clamped {
                let slack = (*w - min_col).max(0.0);
                *w -= deficit * (slack / slack_sum);
            }
        }
    }
    clamped
}

#[cfg(test)]
mod tests {
    use super::super::model::build_document_model;
    use super::*;
    use crate::model::FileType;

    fn lay_out(md: &str) -> LaidOutDocument {
        let mut fonts = FontBook::new().unwrap();
        let doc = build_document_model(md, FileType::Markdown, None, None);
        layout_document(&doc, PdfExportSettings::default(), &mut fonts)
    }

    fn page_texts(page: &LayoutPage) -> Vec<(&str, f32)> {
        page.cmds
            .iter()
            .filter_map(|c| match c {
                DrawCmd::Text(t) => Some((t.run.text.as_str(), t.baseline_y)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn table_row_taller_than_a_page_is_split_across_pages() {
        let long_cell = (0..4000)
            .map(|i| format!("w{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let md = format!("| Head | Other |\n| --- | --- |\n| {long_cell} | x |\n| after | y |\n");
        let out = lay_out(&md);
        assert!(out.pages.len() >= 2, "row should span pages");
        for (p, page) in out.pages.iter().enumerate() {
            for (text, baseline) in page_texts(page) {
                assert!(
                    baseline <= out.page_h - out.margin + 1.0,
                    "page {p}: {text:?} drawn below the content area at {baseline}"
                );
            }
        }
        let all: String = out
            .pages
            .iter()
            .flat_map(|p| page_texts(p).into_iter().map(|(t, _)| t.to_string()))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            all.contains("w3999") && all.contains("after"),
            "content lost"
        );
        // The header repeats on the continuation page.
        assert!(page_texts(&out.pages[1])
            .iter()
            .any(|(t, _)| t.contains("Head")));
    }

    #[test]
    fn list_marker_stays_on_the_page_of_its_first_line() {
        for filler in 25..70 {
            let mut md = String::new();
            for i in 0..filler {
                md.push_str(&format!("Filler paragraph number {i}.\n\n"));
            }
            // A code block as the first block needs more room than one text line.
            md.push_str("- ```\n  bulletitem\n  ```\n");
            let out = lay_out(&md);
            for page in &out.pages {
                let texts = page_texts(page);
                if texts.iter().any(|(t, _)| t.contains('\u{2022}')) {
                    assert!(
                        texts.iter().any(|(t, _)| t.contains("bulletitem")),
                        "bullet orphaned with {filler} filler paragraphs"
                    );
                }
            }
        }
    }

    #[test]
    fn inline_code_wraps_by_drawn_width() {
        let words = (0..400).map(|_| "abc").collect::<Vec<_>>().join(" ");
        let mut fonts = FontBook::new().unwrap();
        let doc = build_document_model(&format!("`{words}`"), FileType::Markdown, None, None);
        let settings = PdfExportSettings::default();
        let out = layout_document(&doc, settings, &mut fonts);
        let max_w = out.page_w - 2.0 * out.margin;
        // Each drawn code background is one line's segment: it must fit, and
        // all but the last line should be close to full (no early wrapping).
        let widths: Vec<f32> = out.pages[0]
            .cmds
            .iter()
            .filter_map(|c| match c {
                DrawCmd::Rect(r) if r.fill == Some(out.palette.inline_code_bg) => Some(r.w),
                _ => None,
            })
            .collect();
        assert!(widths.len() > 2);
        let word_w = fonts.measure_width("abc ", FontRole::MonoRegular, out.body_pt * 0.88);
        for w in &widths[..widths.len() - 1] {
            assert!(*w <= max_w + 0.5, "code line overflows: {w} > {max_w}");
            assert!(
                *w > max_w - word_w - 1.0,
                "code line wrapped early: {w} of {max_w}"
            );
        }
    }

    #[test]
    fn code_block_lines_are_evenly_spaced_inside_the_padding() {
        let out = lay_out("```
first
second
third
```");
        let page = &out.pages[0];
        let box_top = page
            .cmds
            .iter()
            .find_map(|c| match c {
                DrawCmd::Rect(r) if r.fill == Some(out.palette.code_block_bg) => Some(r.y),
                _ => None,
            })
            .expect("code block box");
        let baselines: Vec<f32> = page_texts(page).iter().map(|(_, y)| *y).collect();
        assert_eq!(baselines.len(), 3);
        let gap1 = baselines[1] - baselines[0];
        let gap2 = baselines[2] - baselines[1];
        assert!((gap1 - gap2).abs() < 0.01, "uneven line gaps {gap1} vs {gap2}");
        // The first line's top (baseline minus ascent) sits below the 8pt padding.
        let font_size = (out.body_pt * 0.84).max(7.5);
        assert!(baselines[0] - box_top > 8.0 + font_size * 0.5, "first line hugs the box top");
    }
}
