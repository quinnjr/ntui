use std::collections::HashMap;

use taffy::prelude::*;

use crate::fiber::{FiberId, FiberKind, FiberTree, Rect};
use crate::props::{Dimension as NDim, FlexDirection as NFlex, TextWrap, ViewProps};
use crate::style::BorderStyle;
use crate::text::{truncate_line, wrap_text};

pub(crate) struct TextContext {
    content: String,
    wrap: TextWrap,
}

/// Rebuild the taffy tree from the fiber tree, solve, and write Rects back.
/// Component/Fragment/Provider fibers contribute no taffy node; their children
/// attach to the nearest ancestor host node.
pub(crate) fn compute_layout(tree: &mut FiberTree, width: u16, height: u16) {
    let Some(root) = tree.root else { return };
    let mut taffy: TaffyTree<TextContext> = TaffyTree::new();
    let mut pairs: Vec<(FiberId, NodeId)> = Vec::new();
    let kids = build_nodes(tree, root, &mut taffy, &mut pairs);
    let root_node = taffy
        .new_with_children(
            Style {
                size: Size {
                    width: percent(1.0),
                    height: percent(1.0),
                },
                ..Default::default()
            },
            &kids,
        )
        .unwrap();
    taffy
        .compute_layout_with_measure(
            root_node,
            Size {
                width: AvailableSpace::Definite(width as f32),
                height: AvailableSpace::Definite(height as f32),
            },
            |known, available, _node, ctx, _style| measure_text(known, available, ctx),
        )
        .unwrap();

    let mut abs: HashMap<NodeId, (f32, f32)> = HashMap::new();
    walk_abs(&taffy, root_node, (0.0, 0.0), &mut abs);
    for (fid, node) in pairs {
        let l = taffy.layout(node).unwrap();
        let (x, y) = abs[&node];
        let rect = Rect {
            x: x.round() as u16,
            y: y.round() as u16,
            width: l.size.width.round() as u16,
            height: l.size.height.round() as u16,
        };
        let fiber = tree.get_mut(fid);
        fiber.layout = rect;
        // Wrap/truncate once here at the final resolved width so `paint` (which
        // runs every frame, even when layout is cached) reuses these lines
        // instead of re-wrapping. Refilled each pass so stale lines from a
        // previous frame can never be painted.
        fiber.wrapped = match &fiber.kind {
            FiberKind::Text(props) => Some(match props.wrap {
                TextWrap::Wrap => wrap_text(&props.content, rect.width as usize),
                TextWrap::Truncate => {
                    vec![truncate_line(&props.content, rect.width as usize)]
                }
            }),
            _ => None,
        };
    }
    tree.layout_dirty = false;
}

fn build_nodes(
    tree: &FiberTree,
    id: FiberId,
    taffy: &mut TaffyTree<TextContext>,
    pairs: &mut Vec<(FiberId, NodeId)>,
) -> Vec<NodeId> {
    let fiber = tree.get(id);
    match &fiber.kind {
        FiberKind::View(props) => {
            let kids: Vec<NodeId> = fiber
                .children
                .iter()
                .flat_map(|c| build_nodes(tree, *c, taffy, pairs))
                .collect();
            let node = taffy.new_with_children(view_style(props), &kids).unwrap();
            pairs.push((id, node));
            vec![node]
        }
        FiberKind::Text(props) => {
            let node = taffy
                .new_leaf_with_context(
                    Style::default(),
                    TextContext {
                        content: props.content.clone(),
                        wrap: props.wrap,
                    },
                )
                .unwrap();
            pairs.push((id, node));
            vec![node]
        }
        _ => fiber
            .children
            .iter()
            .flat_map(|c| build_nodes(tree, *c, taffy, pairs))
            .collect(),
    }
}

fn view_style(p: &ViewProps) -> Style {
    Style {
        display: Display::Flex,
        flex_direction: match p.flex_direction {
            NFlex::Row => FlexDirection::Row,
            NFlex::Column => FlexDirection::Column,
        },
        flex_grow: p.flex_grow,
        gap: Size {
            width: length(p.gap as f32),
            height: length(p.gap as f32),
        },
        padding: taffy::Rect::length(p.padding as f32),
        margin: taffy::Rect::length(p.margin as f32),
        border: if p.border_style == BorderStyle::None {
            taffy::Rect::length(0.0)
        } else {
            taffy::Rect::length(1.0)
        },
        size: Size {
            width: dim(p.width),
            height: dim(p.height),
        },
        ..Default::default()
    }
}

fn dim(d: NDim) -> taffy::Dimension {
    match d {
        NDim::Auto => auto(),
        NDim::Cells(n) => length(n as f32),
        NDim::Percent(p) => percent(p / 100.0),
    }
}

fn measure_text(
    known: Size<Option<f32>>,
    available: Size<AvailableSpace>,
    ctx: Option<&mut TextContext>,
) -> Size<f32> {
    let Some(ctx) = ctx else { return Size::ZERO };
    let max_w = known.width.unwrap_or(match available.width {
        AvailableSpace::Definite(w) => w,
        _ => f32::INFINITY,
    });
    match ctx.wrap {
        TextWrap::Truncate => {
            let len = ctx.content.split('\n').next().unwrap_or("").chars().count() as f32;
            Size {
                width: known.width.unwrap_or(len.min(max_w)),
                height: known.height.unwrap_or(1.0),
            }
        }
        TextWrap::Wrap => {
            // f32 -> usize saturates in Rust, so INFINITY becomes usize::MAX.
            let lines = wrap_text(&ctx.content, max_w as usize);
            let w = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32;
            let h = lines.len().max(1) as f32;
            Size {
                width: known.width.unwrap_or(w),
                height: known.height.unwrap_or(h),
            }
        }
    }
}

fn walk_abs(
    taffy: &TaffyTree<TextContext>,
    node: NodeId,
    origin: (f32, f32),
    abs: &mut HashMap<NodeId, (f32, f32)>,
) {
    let l = taffy.layout(node).unwrap();
    let pos = (origin.0 + l.location.x, origin.1 + l.location.y);
    abs.insert(node, pos);
    for c in taffy.children(node).unwrap() {
        walk_abs(taffy, c, pos, abs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::Element;
    use crate::fiber::{FiberTree, Rect};
    use crate::hooks::RuntimeHandle;
    use crate::props::{Dimension, FlexDirection, TextProps, ViewProps};
    use crate::style::BorderStyle;

    fn text(s: &str) -> Element {
        Element::text(TextProps {
            content: s.into(),
            ..Default::default()
        })
    }

    #[test]
    fn column_stacks_texts() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        let root = tree.mount_root(
            Element::view(
                ViewProps {
                    flex_direction: FlexDirection::Column,
                    ..Default::default()
                },
                vec![text("one"), text("two")],
            ),
            &rt,
        );
        compute_layout(&mut tree, 20, 10);
        let kids = tree.get(root).children.clone();
        assert_eq!(
            tree.get(kids[0]).layout,
            Rect {
                x: 0,
                y: 0,
                width: 3,
                height: 1
            }
        );
        assert_eq!(
            tree.get(kids[1]).layout,
            Rect {
                x: 0,
                y: 1,
                width: 3,
                height: 1
            }
        );
        assert!(!tree.layout_dirty);
    }

    #[test]
    fn border_and_padding_offset_children() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        let root = tree.mount_root(
            Element::view(
                ViewProps {
                    padding: 1,
                    border_style: BorderStyle::Single,
                    width: Dimension::Cells(10),
                    height: Dimension::Cells(5),
                    ..Default::default()
                },
                vec![text("hi")],
            ),
            &rt,
        );
        compute_layout(&mut tree, 20, 10);
        let child = tree.get(root).children[0];
        assert_eq!(
            tree.get(child).layout,
            Rect {
                x: 2,
                y: 2,
                width: 2,
                height: 1
            }
        );
        assert_eq!(
            tree.get(root).layout,
            Rect {
                x: 0,
                y: 0,
                width: 10,
                height: 5
            }
        );
    }
}
