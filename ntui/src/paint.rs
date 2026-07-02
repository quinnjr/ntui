#![allow(dead_code)]

use crate::buffer::{Buffer, Cell};
use crate::fiber::{FiberId, FiberKind, FiberTree, Rect};
use crate::props::TextWrap;
use crate::style::{Attrs, BorderStyle, Color, Weight};
use crate::text::{truncate_line, wrap_text};

pub(crate) fn paint(tree: &FiberTree, buf: &mut Buffer) {
    if let Some(root) = tree.root {
        paint_fiber(tree, root, buf);
    }
}

fn paint_fiber(tree: &FiberTree, id: FiberId, buf: &mut Buffer) {
    let fiber = tree.get(id);
    match &fiber.kind {
        FiberKind::View(props) => {
            let r = fiber.layout;
            if props.background != Color::Reset {
                for y in r.y..r.y.saturating_add(r.height) {
                    for x in r.x..r.x.saturating_add(r.width) {
                        buf.set(
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
                    r,
                    props.border_style,
                    props.border_color,
                    props.background,
                );
            }
        }
        FiberKind::Text(props) => {
            let r = fiber.layout;
            let lines = match props.wrap {
                TextWrap::Wrap => wrap_text(&props.content, r.width as usize),
                TextWrap::Truncate => vec![truncate_line(&props.content, r.width as usize)],
            };
            let attrs = Attrs {
                bold: props.weight == Weight::Bold,
                ..Attrs::default()
            };
            for (dy, line) in lines.iter().take(r.height as usize).enumerate() {
                for (dx, ch) in line.chars().take(r.width as usize).enumerate() {
                    let (x, y) = (r.x.saturating_add(dx as u16), r.y.saturating_add(dy as u16));
                    // keep the background an ancestor View already painted
                    let bg = if x < buf.width() && y < buf.height() {
                        buf.get(x, y).bg
                    } else {
                        Color::Reset
                    };
                    buf.set(
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
    for c in &fiber.children {
        paint_fiber(tree, *c, buf);
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

fn draw_border(buf: &mut Buffer, r: Rect, style: BorderStyle, color: Color, bg: Color) {
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
        buf.set(x, r.y, cell(h));
        buf.set(x, y2, cell(h));
    }
    for y in r.y + 1..y2 {
        buf.set(r.x, y, cell(v));
        buf.set(x2, y, cell(v));
    }
    buf.set(r.x, r.y, cell(tl));
    buf.set(x2, r.y, cell(tr));
    buf.set(r.x, y2, cell(bl));
    buf.set(x2, y2, cell(br));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Buffer;
    use crate::element::Element;
    use crate::fiber::FiberTree;
    use crate::hooks::RuntimeHandle;
    use crate::layout::compute_layout;
    use crate::props::{Dimension, FlexDirection, TextProps, TextWrap, ViewProps};
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
}
