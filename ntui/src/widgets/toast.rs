use std::time::Duration;

use crate::component::Component;
use crate::element::Element;
use crate::hooks::Hooks;
use crate::props::{Anchor, TextProps, ViewProps};
use crate::widgets::callback::Callback;

/// A transient notification, anchored to a screen corner (bottom-right by
/// default), that calls `on_dismiss` once after `duration` — set `duration`
/// to `None` for a toast that stays until the caller removes it another way
/// (e.g. an explicit close action elsewhere in the UI).
#[derive(Clone, PartialEq)]
pub struct ToastProps {
    pub message: String,
    pub anchor: Anchor,
    pub duration: Option<Duration>,
    pub on_dismiss: Option<Callback>,
}

impl Default for ToastProps {
    fn default() -> Self {
        ToastProps {
            message: String::new(),
            anchor: Anchor::BottomRight,
            duration: Some(Duration::from_secs(3)),
            on_dismiss: None,
        }
    }
}

pub struct Toast;
impl Component for Toast {
    type Props = ToastProps;
    fn render(props: &ToastProps, hooks: &mut Hooks) -> Element {
        let theme = hooks.use_theme();

        if let Some(duration) = props.duration {
            let fired = hooks.use_state(|| false);
            let f = fired.clone();
            hooks.use_interval(duration, move || {
                // Runs on a spawned task, so only touch `Send` state here —
                // never `on_dismiss` (`Callback` wraps a non-`Send` `Rc`).
                // `use_interval` keeps ticking every `duration`; the `!f.get()`
                // guard means only the first tick actually flips it.
                if !f.get() {
                    f.set(true);
                }
            });
            // Runs on the render/effect-flush thread instead, where calling
            // the `Rc`-based callback is fine.
            let should_fire = fired.get();
            let on_dismiss = props.on_dismiss.clone();
            hooks.use_effect(should_fire, move || {
                if should_fire && let Some(cb) = &on_dismiss {
                    cb.call(());
                }
            });
        }

        Element::view(
            ViewProps {
                overlay: Some(props.anchor),
                padding: 1,
                background: theme.surface,
                border_style: theme.border_style,
                border_color: theme.accent,
                ..Default::default()
            },
            vec![Element::text(TextProps {
                content: props.message.clone(),
                color: theme.foreground,
                ..Default::default()
            })],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestTerminal;
    use std::cell::Cell;
    use std::rc::Rc;

    #[tokio::test]
    async fn shows_the_message() {
        let t = TestTerminal::new(
            30,
            10,
            Element::component::<Toast>(ToastProps {
                message: "Saved.".into(),
                duration: None,
                ..Default::default()
            }),
        )
        .unwrap();
        assert!(t.frame_text().contains("Saved."));
    }

    #[tokio::test(start_paused = true)]
    async fn dismisses_itself_once_after_the_duration() {
        let count = Rc::new(Cell::new(0u32));
        let c = count.clone();
        let mut t = TestTerminal::new(
            30,
            10,
            Element::component::<Toast>(ToastProps {
                message: "Saved.".into(),
                duration: Some(Duration::from_millis(50)),
                on_dismiss: Some(Callback::new(move |()| c.set(c.get() + 1))),
                ..Default::default()
            }),
        )
        .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        t.tick().await.unwrap();
        assert_eq!(count.get(), 1, "must dismiss exactly once, not repeatedly");
    }
}
