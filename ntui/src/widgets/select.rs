use crate::component::Component;
use crate::element::Element;
use crate::hooks::Hooks;
use crate::hooks::input::KeyCode;
use crate::hooks::state::State;
use crate::props::{FlexDirection, TextProps, ViewProps};
use crate::style::Color;
use crate::widgets::callback::Callback;

/// A controlled index that stays valid even if `len` shrinks or `prop_value`
/// is out of range: clamped both at initialization and on every render where
/// `prop_value` or `len` changes (so a shrinking list re-clamps even when
/// `prop_value` itself is unchanged).
pub(crate) fn use_clamped_index(hooks: &mut Hooks, prop_value: usize, len: usize) -> State<usize> {
    let state = hooks.use_state(|| prop_value.min(len.saturating_sub(1)));
    let sync = state.clone();
    hooks.use_effect((prop_value, len), move || {
        sync.set(prop_value.min(len.saturating_sub(1)));
    });
    state
}

/// A focusable single-select list: Up/Down moves the highlighted row and
/// calls `on_change` with the new index. `items` must be non-empty for
/// navigation to do anything.
#[derive(Clone, PartialEq, Default)]
pub struct SelectProps {
    pub items: Vec<String>,
    /// The initially highlighted index; also re-applied whenever it changes
    /// on a later render (a controlled reset), so external code can move the
    /// selection too.
    pub selected: usize,
    pub on_change: Option<Callback<usize>>,
}

pub struct Select;
impl Component for Select {
    type Props = SelectProps;
    fn render(props: &SelectProps, hooks: &mut Hooks) -> Element {
        let theme = hooks.use_theme();
        let focus = hooks.use_focusable();
        let is_focused = focus.is_focused();
        let len = props.items.len();

        let cursor = use_clamped_index(hooks, props.selected, len);

        let c = cursor.clone();
        let on_change = props.on_change.clone();
        hooks.use_input(move |ev, ctx| {
            if !is_focused || len == 0 {
                return;
            }
            let moved = match ev.code {
                KeyCode::Up => {
                    c.update(|i| *i = if *i == 0 { len - 1 } else { *i - 1 });
                    true
                }
                KeyCode::Down => {
                    c.update(|i| *i = (*i + 1) % len);
                    true
                }
                _ => false,
            };
            if moved {
                if let Some(cb) = &on_change {
                    cb.call(c.get());
                }
                ctx.stop_propagation();
            }
        });

        let active_idx = cursor.get();
        let rows = props
            .items
            .iter()
            .enumerate()
            .map(|(i, label)| {
                let active = i == active_idx;
                let (bg, fg) = match (active, is_focused) {
                    (true, true) => (theme.accent, theme.surface),
                    (true, false) => (theme.surface, theme.accent),
                    (false, _) => (Color::Reset, theme.foreground),
                };
                Element::view(
                    ViewProps {
                        background: bg,
                        ..Default::default()
                    },
                    vec![Element::text(TextProps {
                        content: label.clone(),
                        color: fg,
                        ..Default::default()
                    })],
                )
            })
            .collect();

        Element::view(
            ViewProps {
                flex_direction: FlexDirection::Column,
                ..Default::default()
            },
            rows,
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
                vec![Element::component::<Select>(SelectProps {
                    items: vec!["a".into(), "b".into(), "c".into()],
                    selected: 0,
                    on_change: Some(Callback::new(move |i| last.set(Some(i)))),
                })],
            )
        }
    }

    #[tokio::test]
    async fn down_moves_forward_and_wraps() {
        let props = ScopeProps::default();
        let mut t = TestTerminal::new(10, 3, Element::component::<Scope>(props.clone())).unwrap();
        t.send_key(KeyCode::Down).unwrap();
        assert_eq!(props.last.get(), Some(1));
        t.send_key(KeyCode::Down).unwrap();
        assert_eq!(props.last.get(), Some(2));
        t.send_key(KeyCode::Down).unwrap();
        assert_eq!(props.last.get(), Some(0), "should wrap back to the start");
    }

    #[tokio::test]
    async fn up_from_the_start_wraps_to_the_end() {
        let props = ScopeProps::default();
        let mut t = TestTerminal::new(10, 3, Element::component::<Scope>(props.clone())).unwrap();
        t.send_key(KeyCode::Up).unwrap();
        assert_eq!(props.last.get(), Some(2));
    }

    #[tokio::test]
    async fn selected_out_of_range_clamps_to_the_last_item() {
        // 3 items, `selected: 99` is out of range and must clamp to the last
        // valid index (2), not stay at 99 (which would only self-correct via
        // repeated Down presses) or leave nothing highlighted.
        let last = Rc::new(Cell::new(None));
        let last2 = last.clone();
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
                    vec![Element::component::<Select>(SelectProps {
                        items: vec!["a".into(), "b".into(), "c".into()],
                        selected: 99,
                        on_change: Some(Callback::new(move |i| last.set(Some(i)))),
                    })],
                )
            }
        }
        let props = OutOfRangeScopeProps { last: last2 };
        let mut t = TestTerminal::new(10, 3, Element::component::<OutOfRangeScope>(props)).unwrap();
        // If the cursor was correctly clamped to index 2 on mount, Down wraps
        // to 0. If the mount-time effect had overwritten it back to the
        // unclamped 99, Down would land on (99 + 1) % 3 == 1 instead.
        t.send_key(KeyCode::Down).unwrap();
        assert_eq!(last.get(), Some(0));
    }

    #[tokio::test]
    async fn shrinking_items_reclamps_a_still_in_range_selected_prop() {
        // 5 items, `selected: 4` is valid on mount. Pressing `s` shrinks the
        // outer scope's own item count down to 2 and re-renders `Select`
        // with the *same* `selected: 4` prop value, which is now out of
        // range. The clamp effect must re-fire even though `selected` itself
        // didn't change, because `len` did.
        struct ShrinkScope;
        #[derive(Clone, PartialEq, Default)]
        struct ShrinkScopeProps {
            last: Rc<Cell<Option<usize>>>,
        }
        impl Component for ShrinkScope {
            type Props = ShrinkScopeProps;
            fn render(props: &ShrinkScopeProps, hooks: &mut Hooks) -> Element {
                let scope = hooks.use_focus_scope();
                let item_count = hooks.use_state(|| 5usize);
                let ic = item_count.clone();
                hooks.use_input(move |ev, _ctx| {
                    if matches!(ev.code, KeyCode::Char('s')) {
                        ic.set(2);
                    }
                });
                let items = (0..item_count.get()).map(|i| format!("item{i}")).collect();
                let last = props.last.clone();
                Element::provider(
                    scope,
                    vec![Element::component::<Select>(SelectProps {
                        items,
                        selected: 4,
                        on_change: Some(Callback::new(move |i| last.set(Some(i)))),
                    })],
                )
            }
        }
        let props = ShrinkScopeProps {
            last: Rc::new(Cell::new(None)),
        };
        let mut t =
            TestTerminal::new(10, 5, Element::component::<ShrinkScope>(props.clone())).unwrap();

        t.send_key(KeyCode::Char('s')).unwrap();

        // If the cursor correctly re-clamped to index 1 (the last valid item
        // among 2), Down wraps to 0. If it stayed stuck at the stale 4, the
        // un-modulo'd `Up` handler would compute `4 - 1 == 3`, still out of
        // range for a 2-item list — so use Down here, which does modulo and
        // makes the clamped-vs-stale outcomes disagree (0 vs 1).
        t.send_key(KeyCode::Down).unwrap();
        assert_eq!(
            props.last.get(),
            Some(0),
            "selected should have re-clamped to the last valid index (1) and Down should wrap to 0"
        );
    }

    #[tokio::test]
    async fn all_items_render() {
        let props = ScopeProps::default();
        let t = TestTerminal::new(10, 3, Element::component::<Scope>(props.clone())).unwrap();
        let out = t.frame_text();
        assert!(out.contains('a') && out.contains('b') && out.contains('c'));
    }
}
