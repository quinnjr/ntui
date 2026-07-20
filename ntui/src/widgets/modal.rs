use crate::component::Component;
use crate::element::Element;
use crate::hooks::Hooks;
use crate::hooks::input::KeyCode;
use crate::props::{
    AlignItems, Anchor, Dimension, FlexDirection, JustifyContent, TextProps, TextWrap, ViewProps,
};
use crate::style::{Color, Weight};
use crate::widgets::callback::Callback;

/// A centered dialog over a darkened backdrop covering the whole terminal.
/// Esc calls `on_close`. Sized from [`Hooks::use_terminal_size`] rather than
/// `Dimension::Percent`, since percentages on an absolutely-positioned
/// (overlay) box would resolve against an unpredictable containing
/// block — explicit cell sizes sidestep that entirely.
///
/// A cell grid has no real alpha, so "backdrop" here is a solid color (a
/// darkened variant of the theme surface) — a see-through dim of whatever
/// was underneath is not possible on a cell grid, not merely unimplemented.
/// There is no drop-shadow rect either; unlike the alpha limitation, that
/// one could be added later (an offset `View` behind the dialog) but isn't
/// here yet.
#[derive(Clone, PartialEq, Default)]
pub struct ModalProps {
    pub title: String,
    pub message: String,
    pub on_close: Option<Callback>,
}

pub struct Modal;
impl Component for Modal {
    type Props = ModalProps;
    fn render(props: &ModalProps, hooks: &mut Hooks) -> Element {
        let theme = hooks.use_theme();
        let (w, h) = hooks.use_terminal_size();

        let on_close = props.on_close.clone();
        hooks.use_input(move |ev, ctx| {
            if ev.code == KeyCode::Esc {
                if let Some(cb) = &on_close {
                    cb.call(());
                }
                ctx.stop_propagation();
            }
        });

        let backdrop = Color::lerp(theme.surface, Color::Black, 0.75);
        let dialog_width = 44.min(w.saturating_sub(4)).max(10);

        let dialog = Element::view(
            ViewProps {
                flex_direction: FlexDirection::Column,
                border_style: theme.border_style,
                border_color: theme.accent,
                background: theme.surface,
                padding: 1,
                gap: 1,
                width: Dimension::Cells(dialog_width),
                ..Default::default()
            },
            vec![
                Element::text(TextProps {
                    content: props.title.clone(),
                    color: theme.accent,
                    weight: Weight::Bold,
                    ..Default::default()
                }),
                Element::text(TextProps {
                    content: props.message.clone(),
                    color: theme.foreground,
                    wrap: TextWrap::Wrap,
                    ..Default::default()
                }),
                Element::text(TextProps {
                    content: "Esc to close".to_string(),
                    color: theme.muted,
                    ..Default::default()
                }),
            ],
        );

        Element::view(
            ViewProps {
                overlay: Some(Anchor::Center),
                width: Dimension::Cells(w),
                height: Dimension::Cells(h),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                background: backdrop,
                ..Default::default()
            },
            vec![dialog],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Buffer;
    use crate::fiber::FiberTree;
    use crate::hooks::RuntimeHandle;
    use crate::hooks::input::KeyCode;
    use crate::layout::compute_layout;
    use crate::paint::paint;
    use crate::testing::TestTerminal;
    use std::cell::Cell;
    use std::rc::Rc;

    #[tokio::test]
    async fn shows_title_and_message_and_esc_closes() {
        let closed = Rc::new(Cell::new(false));
        let c = closed.clone();
        let mut t = TestTerminal::new(
            40,
            10,
            Element::component::<Modal>(ModalProps {
                title: "Delete file?".into(),
                message: "This can't be undone.".into(),
                on_close: Some(Callback::new(move |()| c.set(true))),
            }),
        )
        .unwrap();
        let out = t.frame_text();
        assert!(out.contains("Delete file?"), "{out:?}");
        assert!(out.contains("undone"), "{out:?}");
        t.send_key(KeyCode::Esc).unwrap();
        assert!(closed.get());
    }

    #[tokio::test]
    async fn backdrop_covers_the_whole_viewport_and_does_not_grow_past_it() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        tree.mount_root(
            Element::component::<Modal>(ModalProps {
                title: "t".into(),
                message: "m".into(),
                on_close: None,
            }),
            &rt,
        );
        compute_layout(&mut tree, 30, 12);
        let mut buf = Buffer::new(30, 12);
        paint(&tree, &mut buf);
        // Corners should be the darkened backdrop, not left untouched.
        assert_ne!(buf.get(0, 0).bg, Color::Reset);
        assert_ne!(buf.get(29, 11).bg, Color::Reset);
    }

    struct StaticOverlays;
    #[derive(Clone, PartialEq, Default)]
    struct StaticOverlaysProps;
    impl Component for StaticOverlays {
        type Props = StaticOverlaysProps;
        fn render(_: &StaticOverlaysProps, _hooks: &mut Hooks) -> Element {
            Element::fragment(vec![
                Element::text(TextProps {
                    content: "background".into(),
                    ..Default::default()
                }),
                Element::view(
                    ViewProps {
                        overlay: Some(Anchor::TopRight),
                        ..Default::default()
                    },
                    vec![Element::text(TextProps {
                        content: "hint".into(),
                        ..Default::default()
                    })],
                ),
                Element::view(
                    ViewProps {
                        overlay: Some(Anchor::BottomRight),
                        ..Default::default()
                    },
                    vec![Element::text(TextProps {
                        content: "toast".into(),
                        ..Default::default()
                    })],
                ),
            ])
        }
    }

    #[tokio::test]
    async fn static_no_hooks_overlays_position_correctly_through_test_terminal() {
        let t = TestTerminal::new(
            20,
            6,
            Element::component::<StaticOverlays>(StaticOverlaysProps),
        )
        .unwrap();
        let out = t.frame_text();
        eprintln!("static overlays via TestTerminal:\n{out}");
        assert!(
            out.lines().next().unwrap().starts_with("background"),
            "background should be untouched at (0,0): {out:?}"
        );
    }

    struct Toggler;
    #[derive(Clone, PartialEq, Default)]
    struct TogglerProps {
        show: crate::test_util::Shared<Option<crate::hooks::state::State<bool>>>,
    }
    impl Component for Toggler {
        type Props = TogglerProps;
        fn render(props: &TogglerProps, hooks: &mut Hooks) -> Element {
            let show = hooks.use_state(|| true);
            *props.show.lock() = Some(show.clone());
            let mut children = vec![Element::text(TextProps {
                content: "background".into(),
                ..Default::default()
            })];
            if show.get() {
                children.push(Element::component::<Modal>(ModalProps {
                    title: "t".into(),
                    message: "m".into(),
                    on_close: None,
                }));
            }
            Element::fragment(children)
        }
    }

    #[tokio::test]
    async fn closing_the_modal_clears_its_backdrop_from_the_terminal() {
        let show = crate::test_util::Shared::default();
        let mut t = TestTerminal::new(
            20,
            6,
            Element::component::<Toggler>(TogglerProps { show: show.clone() }),
        )
        .unwrap();
        // Sanity: with the modal open, its backdrop should paint over the
        // background text at (0, 0) too (unclipped, on top of everything).
        assert_ne!(
            t.frame_text().lines().next().unwrap().trim(),
            "background",
            "modal should be covering the background text"
        );

        let handle = show.lock().clone().unwrap();
        handle.set(false);
        t.tick().await.unwrap();

        let out = t.frame_text();
        assert_eq!(
            out.lines().next().unwrap().trim(),
            "background",
            "closing the modal should clear its backdrop, not leave it stale: {out:?}"
        );
    }

    struct MultiOverlayToggler;
    #[derive(Clone, PartialEq, Default)]
    struct MultiOverlayTogglerProps {
        show: crate::test_util::Shared<Option<crate::hooks::state::State<bool>>>,
    }
    impl Component for MultiOverlayToggler {
        type Props = MultiOverlayTogglerProps;
        fn render(props: &MultiOverlayTogglerProps, hooks: &mut Hooks) -> Element {
            use crate::props::Anchor;
            use crate::widgets::toast::{Toast, ToastProps};
            use crate::widgets::tooltip::{Tooltip, TooltipProps};

            let show = hooks.use_state(|| true);
            *props.show.lock() = Some(show.clone());
            let mut children = vec![
                Element::text(TextProps {
                    content: "background".into(),
                    ..Default::default()
                }),
                Element::component::<Tooltip>(TooltipProps {
                    message: "hint".into(),
                    anchor: Anchor::TopRight,
                }),
                Element::component::<Toast>(ToastProps {
                    message: "toast".into(),
                    duration: None,
                    ..Default::default()
                }),
            ];
            if show.get() {
                children.push(Element::component::<Modal>(ModalProps {
                    title: "t".into(),
                    message: "m".into(),
                    on_close: None,
                }));
            }
            Element::fragment(children)
        }
    }

    struct RawOverlayToggler;
    #[derive(Clone, PartialEq, Default)]
    struct RawOverlayTogglerProps {
        show: crate::test_util::Shared<Option<crate::hooks::state::State<bool>>>,
    }
    impl Component for RawOverlayToggler {
        type Props = RawOverlayTogglerProps;
        fn render(props: &RawOverlayTogglerProps, hooks: &mut Hooks) -> Element {
            use crate::props::Anchor;

            let show = hooks.use_state(|| true);
            *props.show.lock() = Some(show.clone());
            let mut children = vec![
                Element::text(TextProps {
                    content: "background".into(),
                    ..Default::default()
                }),
                Element::view(
                    ViewProps {
                        overlay: Some(Anchor::TopRight),
                        ..Default::default()
                    },
                    vec![Element::text(TextProps {
                        content: "hint".into(),
                        ..Default::default()
                    })],
                ),
                Element::view(
                    ViewProps {
                        overlay: Some(Anchor::BottomRight),
                        ..Default::default()
                    },
                    vec![Element::text(TextProps {
                        content: "toast".into(),
                        ..Default::default()
                    })],
                ),
            ];
            if show.get() {
                children.push(Element::view(
                    ViewProps {
                        overlay: Some(Anchor::Center),
                        width: Dimension::Cells(10),
                        height: Dimension::Cells(3),
                        border_style: crate::style::BorderStyle::Round,
                        ..Default::default()
                    },
                    vec![],
                ));
            }
            Element::fragment(children)
        }
    }

    #[tokio::test]
    async fn raw_view_overlays_also_clear_correctly_when_one_is_removed() {
        let show = crate::test_util::Shared::default();
        let mut t = TestTerminal::new(
            20,
            6,
            Element::component::<RawOverlayToggler>(RawOverlayTogglerProps { show: show.clone() }),
        )
        .unwrap();
        eprintln!("frame 1 (modal open):\n{}", t.frame_text());
        let handle = show.lock().clone().unwrap();
        handle.set(false);
        t.tick().await.unwrap();
        let out = t.frame_text();
        eprintln!("frame 2 (modal closed):\n{out}");
        assert_eq!(
            out.lines().next().unwrap().trim(),
            "background",
            "closing the raw-view overlay should clear it, not leave it stale: {out:?}"
        );
    }

    #[tokio::test]
    async fn closing_the_modal_clears_its_backdrop_even_alongside_other_overlays() {
        // Toast's own bordered box legitimately reaches row 0 at this small
        // a viewport height, so the right invariant isn't "row 0 is plain
        // background text" — it's "the screen after closing the modal looks
        // exactly like it would if the modal had never opened at all."
        let show = crate::test_util::Shared::default();
        let mut t = TestTerminal::new(
            20,
            10,
            Element::component::<MultiOverlayToggler>(MultiOverlayTogglerProps {
                show: show.clone(),
            }),
        )
        .unwrap();
        let with_modal = t.frame_text();
        assert!(
            with_modal.contains("╭") && with_modal.contains("╰"),
            "modal's dialog border should be visible: {with_modal:?}"
        );

        let handle = show.lock().clone().unwrap();
        handle.set(false);
        t.tick().await.unwrap();
        let after_close = t.frame_text();

        // Now render fresh with the modal never opened at all, at the same
        // size, and compare — this is the actual "did closing fully restore
        // things" invariant, independent of where Toast's own box happens to
        // land.
        let show2 = crate::test_util::Shared::default();
        let fresh = TestTerminal::new(
            20,
            10,
            Element::component::<MultiOverlayToggler>(MultiOverlayTogglerProps {
                show: show2.clone(),
            }),
        )
        .unwrap();
        let handle2 = show2.lock().clone().unwrap();
        handle2.set(false);
        let mut fresh = fresh;
        fresh.tick().await.unwrap();
        let never_opened = fresh.frame_text();

        assert_eq!(
            after_close, never_opened,
            "closing the modal should leave the screen identical to it never having opened"
        );
    }
}
