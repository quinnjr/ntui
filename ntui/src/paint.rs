use crate::buffer::{Buffer, Cell};
use crate::fiber::{FiberId, FiberKind, FiberTree, Rect};
use crate::props::{Overflow, TextWrap};
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
        paint_fiber(tree, root, buf, full);
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
fn put(buf: &mut Buffer, clip: Rect, x: u16, y: u16, cell: Cell) {
    if x >= clip.x
        && y >= clip.y
        && x < clip.x.saturating_add(clip.width)
        && y < clip.y.saturating_add(clip.height)
    {
        buf.set(x, y, cell);
    }
}

fn paint_fiber(tree: &FiberTree, id: FiberId, buf: &mut Buffer, clip: Rect) {
    let fiber = tree.get(id);
    match &fiber.kind {
        FiberKind::View(props) => {
            let r = fiber.layout;
            if props.background != Color::Reset {
                for y in r.y..r.y.saturating_add(r.height) {
                    for x in r.x..r.x.saturating_add(r.width) {
                        put(
                            buf,
                            clip,
                            x,
                            y,
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
                    r,
                    props.border_style,
                    props.border_color,
                    props.background,
                );
            }
        }
        FiberKind::Text(props) => {
            let r = fiber.layout;
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
                for (dx, ch) in line.chars().take(r.width as usize).enumerate() {
                    // Sanitize application-supplied control characters (ESC, BEL,
                    // C0/C1, DEL) to prevent terminal escape-sequence injection.
                    let ch = if ch.is_control() { ' ' } else { ch };
                    let (x, y) = (r.x.saturating_add(dx as u16), r.y.saturating_add(dy as u16));
                    // keep the background an ancestor View already painted
                    let bg = if x < buf.width() && y < buf.height() {
                        buf.get(x, y).bg
                    } else {
                        Color::Reset
                    };
                    put(
                        buf,
                        clip,
                        x,
                        y,
                        Cell {
                            ch,
                            fg: props.color,
                            bg,
                            attrs,
                        },
                    );
                }
            }
        }
        _ => {}
    }

    // A Clip/Scroll View confines its descendants to its own box.
    let child_clip = match &fiber.kind {
        FiberKind::View(props) if props.overflow != Overflow::Visible => {
            intersect(clip, fiber.layout)
        }
        _ => clip,
    };
    for c in &fiber.children {
        paint_fiber(tree, *c, buf, child_clip);
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

fn draw_border(buf: &mut Buffer, clip: Rect, r: Rect, style: BorderStyle, color: Color, bg: Color) {
    let [h, v, tl, tr, bl, br] = border_chars(style);
    let (x2, y2) = (
        r.x.saturating_add(r.width - 1),
        r.y.saturating_add(r.height - 1),
    );
    let cell = |ch| Cell {
        ch,
        fg: color,
        bg,
        attrs: Attrs::default(),
    };
    for x in r.x + 1..x2 {
        put(buf, clip, x, r.y, cell(h));
        put(buf, clip, x, y2, cell(h));
    }
    for y in r.y + 1..y2 {
        put(buf, clip, r.x, y, cell(v));
        put(buf, clip, x2, y, cell(v));
    }
    put(buf, clip, r.x, r.y, cell(tl));
    put(buf, clip, x2, r.y, cell(tr));
    put(buf, clip, r.x, y2, cell(bl));
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
        Dimension, FlexDirection, JustifyContent, Overflow, TextProps, TextWrap, ViewProps,
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
}
