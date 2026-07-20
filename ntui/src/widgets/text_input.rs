use std::time::Duration;

use crate::component::Component;
use crate::element::Element;
use crate::hooks::Hooks;
use crate::hooks::input::KeyCode;
use crate::props::{TextProps, ViewProps};
use crate::widgets::callback::Callback;

/// A focusable single-line text field. `value` is the source of truth — this
/// widget holds no text of its own, only a cursor-blink phase; the caller
/// applies `on_change` back into whatever state feeds `value`, the same
/// controlled pattern as [`crate::widgets::Checkbox`]/[`crate::widgets::Select`].
#[derive(Clone, PartialEq, Default)]
pub struct TextInputProps {
    pub value: String,
    /// Shown, dimmed, in place of an empty and unfocused `value`.
    pub placeholder: String,
    pub on_change: Option<Callback<String>>,
    /// Called with `value` when Enter is pressed while focused.
    pub on_submit: Option<Callback<String>>,
}

pub struct TextInput;
impl Component for TextInput {
    type Props = TextInputProps;
    fn render(props: &TextInputProps, hooks: &mut Hooks) -> Element {
        let theme = hooks.use_theme();
        let focus = hooks.use_focusable();
        let is_focused = focus.is_focused();

        let blink = hooks.use_state(|| true);
        let b = blink.clone();
        hooks.use_interval(Duration::from_millis(500), move || {
            b.update(|v| *v = !*v);
        });

        let value = props.value.clone();
        let on_change = props.on_change.clone();
        let on_submit = props.on_submit.clone();
        hooks.use_input(move |ev, ctx| {
            if !is_focused {
                return;
            }
            match ev.code {
                KeyCode::Char(c) => {
                    let mut s = value.clone();
                    s.push(c);
                    if let Some(cb) = &on_change {
                        cb.call(s);
                    }
                    ctx.stop_propagation();
                }
                KeyCode::Backspace => {
                    let mut s = value.clone();
                    s.pop();
                    if let Some(cb) = &on_change {
                        cb.call(s);
                    }
                    ctx.stop_propagation();
                }
                KeyCode::Enter => {
                    if let Some(cb) = &on_submit {
                        cb.call(value.clone());
                    }
                    ctx.stop_propagation();
                }
                _ => {}
            }
        });

        let (content, color) = if is_focused {
            let cursor = if blink.get() { "▌" } else { " " };
            (format!("{}{cursor}", props.value), theme.foreground)
        } else if props.value.is_empty() {
            (props.placeholder.clone(), theme.muted)
        } else {
            (props.value.clone(), theme.foreground)
        };

        Element::view(
            ViewProps {
                border_style: theme.border_style,
                border_color: if is_focused {
                    theme.accent
                } else {
                    theme.border
                },
                ..Default::default()
            },
            vec![Element::text(TextProps {
                content,
                color,
                ..Default::default()
            })],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::input::KeyCode;
    use crate::hooks::state::State;
    use crate::test_util::Shared;
    use crate::testing::TestTerminal;

    struct Scope;
    #[derive(Clone, PartialEq, Default)]
    struct ScopeProps {
        value: Shared<Option<State<String>>>,
        submitted: Shared<Option<String>>,
    }
    impl Component for Scope {
        type Props = ScopeProps;
        fn render(props: &ScopeProps, hooks: &mut Hooks) -> Element {
            let scope = hooks.use_focus_scope();
            let v = hooks.use_state(String::new);
            *props.value.lock() = Some(v.clone());
            let v_change = v.clone();
            let submitted = props.submitted.clone();
            Element::provider(
                scope,
                vec![Element::component::<TextInput>(TextInputProps {
                    value: v.get(),
                    placeholder: "type here".into(),
                    on_change: Some(Callback::new(move |s| v_change.set(s))),
                    on_submit: Some(Callback::new(move |s| *submitted.lock() = Some(s))),
                })],
            )
        }
    }

    #[tokio::test]
    async fn typed_characters_accumulate_in_value() {
        let props = ScopeProps::default();
        let mut t = TestTerminal::new(20, 3, Element::component::<Scope>(props.clone())).unwrap();
        t.send_key(KeyCode::Char('h')).unwrap();
        t.send_key(KeyCode::Char('i')).unwrap();
        assert_eq!(props.value.lock().clone().unwrap().get(), "hi");
        assert!(t.frame_text().contains("hi"));
    }

    #[tokio::test]
    async fn backspace_removes_the_last_character() {
        let props = ScopeProps::default();
        let mut t = TestTerminal::new(20, 3, Element::component::<Scope>(props.clone())).unwrap();
        t.send_key(KeyCode::Char('h')).unwrap();
        t.send_key(KeyCode::Char('i')).unwrap();
        t.send_key(KeyCode::Backspace).unwrap();
        assert_eq!(props.value.lock().clone().unwrap().get(), "h");
    }

    #[tokio::test]
    async fn enter_submits_the_current_value() {
        let props = ScopeProps::default();
        let mut t = TestTerminal::new(20, 3, Element::component::<Scope>(props.clone())).unwrap();
        t.send_key(KeyCode::Char('h')).unwrap();
        t.send_key(KeyCode::Enter).unwrap();
        assert_eq!(props.submitted.lock().clone(), Some("h".to_string()));
    }

    #[tokio::test]
    async fn empty_and_unfocused_shows_the_placeholder() {
        // No focus scope at all: TextInput is never focused.
        let t = TestTerminal::new(
            20,
            3,
            Element::component::<TextInput>(TextInputProps {
                placeholder: "type here".into(),
                ..Default::default()
            }),
        )
        .unwrap();
        assert!(t.frame_text().contains("type here"));
    }

    #[tokio::test(start_paused = true)]
    async fn cursor_blinks_on_a_500ms_interval_while_focused() {
        let props = ScopeProps::default();
        // Scope's single TextInput is the first (only) focusable registered,
        // so it's focused by default (see focus.rs::register).
        let mut t = TestTerminal::new(20, 3, Element::component::<Scope>(props)).unwrap();
        assert!(
            t.frame_text().contains('▌'),
            "cursor glyph should be visible right after mount"
        );

        tokio::time::sleep(Duration::from_millis(600)).await;
        t.tick().await.unwrap();
        assert!(
            !t.frame_text().contains('▌'),
            "cursor glyph should have toggled off after one blink interval"
        );
    }
}
