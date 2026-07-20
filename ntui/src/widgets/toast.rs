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

        // Hooks run unconditionally (hook identity is call order — gating
        // them on `duration.is_some()` would panic if a caller ever flipped
        // `duration` between `Some` and `None` across renders). Keying the
        // task on `duration` restarts the countdown if it changes and stops
        // it entirely on `None`; a one-shot sleep (not an interval) means no
        // timer keeps running after the toast has fired.
        let fired = hooks.use_state(|| false);
        let f = fired.clone();
        let duration = props.duration;
        hooks.use_task(duration, move || async move {
            // Runs on a spawned task, so only touch `Send` state here —
            // never `on_dismiss` (`Callback` wraps a non-`Send` `Rc`).
            let Some(d) = duration else { return };
            tokio::time::sleep(d).await;
            f.set(true);
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
    async fn flipping_duration_to_none_cancels_the_timer_without_panicking() {
        use crate::hooks::state::State;
        use crate::test_util::Shared;

        // Regression: hooks must not be gated on `duration.is_some()` — a
        // caller flipping it Some -> None across renders used to shrink the
        // hook count and panic. It must instead just cancel the countdown.
        struct Host;
        #[derive(Clone, PartialEq, Default)]
        struct HostProps {
            timed: Shared<Option<State<bool>>>,
            dismissed: Rc<Cell<u32>>,
        }
        impl crate::component::Component for Host {
            type Props = HostProps;
            fn render(props: &HostProps, hooks: &mut crate::hooks::Hooks) -> Element {
                let timed = hooks.use_state(|| true);
                *props.timed.lock() = Some(timed.clone());
                let d = props.dismissed.clone();
                Element::component::<Toast>(ToastProps {
                    message: "Saved.".into(),
                    duration: timed.get().then(|| Duration::from_millis(50)),
                    on_dismiss: Some(Callback::new(move |()| d.set(d.get() + 1))),
                    ..Default::default()
                })
            }
        }

        let props = HostProps::default();
        let mut t = TestTerminal::new(30, 10, Element::component::<Host>(props.clone())).unwrap();
        props.timed.lock().clone().unwrap().set(false); // Some -> None before it fires
        t.tick().await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        t.tick().await.unwrap();
        assert_eq!(props.dismissed.get(), 0, "cancelled timer must never fire");
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
