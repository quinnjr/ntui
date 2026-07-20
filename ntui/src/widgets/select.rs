use crate::component::Component;
use crate::element::Element;
use crate::hooks::Hooks;
use crate::hooks::input::KeyCode;
use crate::props::{FlexDirection, TextProps, ViewProps};
use crate::style::Color;
use crate::widgets::callback::Callback;

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

        let cursor = hooks.use_state(|| props.selected.min(len.saturating_sub(1)));
        let sync = cursor.clone();
        let selected_prop = props.selected;
        hooks.use_effect(selected_prop, move || {
            sync.set(selected_prop.min(len.saturating_sub(1)));
        });

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
    async fn all_items_render() {
        let props = ScopeProps::default();
        let t = TestTerminal::new(10, 3, Element::component::<Scope>(props.clone())).unwrap();
        let out = t.frame_text();
        assert!(out.contains('a') && out.contains('b') && out.contains('c'));
    }
}
