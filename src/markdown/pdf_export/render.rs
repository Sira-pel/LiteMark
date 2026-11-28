//! Renders a [`LaidOutDocument`] into PDF bytes using `krilla`.

use super::fonts::{FontBook, FontRole};
use super::layout::{DrawCmd, HeadingAnchor, LaidOutDocument, Rgb};
use super::model::LinkTarget;
use crate::model::PdfExportSettings;
use krilla::action::{Action, LinkAction};
use krilla::annotation::{Annotation, LinkAnnotation, Target};
use krilla::color::rgb;
use krilla::destination::{Destination, XyzDestination};
use krilla::geom::{PathBuilder, Point, Rect, Size, Transform};
use krilla::metadata::Metadata;
use krilla::num::NormalizedF32;
use krilla::outline::{Outline, OutlineNode};
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule, LineCap, LineJoin, Stroke};
use krilla::surface::Surface;
use krilla::Document;
use std::collections::HashMap;

pub(crate) fn render_pdf_bytes(
    title: &str,
    laid_out: &LaidOutDocument,
    settings: PdfExportSettings,
    fonts: &mut FontBook,
) -> Result<Vec<u8>, String> {
    let mut doc = Document::new();
    doc.set_metadata(
        Metadata::new()
            .title(title.to_string())
            .creator("LiteMark".to_string()),
    );

    let total_pages = laid_out.pages.len();
    let page_settings = PageSettings::from_wh(laid_out.page_w, laid_out.page_h)
        .ok_or_else(|| "Invalid PDF page dimensions.".to_string())?;

    // Map heading slugs -> XYZ destination for internal `#heading` anchor links.
    let mut slug_dests: HashMap<String, XyzDestination> = HashMap::new();
    for anchor in &laid_out.headings {
        if !anchor.slug.is_empty() && anchor.page_idx < total_pages {
            slug_dests.entry(anchor.slug.clone()).or_insert_with(|| {
                XyzDestination::new(
                    anchor.page_idx,
                    Point::from_xy(laid_out.margin, anchor.y_pt),
                )
            });
        }
    }

    let footer_pt = (laid_out.body_pt * 0.82).clamp(7.5, 10.5);
    let footer_ascent = fonts.ascent(FontRole::Regular, footer_pt);

    for (page_idx, layout_page) in laid_out.pages.iter().enumerate() {
        let mut page = doc.start_page_with(page_settings.clone());

        // Attach clickable external & internal link annotations.
        for link in &layout_page.links {
            let Some(rect) = Rect::from_xywh(link.x, link.y, link.w.max(0.5), link.h.max(0.5))
            else {
                continue;
            };
            let target = match &link.target {
                LinkTarget::Uri(uri) => {
                    Some(Target::Action(Action::Link(LinkAction::new(uri.clone()))))
                }
                LinkTarget::InternalHeading(slug) => slug_dests
                    .get(slug)
                    .cloned()
                    .map(|dest| Target::Destination(Destination::Xyz(dest))),
            };
            if let Some(target) = target {
                let annot = Annotation::new_link(
                    LinkAnnotation::new(rect, target),
                    (!link.alt.is_empty()).then(|| link.alt.clone()),
                );
                page.add_annotation(annot);
            }
        }

        let mut surface = page.surface();

        // Full-page background fill for Sepia / Dark themes.
        if let Some(bg) = laid_out.palette.page_bg {
            draw_filled_rect(
                &mut surface,
                0.0,
                0.0,
                laid_out.page_w,
                laid_out.page_h,
                0.0,
                bg,
            );
        }

        for cmd in &layout_page.cmds {
            match cmd {
                DrawCmd::Rect(r) => {
                    if let Some(fill) = r.fill {
                        draw_filled_rect(&mut surface, r.x, r.y, r.w, r.h, r.radius, fill);
                    }
                    if let Some((color, width)) = r.stroke {
                        draw_stroked_rect(&mut surface, r.x, r.y, r.w, r.h, r.radius, color, width);
                    }
                }
                DrawCmd::Line(l) => {
                    draw_line(&mut surface, l.x1, l.y1, l.x2, l.y2, l.color, l.width);
                }
                DrawCmd::Text(t) => {
                    if t.run.glyphs.is_empty() {
                        continue;
                    }
                    let font = fonts.face(t.run.face_idx).krilla_font.clone();
                    surface.set_stroke(None);
                    surface.set_fill(Some(solid_fill(t.color)));
                    surface.draw_glyphs(
                        Point::from_xy(t.x, t.baseline_y),
                        &t.run.glyphs,
                        font,
                        &t.run.text,
                        t.font_size,
                        false,
                    );
                }
                DrawCmd::Image(img) => {
                    if let Some(size) = Size::from_wh(img.w, img.h) {
                        surface.push_transform(&Transform::from_translate(img.x, img.y));
                        surface.draw_image(img.image.clone(), size);
                        surface.pop();
                    }
                }
                DrawCmd::Checkbox(cb) => {
                    if cb.checked {
                        draw_filled_rect(
                            &mut surface,
                            cb.x,
                            cb.y,
                            cb.size,
                            cb.size,
                            1.8,
                            cb.checked_bg,
                        );
                        // Crisp vector checkmark inside the box.
                        let mut pb = PathBuilder::new();
                        pb.move_to(cb.x + cb.size * 0.22, cb.y + cb.size * 0.52);
                        pb.line_to(cb.x + cb.size * 0.43, cb.y + cb.size * 0.73);
                        pb.line_to(cb.x + cb.size * 0.78, cb.y + cb.size * 0.28);
                        if let Some(path) = pb.finish() {
                            surface.set_fill(None);
                            surface.set_stroke(Some(Stroke {
                                paint: to_rgb(cb.check_color).into(),
                                width: (cb.size * 0.14).max(0.9),
                                line_cap: LineCap::Round,
                                line_join: LineJoin::Round,
                                ..Default::default()
                            }));
                            surface.draw_path(&path);
                        }
                    } else {
                        draw_stroked_rect(
                            &mut surface,
                            cb.x,
                            cb.y,
                            cb.size,
                            cb.size,
                            1.8,
                            cb.border,
                            0.85,
                        );
                    }
                }
            }
        }

        // Subtle centered page number footer.
        if settings.page_numbers {
            let label = format!("{} / {}", page_idx + 1, total_pages);
            let runs = fonts.shape_text(&label, FontRole::Regular, footer_pt);
            let label_w: f32 = runs.iter().map(|r| r.width).sum();
            let fx = (laid_out.page_w - label_w) * 0.5;
            let footer_margin = laid_out.margin.max(36.0);
            let fy = laid_out.page_h - footer_margin * 0.5 + footer_ascent * 0.35;
            let mut x = fx;
            surface.set_stroke(None);
            surface.set_fill(Some(solid_fill(laid_out.palette.muted)));
            for run in runs {
                let rw = run.width;
                if !run.glyphs.is_empty() {
                    let font = fonts.face(run.face_idx).krilla_font.clone();
                    surface.draw_glyphs(
                        Point::from_xy(x, fy),
                        &run.glyphs,
                        font,
                        &run.text,
                        footer_pt,
                        false,
                    );
                }
                x += rw;
            }
        }

        surface.finish();
        page.finish();
    }

    // Build nested PDF Document Outline (viewer Table of Contents) from headings.
    if !laid_out.headings.is_empty() {
        let outline = build_pdf_outline(&laid_out.headings, total_pages, laid_out.margin);
        doc.set_outline(outline);
    }

    doc.finish()
        .map_err(|err| format!("Couldn't build PDF: {err:?}"))
}

fn build_pdf_outline(headings: &[HeadingAnchor], total_pages: usize, margin: f32) -> Outline {
    // Stack of `(level, OutlineNode)` to construct a proper nested tree where
    // H2 attaches under H1, H3 under H2, etc.
    let mut stack: Vec<(u8, OutlineNode)> = Vec::new();
    let mut roots: Vec<OutlineNode> = Vec::new();

    let collapse =
        |target_level: u8, stack: &mut Vec<(u8, OutlineNode)>, roots: &mut Vec<OutlineNode>| {
            while stack.last().is_some_and(|(lvl, _)| *lvl >= target_level) {
                let (_, finished) = stack.pop().unwrap();
                if let Some((_, parent)) = stack.last_mut() {
                    parent.push_child(finished);
                } else {
                    roots.push(finished);
                }
            }
        };

    for h in headings {
        if h.page_idx >= total_pages {
            continue;
        }
        collapse(h.level, &mut stack, &mut roots);
        let dest = XyzDestination::new(h.page_idx, Point::from_xy(margin, h.y_pt));
        let node = OutlineNode::new(h.title.clone(), dest).with_open(h.level <= 2);
        stack.push((h.level, node));
    }
    collapse(0, &mut stack, &mut roots);

    let mut outline = Outline::new();
    for node in roots {
        outline.push_child(node);
    }
    outline
}

fn to_rgb(c: Rgb) -> rgb::Color {
    rgb::Color::new(c.0, c.1, c.2)
}

fn solid_fill(c: Rgb) -> Fill {
    Fill {
        paint: to_rgb(c).into(),
        opacity: NormalizedF32::ONE,
        rule: FillRule::NonZero,
    }
}

fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<krilla::geom::Path> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let mut pb = PathBuilder::new();
    let r = radius.min(w * 0.5).min(h * 0.5);
    if r <= 0.05 {
        let rect = Rect::from_xywh(x, y, w, h)?;
        pb.push_rect(rect);
        return pb.finish();
    }
    // Cubic Bezier approximation for quarter circle (`k = 4/3 * (sqrt(2) - 1)`).
    const K: f32 = 0.552_284_8;
    let c = r * K;
    let x2 = x + w;
    let y2 = y + h;

    pb.move_to(x + r, y);
    pb.line_to(x2 - r, y);
    pb.cubic_to(x2 - r + c, y, x2, y + r - c, x2, y + r);
    pb.line_to(x2, y2 - r);
    pb.cubic_to(x2, y2 - r + c, x2 - r + c, y2, x2 - r, y2);
    pb.line_to(x + r, y2);
    pb.cubic_to(x + r - c, y2, x, y2 - r + c, x, y2 - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - c, x + r - c, y, x + r, y);
    pb.close();
    pb.finish()
}

fn draw_filled_rect(
    surface: &mut Surface<'_>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    color: Rgb,
) {
    if let Some(path) = rounded_rect_path(x, y, w, h, radius) {
        surface.set_stroke(None);
        surface.set_fill(Some(solid_fill(color)));
        surface.draw_path(&path);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_stroked_rect(
    surface: &mut Surface<'_>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    color: Rgb,
    width: f32,
) {
    if let Some(path) = rounded_rect_path(x, y, w, h, radius) {
        surface.set_fill(None);
        surface.set_stroke(Some(Stroke {
            paint: to_rgb(color).into(),
            width,
            ..Default::default()
        }));
        surface.draw_path(&path);
    }
}

fn draw_line(
    surface: &mut Surface<'_>,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    color: Rgb,
    width: f32,
) {
    let mut pb = PathBuilder::new();
    pb.move_to(x1, y1);
    pb.line_to(x2, y2);
    if let Some(path) = pb.finish() {
        surface.set_fill(None);
        surface.set_stroke(Some(Stroke {
            paint: to_rgb(color).into(),
            width,
            ..Default::default()
        }));
        surface.draw_path(&path);
    }
}
