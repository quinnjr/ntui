use crate::component::Component;
use crate::element::Element;
use crate::hooks::Hooks;
use crate::hooks::input::KeyCode;
use crate::props::{FlexDirection, TextProps, ViewProps};
use crate::widgets::callback::Callback;

/// Shared input handling for [`Checkbox`] and [`Toggle`]: while focused,
/// Enter/Space calls `on_change` with the flipped boolean and stops
/// propagation so the key doesn't also bubble to an ancestor handler.
fn use_toggle_input(
    hooks: &mut Hooks,
    is_focused: bool,
    checked: bool,
    on_change: Option<Callback<bool>>,
) {
    hooks.use_input(move |ev, ctx| {
        if !is_focused {
            return;
        }
        if matches!(ev.code, KeyCode::Enter | KeyCode::Char(' ')) {
            if let Some(cb) = &on_change {
                cb.call(!checked);
            }
            ctx.stop_propagation();
        }
    });
}

/// A focusable checkbox: Enter/Space toggles it and calls `on_change` with
/// the new state. `checked` is the source of truth — this widget doesn't
/// hold its own internal copy, so the caller owns the state.
#[derive(Clone, PartialEq, Default)]
pub struct CheckboxProps {
    pub label: String,
    pub checked: bool,
    pub on_change: Option<Callback<bool>>,
}

pub struct Checkbox;
impl Component for Checkbox {
    type Props = CheckboxProps;
    fn render(props: &CheckboxProps, hooks: &mut Hooks) -> Element {
        let theme = hooks.use_theme();
        let focus = hooks.use_focusable();
        let is_focused = focus.is_focused();

        use_toggle_input(hooks, is_focused, props.checked, props.on_change.clone());

        let mark = if props.checked { "◉" } else { "○" };
        let mark_color = if is_focused {
            theme.accent
        } else {
            theme.foreground
        };
        Element::view(
            ViewProps {
                flex_direction: FlexDirection::Row,
                gap: 1,
                ..Default::default()
            },
            vec![
                Element::text(TextProps {
                    content: mark.to_string(),
                    color: mark_color,
                    ..Default::default()
                }),
                Element::text(TextProps {
                    content: props.label.clone(),
                    color: theme.foreground,
                    ..Default::default()
                }),
            ],
        )
    }
}

/// A focusable on/off switch, functionally identical to [`Checkbox`] with a
/// pill-style rendering instead of a check mark — use whichever reads better
/// for the setting (a `Checkbox` for "select one of several", a `Toggle` for
/// a single on/off preference).
#[derive(Clone, PartialEq, Default)]
pub struct ToggleProps {
    pub label: String,
    pub on: bool,
    pub on_change: Option<Callback<bool>>,
}

pub struct Toggle;
impl Component for Toggle {
    type Props = ToggleProps;
    fn render(props: &ToggleProps, hooks: &mut Hooks) -> Element {
        let theme = hooks.use_theme();
        let focus = hooks.use_focusable();
        let is_focused = focus.is_focused();

        use_toggle_input(hooks, is_focused, props.on, props.on_change.clone());

        let pill = if props.on { "[ on]" } else { "[off]" };
        let pill_color = if props.on { theme.success } else { theme.muted };
        Element::view(
            ViewProps {
                flex_direction: FlexDirection::Row,
                gap: 1,
                ..Default::default()
            },
            vec![
                Element::text(TextProps {
                    content: pill.to_string(),
                    color: if is_focused { theme.accent } else { pill_color },
                    ..Default::default()
                }),
                Element::text(TextProps {
                    content: props.label.clone(),
                    color: theme.foreground,
                    ..Default::default()
                }),
            ],
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

    struct CheckScope;
    #[derive(Clone, PartialEq, Default)]
    struct CheckScopeProps {
        checked: bool,
        last: Rc<Cell<Option<bool>>>,
    }
    impl Component for CheckScope {
        type Props = CheckScopeProps;
        fn render(props: &CheckScopeProps, hooks: &mut Hooks) -> Element {
            let scope = hooks.use_focus_scope();
            let last = props.last.clone();
            Element::provider(
                scope,
                vec![Element::component::<Checkbox>(CheckboxProps {
                    label: "agree".into(),
                    checked: props.checked,
                    on_change: Some(Callback::new(move |v| last.set(Some(v)))),
                })],
            )
        }
    }

    #[tokio::test]
    async fn space_toggles_an_unchecked_box_on() {
        let props = CheckScopeProps::default();
        let mut t =
            TestTerminal::new(10, 3, Element::component::<CheckScope>(props.clone())).unwrap();
        assert!(t.frame_text().contains('○'));
        t.send_key(KeyCode::Char(' ')).unwrap();
        assert_eq!(props.last.get(), Some(true));
    }

    #[tokio::test]
    async fn enter_toggles_a_checked_box_off() {
        let props = CheckScopeProps {
            checked: true,
            ..Default::default()
        };
        let mut t =
            TestTerminal::new(10, 3, Element::component::<CheckScope>(props.clone())).unwrap();
        assert!(t.frame_text().contains('◉'));
        t.send_key(KeyCode::Enter).unwrap();
        assert_eq!(props.last.get(), Some(false));
    }

    struct ToggleScope;
    #[derive(Clone, PartialEq, Default)]
    struct ToggleScopeProps {
        last: Rc<Cell<Option<bool>>>,
    }
    impl Component for ToggleScope {
        type Props = ToggleScopeProps;
        fn render(props: &ToggleScopeProps, hooks: &mut Hooks) -> Element {
            let scope = hooks.use_focus_scope();
            let last = props.last.clone();
            Element::provider(
                scope,
                vec![Element::component::<Toggle>(ToggleProps {
                    label: "dark mode".into(),
                    on: false,
                    on_change: Some(Callback::new(move |v| last.set(Some(v)))),
                })],
            )
        }
    }

    #[tokio::test]
    async fn toggle_flips_and_shows_state() {
        let props = ToggleScopeProps::default();
        let mut t =
            TestTerminal::new(20, 3, Element::component::<ToggleScope>(props.clone())).unwrap();
        assert!(t.frame_text().contains("[off]"));
        t.send_key(KeyCode::Enter).unwrap();
        assert_eq!(props.last.get(), Some(true));
    }
}
