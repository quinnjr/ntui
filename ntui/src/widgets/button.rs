use crate::component::Component;
use crate::element::Element;
use crate::hooks::Hooks;
use crate::hooks::input::KeyCode;
use crate::props::{TextProps, ViewProps};
use crate::style::Weight;
use crate::widgets::callback::Callback;

/// A focusable push button. Requires an enclosing `Hooks::use_focus_scope`
/// to receive focus at all (see `ntui::widgets::focus`); with none, it
/// renders unfocused and `on_press` is only reachable by mouse — which ntui
/// doesn't support — so it becomes effectively unreachable. Always use
/// `Button` inside a focus scope.
#[derive(Clone, PartialEq, Default)]
pub struct ButtonProps {
    pub label: String,
    /// Called when Enter or Space is pressed while this button is focused.
    pub on_press: Option<Callback>,
}

pub struct Button;
impl Component for Button {
    type Props = ButtonProps;
    fn render(props: &ButtonProps, hooks: &mut Hooks) -> Element {
        let theme = hooks.use_theme();
        let focus = hooks.use_focusable();
        let is_focused = focus.is_focused();

        let on_press = props.on_press.clone();
        hooks.use_input(move |ev, ctx| {
            if !is_focused {
                return;
            }
            if matches!(ev.code, KeyCode::Enter | KeyCode::Char(' ')) {
                if let Some(cb) = &on_press {
                    cb.call(());
                }
                ctx.stop_propagation();
            }
        });

        let (bg, fg) = if is_focused {
            (theme.accent, theme.surface)
        } else {
            (theme.surface, theme.foreground)
        };
        Element::view(
            ViewProps {
                padding: 1,
                background: bg,
                border_style: theme.border_style,
                border_color: if is_focused {
                    theme.accent
                } else {
                    theme.border
                },
                ..Default::default()
            },
            vec![Element::text(TextProps {
                content: props.label.clone(),
                color: fg,
                weight: Weight::Bold,
                ..Default::default()
            })],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::Element;
    use crate::hooks::input::KeyCode;
    use crate::testing::TestTerminal;
    use std::cell::Cell;
    use std::rc::Rc;

    struct Scope;
    #[derive(Clone, PartialEq, Default)]
    struct ScopeProps {
        pressed: Rc<Cell<u32>>,
    }
    impl Component for Scope {
        type Props = ScopeProps;
        fn render(props: &ScopeProps, hooks: &mut Hooks) -> Element {
            let scope = hooks.use_focus_scope();
            let pressed = props.pressed.clone();
            Element::provider(
                scope,
                vec![Element::component::<Button>(ButtonProps {
                    label: "Go".into(),
                    on_press: Some(Callback::new(move |()| pressed.set(pressed.get() + 1))),
                })],
            )
        }
    }

    #[tokio::test]
    async fn enter_presses_the_focused_button() {
        let props = ScopeProps::default();
        let mut t = TestTerminal::new(12, 5, Element::component::<Scope>(props.clone())).unwrap();
        assert!(t.frame_text().contains("Go"));
        t.send_key(KeyCode::Enter).unwrap();
        assert_eq!(props.pressed.get(), 1);
    }

    #[tokio::test]
    async fn space_also_presses_the_button() {
        let props = ScopeProps::default();
        let mut t = TestTerminal::new(12, 5, Element::component::<Scope>(props.clone())).unwrap();
        t.send_key(KeyCode::Char(' ')).unwrap();
        assert_eq!(props.pressed.get(), 1);
    }

    #[tokio::test]
    async fn without_a_focus_scope_enter_does_nothing() {
        let props = ScopeProps::default();
        let mut t = TestTerminal::new(
            12,
            5,
            Element::component::<Button>(ButtonProps {
                label: "Go".into(),
                on_press: Some(Callback::new({
                    let p = props.pressed.clone();
                    move |()| p.set(p.get() + 1)
                })),
            }),
        )
        .unwrap();
        t.send_key(KeyCode::Enter).unwrap();
        assert_eq!(props.pressed.get(), 0);
    }
}
