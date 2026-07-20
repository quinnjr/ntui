use crate::buffer::{Buffer, Cell};
use crate::fiber::{FiberId, FiberKind, FiberTree, Rect};
use crate::props::{GradientDirection, Overflow, TextWrap};
use crate::style::{Attrs, BorderStyle, Color, Weight};
use crate::text::{truncate_line, wrap_text};

pub(crate) fn paint(tree: &FiberTree, buf: &mut Buffer) {
    if let Some(root) = tree.root {
        let full = Rect {
            x: 0,
            y: 0,
            width: buf.width(),
            height: buf.height(),
        };
        let mut overlays = Vec::new();
        paint_fiber(tree, root, buf, full, (0, 0), &mut overlays, false);
        // Second pass: overlay views (`ViewProps::overlay`) paint last, over
        // everything the first pass just painted, unclipped and at the
        // root-anchored rect `compute_layout` gave them — regardless of
        // where they sit in the tree. A throwaway `overlays` list below
        // means an overlay nested inside another overlay is silently not
        // painted; see `Anchor`.
        for id in overlays {
            let mut inner_overlays = Vec::new();
            paint_fiber(tree, id, buf, full, (0, 0), &mut inner_overlays, true);
            debug_assert!(
                inner_overlays.is_empty(),
                "overlay nested inside overlay (fiber {id:?}) is not supported and was not painted"
            );
        }
    }
}

/// Overlap of two rectangles; a zero-size result means nothing is visible.
fn intersect(a: Rect, b: Rect) -> Rect {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = a.x.saturating_add(a.width).min(b.x.saturating_add(b.width));
    let y2 =
        a.y.saturating_add(a.height)
            .min(b.y.saturating_add(b.height));
    Rect {
        x: x1,
        y: y1,
        width: x2.saturating_sub(x1),
        height: y2.saturating_sub(y1),
    }
}

/// Write a cell only if it lands inside the clip region (and the buffer).
/// Coordinates are `i32` so content scrolled above/left of the origin is
/// simply discarded rather than wrapping around.
fn put(buf: &mut Buffer, clip: Rect, x: i32, y: i32, cell: Cell) {
    let (cx, cy) = (clip.x as i32, clip.y as i32);
    if x >= cx && y >= cy && x < cx + clip.width as i32 && y < cy + clip.height as i32 {
        buf.set(x as u16, y as u16, cell);
    }
}

/// The on-screen portion of a rect whose top-left may be negative (scrolled
/// above/left of the viewport), expressed as a `u16` clip rect.
fn onscreen_rect(ox: i32, oy: i32, w: u16, h: u16) -> Rect {
    let x0 = ox.max(0);
    let y0 = oy.max(0);
    let x1 = (ox + w as i32).max(0);
    let y1 = (oy + h as i32).max(0);
    Rect {
        x: x0.min(u16::MAX as i32) as u16,
        y: y0.min(u16::MAX as i32) as u16,
        width: (x1 - x0).min(u16::MAX as i32) as u16,
        height: (y1 - y0).min(u16::MAX as i32) as u16,
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_fiber(
    tree: &FiberTree,
    id: FiberId,
    buf: &mut Buffer,
    clip: Rect,
    offset: (i32, i32),
    overlays: &mut Vec<FiberId>,
    is_overlay_root: bool,
) {
    let fiber = tree.get(id);
    if !is_overlay_root
        && let FiberKind::View(props) = &fiber.kind
        && props.overlay.is_some()
    {
        overlays.push(id);
        return;
    }
    let r = fiber.layout;
    // This fiber's on-screen top-left, after ancestor scroll offsets.
    let (ox, oy) = (r.x as i32 + offset.0, r.y as i32 + offset.1);
    match &fiber.kind {
        FiberKind::View(props) => {
            if let Some((from, to, dir)) = props.background_gradient {
                for dy in 0..r.height as i32 {
                    for dx in 0..r.width as i32 {
                        let t = gradient_t(dx, dy, r.width, r.height, dir);
                        put(
                            buf,
                            clip,
                            ox + dx,
                            oy + dy,
                            Cell {
                                bg: Color::lerp(from, to, t),
                                ..Cell::default()
                            },
                        );
                    }
                }
            } else if props.background != Color::Reset {
                for dy in 0..r.height as i32 {
                    for dx in 0..r.width as i32 {
                        put(
                            buf,
                            clip,
                            ox + dx,
                            oy + dy,
                            Cell {
                                bg: props.background,
                                ..Cell::default()
                            },
                        );
                    }
                }
            }
            if props.border_style != BorderStyle::None && r.width >= 2 && r.height >= 2 {
                draw_border(
                    buf,
                    clip,
                    ox,
                    oy,
                    r.width,
                    r.height,
                    props.border_style,
                    props.border_color,
                    props.background,
                );
            }
        }
        FiberKind::Text(props) => {
            // Reuse the lines wrapped by `compute_layout` at the final resolved
            // width; fall back to wrapping here only if layout hasn't run.
            let fallback;
            let lines: &[String] = if let Some(cached) = &fiber.wrapped {
                cached
            } else {
                fallback = match props.wrap {
                    TextWrap::Wrap => wrap_text(&props.content, r.width as usize),
                    TextWrap::Truncate => vec![truncate_line(&props.content, r.width as usize)],
                };
                &fallback
            };
            let attrs = Attrs {
                bold: props.weight == Weight::Bold,
                ..Attrs::default()
            };
            for (dy, line) in lines.iter().take(r.height as usize).enumerate() {
                let line_len = props
                    .color_gradient
                    .is_some()
                    .then(|| line.chars().count().max(1));
                for (dx, ch) in line.chars().take(r.width as usize).enumerate() {
                    // Sanitize application-supplied control characters (ESC, BEL,
                    // C0/C1, DEL) to prevent terminal escape-sequence injection.
                    let ch = if ch.is_control() { ' ' } else { ch };
                    let (x, y) = (ox + dx as i32, oy + dy as i32);
                    // keep the background an ancestor View already painted
                    let bg = if x >= 0
                        && y >= 0
                        && (x as u16) < buf.width()
                        && (y as u16) < buf.height()
                    {
                        buf.get(x as u16, y as u16).bg
                    } else {
                        Color::Reset
                    };
                    let fg = if let Some((from, to)) = props.color_gradient {
                        let t = dx as f32 / (line_len.unwrap().saturating_sub(1).max(1)) as f32;
                        Color::lerp(from, to, t)
                    } else {
                        props.color
                    };
                    put(buf, clip, x, y, Cell { ch, fg, bg, attrs });
                }
            }
        }
        _ => {}
    }

    // A Scroll View shifts its descendants up by its offset; Clip/Scroll views
    // also confine descendants to their on-screen rect.
    let (child_clip, child_offset) = match &fiber.kind {
        FiberKind::View(props) => {
            let clip = if props.overflow != Overflow::Visible {
                intersect(clip, onscreen_rect(ox, oy, r.width, r.height))
            } else {
                clip
            };
            let scroll_dy = props
                .scroll
                .as_ref()
                .map(|s| s.offset() as i32)
                .unwrap_or(0);
            (clip, (offset.0, offset.1 - scroll_dy))
        }
        _ => (clip, offset),
    };
    for c in &fiber.children {
        paint_fiber(tree, *c, buf, child_clip, child_offset, overlays, false);
    }
}

/// Interpolation factor `[0.0, 1.0]` for the cell at `(dx, dy)` within a
/// `w`×`h` box, along the given gradient axis.
fn gradient_t(dx: i32, dy: i32, w: u16, h: u16, dir: GradientDirection) -> f32 {
    match dir {
        GradientDirection::Horizontal => dx as f32 / (w.saturating_sub(1).max(1)) as f32,
        GradientDirection::Vertical => dy as f32 / (h.saturating_sub(1).max(1)) as f32,
    }
}

/// [horizontal, vertical, top-left, top-right, bottom-left, bottom-right]
fn border_chars(style: BorderStyle) -> [char; 6] {
    match style {
        BorderStyle::Single => ['─', '│', '┌', '┐', '└', '┘'],
        BorderStyle::Round => ['─', '│', '╭', '╮', '╰', '╯'],
        BorderStyle::Double => ['═', '║', '╔', '╗', '╚', '╝'],
        BorderStyle::None => [' '; 6],
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_border(
    buf: &mut Buffer,
    clip: Rect,
    ox: i32,
    oy: i32,
    w: u16,
    h: u16,
    style: BorderStyle,
    color: Color,
    bg: Color,
) {
    let [hc, vc, tl, tr, bl, br] = border_chars(style);
    let (x2, y2) = (ox + w as i32 - 1, oy + h as i32 - 1);
    let cell = |ch| Cell {
        ch,
        fg: color,
        bg,
        attrs: Attrs::default(),
    };
    for dx in 1..w as i32 - 1 {
        put(buf, clip, ox + dx, oy, cell(hc));
        put(buf, clip, ox + dx, y2, cell(hc));
    }
    for dy in 1..h as i32 - 1 {
        put(buf, clip, ox, oy + dy, cell(vc));
        put(buf, clip, x2, oy + dy, cell(vc));
    }
    put(buf, clip, ox, oy, cell(tl));
    put(buf, clip, x2, oy, cell(tr));
    put(buf, clip, ox, y2, cell(bl));
    put(buf, clip, x2, y2, cell(br));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Buffer;
    use crate::element::Element;
    use crate::fiber::FiberTree;
    use crate::hooks::RuntimeHandle;
    use crate::layout::compute_layout;
    use crate::props::{
        Dimension, FlexDirection, GradientDirection, JustifyContent, Overflow, TextProps, TextWrap,
        ViewProps,
    };
    use crate::style::{BorderStyle, Color, Weight};

    fn render_to_text(el: Element, w: u16, h: u16) -> String {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        tree.mount_root(el, &rt);
        compute_layout(&mut tree, w, h);
        let mut buf = Buffer::new(w, h);
        paint(&tree, &mut buf);
        buf.to_text()
    }

    #[test]
    fn bordered_box_with_text() {
        let out = render_to_text(
            Element::view(
                ViewProps {
                    border_style: BorderStyle::Round,
                    width: Dimension::Cells(8),
                    height: Dimension::Cells(3),
                    ..Default::default()
                },
                vec![Element::text(TextProps {
                    content: "hi".into(),
                    ..Default::default()
                })],
            ),
            8,
            3,
        );
        assert_eq!(out, "╭──────╮\n│hi    │\n╰──────╯");
    }

    #[test]
    fn column_with_wrap() {
        let out = render_to_text(
            Element::view(
                ViewProps {
                    flex_direction: FlexDirection::Column,
                    width: Dimension::Cells(5),
                    height: Dimension::Cells(3),
                    ..Default::default()
                },
                vec![Element::text(TextProps {
                    content: "one two".into(),
                    ..Default::default()
                })],
            ),
            5,
            3,
        );
        assert_eq!(out, "one  \ntwo  \n     ");
    }

    #[test]
    fn text_styles_land_in_cells() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        tree.mount_root(
            Element::text(TextProps {
                content: "x".into(),
                color: Color::Yellow,
                weight: Weight::Bold,
                wrap: TextWrap::Truncate,
                ..Default::default()
            }),
            &rt,
        );
        compute_layout(&mut tree, 3, 1);
        let mut buf = Buffer::new(3, 1);
        paint(&tree, &mut buf);
        let cell = buf.get(0, 0);
        assert_eq!(cell.fg, Color::Yellow);
        assert!(cell.attrs.bold);
    }

    #[test]
    fn view_background_fills_cells() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        tree.mount_root(
            Element::view(
                ViewProps {
                    background: Color::Blue,
                    width: Dimension::Cells(3),
                    height: Dimension::Cells(3),
                    ..Default::default()
                },
                vec![],
            ),
            &rt,
        );
        compute_layout(&mut tree, 3, 3);
        let mut buf = Buffer::new(3, 3);
        paint(&tree, &mut buf);
        assert_eq!(buf.get(1, 1).bg, Color::Blue);
    }

    #[test]
    fn text_control_chars_are_sanitized() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        tree.mount_root(
            Element::text(TextProps {
                content: "\x1b]52;c;evil\x07hi".into(),
                ..Default::default()
            }),
            &rt,
        );
        compute_layout(&mut tree, 20, 3);
        let mut buf = Buffer::new(20, 3);
        paint(&tree, &mut buf);
        // No painted cell may carry a control character.
        for y in 0..buf.height() {
            for x in 0..buf.width() {
                assert!(
                    !buf.get(x, y).ch.is_control(),
                    "control char leaked at ({x},{y})"
                );
            }
        }
        // The visible payload still survives.
        let out = buf.to_text();
        assert!(out.contains("hi"), "visible text missing: {out:?}");
    }

    #[test]
    fn border_color_lands_on_border_cells() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        tree.mount_root(
            Element::view(
                ViewProps {
                    border_style: BorderStyle::Single,
                    border_color: Color::Red,
                    width: Dimension::Cells(3),
                    height: Dimension::Cells(3),
                    ..Default::default()
                },
                vec![],
            ),
            &rt,
        );
        compute_layout(&mut tree, 3, 3);
        let mut buf = Buffer::new(3, 3);
        paint(&tree, &mut buf);
        let cell = buf.get(0, 0);
        assert_eq!(cell.fg, Color::Red);
        assert_eq!(cell.ch, '┌');
    }

    #[test]
    fn justify_content_end_bottom_aligns() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        tree.mount_root(
            Element::view(
                ViewProps {
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::End,
                    width: Dimension::Cells(3),
                    height: Dimension::Cells(4),
                    ..Default::default()
                },
                vec![Element::text(TextProps {
                    content: "x".into(),
                    ..Default::default()
                })],
            ),
            &rt,
        );
        compute_layout(&mut tree, 3, 4);
        let mut buf = Buffer::new(3, 4);
        paint(&tree, &mut buf);
        assert_eq!(buf.get(0, 3).ch, 'x', "child should sit on the bottom row");
        assert_eq!(buf.get(0, 0).ch, ' ', "top row should be empty");
    }

    #[test]
    fn overflow_clip_hides_spill() {
        // "abcdefghij" wraps at width 4 to ["abcd", "efgh", "ij"]; the box is 2 tall,
        // so the third line only appears when overflow is Visible.
        fn render_at(overflow: Overflow) -> Buffer {
            let (rt, _rx) = RuntimeHandle::test_handle();
            let mut tree = FiberTree::new();
            tree.mount_root(
                Element::view(
                    ViewProps {
                        overflow,
                        flex_direction: FlexDirection::Column,
                        width: Dimension::Cells(4),
                        height: Dimension::Cells(2),
                        ..Default::default()
                    },
                    vec![Element::text(TextProps {
                        content: "abcdefghij".into(),
                        ..Default::default()
                    })],
                ),
                &rt,
            );
            compute_layout(&mut tree, 4, 4);
            let mut buf = Buffer::new(4, 4);
            paint(&tree, &mut buf);
            buf
        }
        assert_eq!(
            render_at(Overflow::Visible).get(0, 2).ch,
            'i',
            "spill painted"
        );
        let clipped = render_at(Overflow::Clip);
        assert_eq!(clipped.get(0, 2).ch, ' ', "spill clipped away");
        assert_eq!(clipped.get(0, 0).ch, 'a', "visible rows intact");
    }

    #[test]
    fn scroll_offset_and_bottom_follow() {
        use crate::hooks::scroll::Scroll;

        let (rt, _rx) = RuntimeHandle::test_handle();
        let scroll = Scroll::new(0, rt.wake.clone());

        // Five 1-row lines in a 2-tall scroll box (content 5, viewport 2).
        let render = |scroll: &Scroll| -> String {
            let (rt, _rx) = RuntimeHandle::test_handle();
            let mut tree = FiberTree::new();
            let rows: Vec<Element> = (0..5)
                .map(|i| {
                    Element::text(TextProps {
                        content: format!("row{i}"),
                        ..Default::default()
                    })
                })
                .collect();
            tree.mount_root(
                Element::view(
                    ViewProps {
                        overflow: Overflow::Scroll,
                        scroll: Some(scroll.clone()),
                        flex_direction: FlexDirection::Column,
                        width: Dimension::Cells(4),
                        height: Dimension::Cells(2),
                        ..Default::default()
                    },
                    rows,
                ),
                &rt,
            );
            compute_layout(&mut tree, 4, 2);
            let mut buf = Buffer::new(4, 2);
            paint(&tree, &mut buf);
            buf.to_text()
        };

        // First layout starts pinned to the bottom (chat-follow): last two rows.
        assert_eq!(render(&scroll), "row3\nrow4");
        assert_eq!(scroll.max_offset(), 3);
        assert!(scroll.at_bottom());

        // Scrolling to the top reveals the first two rows.
        scroll.to_top();
        assert_eq!(render(&scroll), "row0\nrow1");

        // A middle offset shows the interior window.
        scroll.scroll_to(1);
        assert_eq!(render(&scroll), "row1\nrow2");
    }

    #[test]
    fn background_gradient_interpolates_across_the_box() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        tree.mount_root(
            Element::view(
                ViewProps {
                    background_gradient: Some((
                        Color::Rgb(0, 0, 0),
                        Color::Rgb(255, 0, 0),
                        GradientDirection::Horizontal,
                    )),
                    width: Dimension::Cells(3),
                    height: Dimension::Cells(1),
                    ..Default::default()
                },
                vec![],
            ),
            &rt,
        );
        compute_layout(&mut tree, 3, 1);
        let mut buf = Buffer::new(3, 1);
        paint(&tree, &mut buf);
        assert_eq!(buf.get(0, 0).bg, Color::Rgb(0, 0, 0));
        assert_eq!(buf.get(1, 0).bg, Color::Rgb(128, 0, 0));
        assert_eq!(buf.get(2, 0).bg, Color::Rgb(255, 0, 0));
    }

    #[test]
    fn background_gradient_interpolates_top_to_bottom_for_vertical() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        tree.mount_root(
            Element::view(
                ViewProps {
                    background_gradient: Some((
                        Color::Rgb(0, 0, 0),
                        Color::Rgb(255, 255, 255),
                        GradientDirection::Vertical,
                    )),
                    width: Dimension::Cells(3),
                    height: Dimension::Cells(10),
                    ..Default::default()
                },
                vec![],
            ),
            &rt,
        );
        compute_layout(&mut tree, 3, 10);
        let mut buf = Buffer::new(3, 10);
        paint(&tree, &mut buf);

        // Top and bottom rows differ (interpolation runs vertically)...
        assert_eq!(buf.get(0, 0).bg, Color::Rgb(0, 0, 0));
        assert_eq!(buf.get(0, 9).bg, Color::Rgb(255, 255, 255));
        assert_ne!(buf.get(0, 0).bg, buf.get(0, 9).bg);

        // ...while columns within the same row all match (no horizontal drift).
        for y in 0..10u16 {
            let row_color = buf.get(0, y).bg;
            assert_eq!(buf.get(1, y).bg, row_color, "row {y} col 1 mismatch");
            assert_eq!(buf.get(2, y).bg, row_color, "row {y} col 2 mismatch");
        }
    }

    #[test]
    fn text_color_gradient_interpolates_across_the_line() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        tree.mount_root(
            Element::text(TextProps {
                content: "abc".into(),
                color_gradient: Some((Color::Rgb(0, 0, 0), Color::Rgb(0, 0, 255))),
                wrap: TextWrap::Truncate,
                ..Default::default()
            }),
            &rt,
        );
        compute_layout(&mut tree, 3, 1);
        let mut buf = Buffer::new(3, 1);
        paint(&tree, &mut buf);
        assert_eq!(buf.get(0, 0).fg, Color::Rgb(0, 0, 0));
        assert_eq!(buf.get(1, 0).fg, Color::Rgb(0, 0, 128));
        assert_eq!(buf.get(2, 0).fg, Color::Rgb(0, 0, 255));
    }

    #[test]
    fn double_border_no_root_and_text_past_buffer() {
        // Painting an empty tree is a no-op.
        let mut empty = Buffer::new(3, 1);
        paint(&FiberTree::new(), &mut empty);

        let out = render_to_text(
            Element::view(
                ViewProps {
                    border_style: BorderStyle::Double,
                    width: Dimension::Cells(4),
                    height: Dimension::Cells(3),
                    ..Default::default()
                },
                vec![],
            ),
            4,
            3,
        );
        assert!(out.contains('\u{2554}'), "double corner");

        // Text laid out wider than the buffer exercises the out-of-buffer bg read.
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        tree.mount_root(
            Element::text(TextProps {
                content: "abcdef".into(),
                ..Default::default()
            }),
            &rt,
        );
        compute_layout(&mut tree, 6, 1);
        let mut small = Buffer::new(2, 1);
        paint(&tree, &mut small);
        assert_eq!(small.get(0, 0).ch, 'a');
    }

    #[test]
    fn overlay_layer_paints_after_a_later_sibling_that_would_otherwise_cover_it() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        tree.mount_root(
            Element::fragment(vec![
                // Comes first in tree order; a naive single-pass, depth-first
                // paint would let the later Green sibling below overwrite it.
                Element::view(
                    ViewProps {
                        width: Dimension::Cells(1),
                        height: Dimension::Cells(1),
                        ..Default::default()
                    },
                    vec![Element::view(
                        ViewProps {
                            overlay: Some(crate::props::Anchor::Center),
                            width: Dimension::Cells(4),
                            height: Dimension::Cells(4),
                            background: Color::Red,
                            ..Default::default()
                        },
                        vec![],
                    )],
                ),
                // Comes after the overlay in tree order and covers the whole
                // viewport.
                Element::view(
                    ViewProps {
                        width: Dimension::Cells(10),
                        height: Dimension::Cells(10),
                        background: Color::Green,
                        ..Default::default()
                    },
                    vec![],
                ),
            ]),
            &rt,
        );
        compute_layout(&mut tree, 10, 10);
        let mut buf = Buffer::new(10, 10);
        paint(&tree, &mut buf);
        // 4x4 centered in a 10x10 viewport sits at (3,3)..(7,7). The first
        // (1x1) child sits at (0,0) in the row layout, so check a cell that's
        // unambiguously part of the Green sibling instead.
        assert_eq!(buf.get(4, 4).bg, Color::Red, "overlay should paint on top");
        assert_eq!(
            buf.get(9, 9).bg,
            Color::Green,
            "outside it, the later sibling still shows"
        );
    }
}
