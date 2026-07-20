use std::collections::HashMap;

use taffy::prelude::*;

use crate::fiber::{FiberId, FiberKind, FiberTree, Rect};
use crate::props::{
    AlignItems as NAlign, Anchor, Dimension as NDim, FlexDirection as NFlex,
    JustifyContent as NJustify, Overflow as NOverflow, TextWrap, ViewProps,
};
use crate::style::BorderStyle;
use crate::text::{truncate_line, wrap_text};

pub(crate) struct TextContext {
    content: String,
    wrap: TextWrap,
    /// Lines computed by `measure_text` for the last width it was asked
    /// about, so the post-layout pass can reuse them instead of re-running
    /// `wrap_text`/`truncate_line` from scratch when the final resolved
    /// width matches what was measured. Keyed by that width; a mismatch
    /// means a stale cache that must be recomputed, never reused.
    cached: std::cell::RefCell<Option<(u16, Vec<String>)>>,
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
                    width: percent(1.0f32),
                    height: percent(1.0f32),
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

    // Overlay nodes (`ViewProps.overlay`) need their *own* rect placed against the whole
    // viewport rather than wherever taffy's own (`position: Absolute`)
    // resolution would put them — but critically, `walk_abs` must apply that
    // same correction as the *origin* it hands down to their descendants
    // too, or a "background" text sitting deep inside e.g. `widgets::Toast`
    // ends up positioned relative to taffy's uncorrected location instead of
    // the corrected one, landing nowhere near its own overlay box.
    let overlay_anchors: HashMap<NodeId, Anchor> = pairs
        .iter()
        .filter_map(|(fid, node)| match &tree.get(*fid).kind {
            FiberKind::View(p) => p.overlay.map(|anchor| (*node, anchor)),
            _ => None,
        })
        .collect();

    let mut abs: HashMap<NodeId, (f32, f32)> = HashMap::new();
    walk_abs(
        &taffy,
        root_node,
        (0.0, 0.0),
        &mut abs,
        &overlay_anchors,
        (width, height),
    );
    for (fid, node) in pairs {
        let l = taffy.layout(node).unwrap();
        let w = l.size.width.round() as u16;
        let h = l.size.height.round() as u16;
        let fiber = tree.get_mut(fid);
        let (x, y) = abs[&node];
        let rect = Rect {
            x: x.round() as u16,
            y: y.round() as u16,
            width: w,
            height: h,
        };
        fiber.layout = rect;
        // Wrap/truncate once here at the final resolved width so `paint` (which
        // runs every frame, even when layout is cached) reuses these lines
        // instead of re-wrapping. Refilled each pass so stale lines from a
        // previous frame can never be painted. For `TextWrap::Wrap`, reuse
        // `measure_text`'s cached lines when they were already computed for
        // this exact final width, instead of re-running `wrap_text`.
        fiber.wrapped = match &fiber.kind {
            FiberKind::Text(props) => Some(match props.wrap {
                TextWrap::Wrap => {
                    let cached = taffy.get_node_context(node).and_then(|ctx| {
                        let cached = ctx.cached.borrow();
                        cached
                            .as_ref()
                            .filter(|(w, _)| *w == rect.width)
                            .map(|(_, lines)| lines.clone())
                    });
                    cached.unwrap_or_else(|| wrap_text(&props.content, rect.width as usize))
                }
                TextWrap::Truncate => {
                    vec![truncate_line(&props.content, rect.width as usize)]
                }
            }),
            _ => None,
        };
        // Feed a scroll box its measured sizes so its offset stays clamped and
        // bottom-following. `content_size` includes the overflowing children.
        if let FiberKind::View(props) = &fiber.kind
            && let Some(scroll) = &props.scroll
        {
            let content = l.content_size.height.round() as u16;
            scroll.set_metrics(content, rect.height);
        }
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
                        cached: std::cell::RefCell::new(None),
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
    let overflow = match p.overflow {
        NOverflow::Visible => taffy::Overflow::Visible,
        NOverflow::Clip => taffy::Overflow::Clip,
        NOverflow::Scroll => taffy::Overflow::Scroll,
    };
    Style {
        display: Display::Flex,
        // Absolute takes it out of its parent's flex flow (no space
        // reserved, doesn't push siblings); `compute_layout` then overrides
        // its resolved position to center it against the viewport, ignoring
        // whatever positioning-context taffy would otherwise have used.
        position: if p.overlay.is_some() {
            Position::Absolute
        } else {
            Position::Relative
        },
        flex_direction: match p.flex_direction {
            NFlex::Row => FlexDirection::Row,
            NFlex::Column => FlexDirection::Column,
        },
        justify_content: Some(match p.justify_content {
            NJustify::Start => JustifyContent::START,
            NJustify::End => JustifyContent::END,
            NJustify::Center => JustifyContent::CENTER,
            NJustify::SpaceBetween => JustifyContent::SPACE_BETWEEN,
            NJustify::SpaceAround => JustifyContent::SPACE_AROUND,
            NJustify::SpaceEvenly => JustifyContent::SPACE_EVENLY,
        }),
        align_items: Some(match p.align_items {
            NAlign::Stretch => AlignItems::STRETCH,
            NAlign::Start => AlignItems::START,
            NAlign::End => AlignItems::END,
            NAlign::Center => AlignItems::CENTER,
        }),
        flex_grow: p.flex_grow,
        gap: Size {
            width: length(p.gap as f32),
            height: length(p.gap as f32),
        },
        padding: taffy::Rect::length(p.padding as f32),
        margin: taffy::Rect::length(p.margin as f32),
        border: if p.border_style == BorderStyle::None {
            taffy::Rect::length(0.0f32)
        } else {
            taffy::Rect::length(1.0f32)
        },
        size: Size {
            width: dim(p.width),
            height: dim(p.height),
        },
        // Clip/Scroll containers must not reserve a scrollbar gutter.
        overflow: taffy::Point {
            x: overflow,
            y: overflow,
        },
        scrollbar_width: 0.0f32,
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
            // Stash for the post-layout pass to reuse if the final resolved
            // width matches — taffy may call this measure fn more than once
            // per node while solving, so this just keeps the *last* measured
            // width/lines, which is cheap and correct either way: a mismatch
            // at reuse time falls back to recomputing from scratch.
            *ctx.cached.borrow_mut() = Some((max_w as u16, lines));
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
    overlay_anchors: &HashMap<NodeId, Anchor>,
    viewport: (u16, u16),
) {
    let l = taffy.layout(node).unwrap();
    let pos = if let Some(&anchor) = overlay_anchors.get(&node) {
        overlay_position(
            anchor,
            l.size.width.round() as u16,
            l.size.height.round() as u16,
            viewport,
        )
    } else {
        (origin.0 + l.location.x, origin.1 + l.location.y)
    };
    abs.insert(node, pos);
    for c in taffy.children(node).unwrap() {
        walk_abs(taffy, c, pos, abs, overlay_anchors, viewport);
    }
}

/// Where an overlay box of size `w`×`h` lands against a
/// `viewport`-sized screen, as `f32` so it composes with `walk_abs`'s
/// location arithmetic. Corner anchors sit `EDGE_MARGIN` cells in.
fn overlay_position(anchor: Anchor, w: u16, h: u16, viewport: (u16, u16)) -> (f32, f32) {
    const EDGE_MARGIN: u16 = 1;
    let (vw, vh) = viewport;
    let (x, y) = match anchor {
        Anchor::Center => (vw.saturating_sub(w) / 2, vh.saturating_sub(h) / 2),
        Anchor::TopLeft => (EDGE_MARGIN, EDGE_MARGIN),
        Anchor::TopRight => (
            vw.saturating_sub(w).saturating_sub(EDGE_MARGIN),
            EDGE_MARGIN,
        ),
        Anchor::BottomLeft => (
            EDGE_MARGIN,
            vh.saturating_sub(h).saturating_sub(EDGE_MARGIN),
        ),
        Anchor::BottomRight => (
            vw.saturating_sub(w).saturating_sub(EDGE_MARGIN),
            vh.saturating_sub(h).saturating_sub(EDGE_MARGIN),
        ),
    };
    (x as f32, y as f32)
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

    #[test]
    fn justify_align_and_percent_variants() {
        use crate::props::{AlignItems, JustifyContent};

        // Lay out a Row of two 1-wide / 1-tall texts in a 20x5 (Percent(100)) box
        // and return the two children's rects, so we can assert positions.
        fn row(j: JustifyContent, a: AlignItems) -> (Rect, Rect) {
            let (rt, _rx) = RuntimeHandle::test_handle();
            let mut tree = FiberTree::new();
            let root = tree.mount_root(
                Element::view(
                    ViewProps {
                        justify_content: j,
                        align_items: a,
                        flex_direction: FlexDirection::Row,
                        width: Dimension::Percent(100.0),
                        height: Dimension::Percent(100.0),
                        ..Default::default()
                    },
                    vec![
                        Element::text(TextProps {
                            content: "a".into(),
                            ..Default::default()
                        }),
                        Element::text(TextProps {
                            content: "b".into(),
                            ..Default::default()
                        }),
                    ],
                ),
                &rt,
            );
            compute_layout(&mut tree, 20, 5);
            let kids = tree.get(root).children.clone();
            (tree.get(kids[0]).layout, tree.get(kids[1]).layout)
        }

        // justify_content drives main-axis (horizontal) placement.
        let stretch = AlignItems::Stretch;
        let (r0, r1) = row(JustifyContent::Start, stretch);
        assert_eq!((r0.x, r1.x), (0, 1), "Start packs at the left");
        let (r0, r1) = row(JustifyContent::End, stretch);
        assert_eq!((r0.x, r1.x), (18, 19), "End packs at the right");
        let (r0, r1) = row(JustifyContent::SpaceBetween, stretch);
        assert_eq!((r0.x, r1.x), (0, 19), "SpaceBetween pins to both edges");
        let (r0, r1) = row(JustifyContent::Center, stretch);
        assert!(
            r0.x > 0 && r1.x < 19 && r0.x < r1.x,
            "Center is inset: {r0:?} {r1:?}"
        );
        for j in [JustifyContent::SpaceAround, JustifyContent::SpaceEvenly] {
            let (r0, r1) = row(j, stretch);
            assert!(
                r0.x > 0 && r1.x < 19 && r0.x < r1.x,
                "{j:?} distributes interior gaps: {r0:?} {r1:?}"
            );
        }

        // align_items drives cross-axis (vertical) placement in the 5-tall box.
        assert_eq!(row(JustifyContent::Start, AlignItems::Start).0.y, 0);
        assert_eq!(row(JustifyContent::Start, AlignItems::End).0.y, 4);
        assert_eq!(row(JustifyContent::Start, AlignItems::Center).0.y, 2);
        assert_eq!(
            row(JustifyContent::Start, AlignItems::Stretch).0.height,
            5,
            "Stretch fills the cross axis"
        );
    }

    #[test]
    fn empty_tree_layout_is_a_noop() {
        let mut tree = FiberTree::new(); // no root -> early return
        compute_layout(&mut tree, 10, 5);
        assert_eq!(tree.root, None, "no root: layout leaves the tree untouched");
    }

    #[test]
    fn overlay_layer_is_centered_in_the_viewport_regardless_of_nesting() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        // Deeply nested, off to one side — should make no difference: an
        // overlay is centered against the whole viewport, not its ancestors.
        let root = tree.mount_root(
            Element::view(
                ViewProps {
                    justify_content: crate::props::JustifyContent::End,
                    width: Dimension::Cells(20),
                    height: Dimension::Cells(10),
                    ..Default::default()
                },
                vec![Element::view(
                    ViewProps {
                        width: Dimension::Cells(1),
                        height: Dimension::Cells(1),
                        ..Default::default()
                    },
                    vec![Element::view(
                        ViewProps {
                            overlay: Some(crate::props::Anchor::Center),
                            width: Dimension::Cells(6),
                            height: Dimension::Cells(4),
                            ..Default::default()
                        },
                        vec![],
                    )],
                )],
            ),
            &rt,
        );
        compute_layout(&mut tree, 20, 10);
        let overlay = tree.get(tree.get(root).children[0]).children[0];
        // (20-6)/2 = 7, (10-4)/2 = 3.
        assert_eq!(
            tree.get(overlay).layout,
            Rect {
                x: 7,
                y: 3,
                width: 6,
                height: 4
            }
        );
    }

    #[test]
    fn overlay_layer_does_not_affect_its_parents_size() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        let root = tree.mount_root(
            Element::view(
                ViewProps {
                    width: Dimension::Cells(2),
                    height: Dimension::Cells(2),
                    ..Default::default()
                },
                vec![Element::view(
                    ViewProps {
                        overlay: Some(crate::props::Anchor::Center),
                        width: Dimension::Cells(15),
                        height: Dimension::Cells(8),
                        ..Default::default()
                    },
                    vec![],
                )],
            ),
            &rt,
        );
        compute_layout(&mut tree, 20, 10);
        assert_eq!(
            tree.get(root).layout,
            Rect {
                x: 0,
                y: 0,
                width: 2,
                height: 2
            },
            "an out-of-flow overlay child must not grow its parent"
        );
    }

    #[test]
    fn overlay_layer_top_right_anchor_sits_in_the_top_right_corner() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        let root = tree.mount_root(
            Element::view(
                ViewProps {
                    overlay: Some(crate::props::Anchor::TopRight),
                    width: Dimension::Cells(4),
                    height: Dimension::Cells(1),
                    ..Default::default()
                },
                vec![],
            ),
            &rt,
        );
        compute_layout(&mut tree, 20, 10);
        // width=4, EDGE_MARGIN=1: expect x = 20-4-1 = 15, y = 1.
        assert_eq!(
            tree.get(root).layout,
            Rect {
                x: 15,
                y: 1,
                width: 4,
                height: 1
            }
        );
    }

    #[test]
    fn overlay_layer_top_right_anchor_with_auto_width_sized_to_content() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        let root = tree.mount_root(
            Element::view(
                ViewProps {
                    overlay: Some(crate::props::Anchor::TopRight),
                    // width left at Auto, like `widgets::Tooltip` actually does.
                    ..Default::default()
                },
                vec![text("hint")],
            ),
            &rt,
        );
        compute_layout(&mut tree, 20, 10);
        let r = tree.get(root).layout;
        eprintln!("auto-width top-right overlay resolved to {r:?}");
        assert_eq!(r.width, 4, "should shrink to the 4-char content");
        assert_eq!(r.x, 15, "20 - 4 - EDGE_MARGIN(1) = 15");
    }

    #[test]
    fn two_overlay_siblings_are_each_positioned_independently() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        let root = tree.mount_root(
            Element::fragment(vec![
                Element::view(
                    ViewProps {
                        overlay: Some(crate::props::Anchor::TopRight),
                        ..Default::default()
                    },
                    vec![text("hint")],
                ),
                Element::view(
                    ViewProps {
                        overlay: Some(crate::props::Anchor::BottomRight),
                        ..Default::default()
                    },
                    vec![text("toast")],
                ),
            ]),
            &rt,
        );
        compute_layout(&mut tree, 20, 10);
        let kids = tree.get(root).children.clone();
        let r0 = tree.get(kids[0]).layout;
        let r1 = tree.get(kids[1]).layout;
        eprintln!("overlay 0 (TopRight, 'hint'): {r0:?}");
        eprintln!("overlay 1 (BottomRight, 'toast'): {r1:?}");
        assert_eq!(
            r0,
            Rect {
                x: 15,
                y: 1,
                width: 4,
                height: 1
            }
        );
        assert_eq!(
            r1,
            Rect {
                x: 14,
                y: 8,
                width: 5,
                height: 1
            }
        );
    }

    #[test]
    fn overlay_descendants_are_positioned_relative_to_the_corrected_overlay_rect() {
        // Regression test: `walk_abs` originally accumulated absolute
        // positions using taffy's own (uncorrected) location for an
        // `Position: Absolute` overlay node, then handed *that* down as the
        // origin for its children — so a Text child several levels inside an
        // overlay (like `widgets::Toast`'s message) landed near (0, 0)
        // instead of near its overlay box, even though the overlay box
        // itself reported the right rect.
        use crate::props::Anchor;
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        let root = tree.mount_root(
            Element::fragment(vec![
                text("background"),
                Element::view(
                    ViewProps {
                        overlay: Some(Anchor::BottomRight),
                        padding: 1,
                        border_style: BorderStyle::Round,
                        ..Default::default()
                    },
                    vec![text("toast")],
                ),
            ]),
            &rt,
        );
        compute_layout(&mut tree, 20, 10);
        let kids = tree.get(root).children.clone();
        let overlay_id = kids[1];
        let overlay = tree.get(overlay_id).layout;
        let text_id = tree.get(overlay_id).children[0];
        let text_rect = tree.get(text_id).layout;

        // Overlay box: "toast" (5 wide, 1 tall) + padding 1 + border 1 each
        // side = 9x5, bottom-right anchored in a 20x10 viewport (EDGE_MARGIN 1).
        assert_eq!(
            overlay,
            Rect {
                x: 10,
                y: 4,
                width: 9,
                height: 5
            }
        );
        // The Text child sits one cell in from the overlay's own top-left
        // (border + padding), i.e. relative to the *corrected* overlay
        // position — not off near (0, 0).
        assert_eq!(
            text_rect,
            Rect {
                x: 12,
                y: 6,
                width: 5,
                height: 1
            }
        );
    }

    #[test]
    fn three_overlays_plus_text_position_correctly_through_a_component_wrapper() {
        use crate::component::Component;
        use crate::props::Anchor;

        struct Wrapper;
        #[derive(Clone, PartialEq, Default)]
        struct WrapperProps;
        impl Component for Wrapper {
            type Props = WrapperProps;
            fn render(_: &WrapperProps, _hooks: &mut crate::hooks::Hooks) -> Element {
                Element::fragment(vec![
                    text("background"),
                    Element::view(
                        ViewProps {
                            overlay: Some(Anchor::TopRight),
                            ..Default::default()
                        },
                        vec![text("hint")],
                    ),
                    Element::view(
                        ViewProps {
                            overlay: Some(Anchor::BottomRight),
                            ..Default::default()
                        },
                        vec![text("toast")],
                    ),
                    Element::view(
                        ViewProps {
                            overlay: Some(Anchor::Center),
                            width: Dimension::Cells(10),
                            height: Dimension::Cells(3),
                            ..Default::default()
                        },
                        vec![],
                    ),
                ])
            }
        }

        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        let root = tree.mount_root(Element::component::<Wrapper>(WrapperProps), &rt);
        compute_layout(&mut tree, 20, 10);
        // Walk down through the Component fiber to its Fragment child's kids.
        let frag = tree.get(root).children[0];
        let kids = tree.get(frag).children.clone();
        assert_eq!(
            tree.get(kids[1]).layout,
            Rect {
                x: 15,
                y: 1,
                width: 4,
                height: 1
            }
        );
        assert_eq!(
            tree.get(kids[2]).layout,
            Rect {
                x: 14,
                y: 8,
                width: 5,
                height: 1
            }
        );
        // And each overlay's own Text child, not just the overlay box itself.
        let hint_text = tree.get(kids[1]).children[0];
        let toast_text = tree.get(kids[2]).children[0];
        assert_eq!(
            tree.get(hint_text).layout,
            Rect {
                x: 15,
                y: 1,
                width: 4,
                height: 1
            }
        );
        assert_eq!(
            tree.get(toast_text).layout,
            Rect {
                x: 14,
                y: 8,
                width: 5,
                height: 1
            }
        );
    }
}
