use crate::component::Component;
use crate::element::Element;
use crate::hooks::Hooks;
use crate::hooks::input::KeyCode;
use crate::props::{FlexDirection, TextProps, ViewProps};
use crate::style::Weight;
use crate::widgets::callback::Callback;
use crate::widgets::select::use_clamped_index;

/// A focusable tab strip: Left/Right moves the active tab and calls
/// `on_change` with the new index, wrapping at the ends.
///
/// `Tabs` renders only the strip, not tab content — pair it with your own
/// `active`-indexed `match`/`if` in the surrounding component to show the
/// right panel. Keeping content out of this widget avoids needing arbitrary
/// child elements inside `Component::Props` (which must be `PartialEq`).
#[derive(Clone, PartialEq, Default)]
pub struct TabsProps {
    pub labels: Vec<String>,
    pub active: usize,
    pub on_change: Option<Callback<usize>>,
}

pub struct Tabs;
impl Component for Tabs {
    type Props = TabsProps;
    fn render(props: &TabsProps, hooks: &mut Hooks) -> Element {
        let theme = hooks.use_theme();
        let focus = hooks.use_focusable();
        let is_focused = focus.is_focused();
        let len = props.labels.len();

        let active = use_clamped_index(hooks, props.active, len);

        let a = active.clone();
        let on_change = props.on_change.clone();
        hooks.use_input(move |ev, ctx| {
            if !is_focused || len == 0 {
                return;
            }
            let next = match ev.code {
                KeyCode::Left => Some((a.get().min(len - 1) + len - 1) % len),
                KeyCode::Right => Some((a.get() + 1) % len),
                _ => None,
            };
            if let Some(next) = next {
                a.set(next);
                if let Some(cb) = &on_change {
                    cb.call(next);
                }
                ctx.stop_propagation();
            }
        });

        let active_idx = active.get();
        let cells = props
            .labels
            .iter()
            .enumerate()
            .map(|(i, label)| {
                let selected = i == active_idx;
                let color = match (selected, is_focused) {
                    (true, true) => theme.accent,
                    (true, false) => theme.foreground,
                    (false, _) => theme.muted,
                };
                Element::text(TextProps {
                    content: label.clone(),
                    color,
                    weight: if selected {
                        Weight::Bold
                    } else {
                        Weight::Normal
                    },
                    ..Default::default()
                })
            })
            .collect();

        Element::view(
            ViewProps {
                flex_direction: FlexDirection::Row,
                gap: 2,
                ..Default::default()
            },
            cells,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::input::KeyCode;
    use crate::testing::TestTerminal;
    use std::cell::Cell;
    use std::rc::Rc;

    struct Scope;
    #[derive(Clone, PartialEq, Default)]
    struct ScopeProps {
        last: Rc<Cell<Option<usize>>>,
    }
    impl Component for Scope {
        type Props = ScopeProps;
        fn render(props: &ScopeProps, hooks: &mut Hooks) -> Element {
            let scope = hooks.use_focus_scope();
            let last = props.last.clone();
            Element::provider(
                scope,
                vec![Element::component::<Tabs>(TabsProps {
                    labels: vec!["one".into(), "two".into(), "three".into()],
                    active: 0,
                    on_change: Some(Callback::new(move |i| last.set(Some(i)))),
                })],
            )
        }
    }

    #[tokio::test]
    async fn right_advances_and_wraps() {
        let props = ScopeProps::default();
        let mut t = TestTerminal::new(30, 1, Element::component::<Scope>(props.clone())).unwrap();
        t.send_key(KeyCode::Right).unwrap();
        assert_eq!(props.last.get(), Some(1));
        t.send_key(KeyCode::Right).unwrap();
        assert_eq!(props.last.get(), Some(2));
        t.send_key(KeyCode::Right).unwrap();
        assert_eq!(props.last.get(), Some(0));
    }

    #[tokio::test]
    async fn left_from_the_start_wraps_to_the_end() {
        let props = ScopeProps::default();
        let mut t = TestTerminal::new(30, 1, Element::component::<Scope>(props.clone())).unwrap();
        t.send_key(KeyCode::Left).unwrap();
        assert_eq!(props.last.get(), Some(2));
    }

    #[tokio::test]
    async fn active_out_of_range_clamps_and_left_still_navigates_correctly() {
        // 3 labels, `active: 99` is out of range and must clamp to the last
        // valid index (2) on mount, and Left must still navigate sanely from
        // there rather than getting stuck out of range.
        struct OutOfRangeScope;
        #[derive(Clone, PartialEq, Default)]
        struct OutOfRangeScopeProps {
            last: Rc<Cell<Option<usize>>>,
        }
        impl Component for OutOfRangeScope {
            type Props = OutOfRangeScopeProps;
            fn render(props: &OutOfRangeScopeProps, hooks: &mut Hooks) -> Element {
                let scope = hooks.use_focus_scope();
                let last = props.last.clone();
                Element::provider(
                    scope,
                    vec![Element::component::<Tabs>(TabsProps {
                        labels: vec!["one".into(), "two".into(), "three".into()],
                        active: 99,
                        on_change: Some(Callback::new(move |i| last.set(Some(i)))),
                    })],
                )
            }
        }
        let props = OutOfRangeScopeProps {
            last: Rc::new(Cell::new(None)),
        };
        let mut t =
            TestTerminal::new(30, 1, Element::component::<OutOfRangeScope>(props.clone())).unwrap();
        // Sanity: all three tabs still render (color/weight highlighting
        // isn't observable via frame_text, so the real clamp check is below).
        let out = t.frame_text();
        assert!(out.contains("three"));

        // If `active` had correctly clamped to index 2 ("three") on mount,
        // Left moves to index 1. If it stayed unclamped at 99, Left would
        // compute 98 instead (99 != 0, so the old branch just decrements).
        t.send_key(KeyCode::Left).unwrap();
        assert_eq!(props.last.get(), Some(1));
    }

    #[tokio::test]
    async fn shrinking_labels_reclamps_a_still_in_range_active_prop() {
        // 5 tabs, `active: 4` is valid on mount. Pressing `s` shrinks the
        // outer scope's own tab count down to 2 and re-renders `Tabs` with
        // the *same* `active: 4` prop value, which is now out of range. The
        // clamp effect must re-fire even though `active` itself didn't
        // change, because `len` did.
        struct ShrinkScope;
        #[derive(Clone, PartialEq, Default)]
        struct ShrinkScopeProps {
            last: Rc<Cell<Option<usize>>>,
        }
        impl Component for ShrinkScope {
            type Props = ShrinkScopeProps;
            fn render(props: &ShrinkScopeProps, hooks: &mut Hooks) -> Element {
                let scope = hooks.use_focus_scope();
                let label_count = hooks.use_state(|| 5usize);
                let lc = label_count.clone();
                hooks.use_input(move |ev, _ctx| {
                    if matches!(ev.code, KeyCode::Char('s')) {
                        lc.set(2);
                    }
                });
                let labels = (0..label_count.get()).map(|i| format!("tab{i}")).collect();
                let last = props.last.clone();
                Element::provider(
                    scope,
                    vec![Element::component::<Tabs>(TabsProps {
                        labels,
                        active: 4,
                        on_change: Some(Callback::new(move |i| last.set(Some(i)))),
                    })],
                )
            }
        }
        let props = ShrinkScopeProps {
            last: Rc::new(Cell::new(None)),
        };
        let mut t =
            TestTerminal::new(30, 1, Element::component::<ShrinkScope>(props.clone())).unwrap();

        t.send_key(KeyCode::Char('s')).unwrap();

        // If `active` correctly re-clamped to index 1 (the last valid tab
        // among 2), Right wraps to 0. If it stayed stuck at the stale 4,
        // Right computes `(4 + 1) % 2 == 1` instead — the clamped-vs-stale
        // outcomes disagree (0 vs 1).
        t.send_key(KeyCode::Right).unwrap();
        assert_eq!(
            props.last.get(),
            Some(0),
            "active should have re-clamped to the last valid index (1) and Right should wrap to 0"
        );
    }
}
