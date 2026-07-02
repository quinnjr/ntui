use std::future::Future;

use futures::{Stream, StreamExt};

use crate::hooks::{HookSlot, Hooks};

impl<'a> Hooks<'a> {
    /// Spawn `make()` once, on mount. The task is aborted when the component
    /// unmounts. Communicate back through `State<T>` handles (they are Send).
    /// Panics inside the task are currently swallowed (the JoinHandle is never joined);
    /// propagate errors through `State` values instead.
    pub fn use_future<F, Fut>(&mut self, make: F)
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let mut make = Some(make);
        let slot = self.next_slot(move || HookSlot::Task(tokio::spawn((make.take().unwrap())())));
        let HookSlot::Task(_) = slot else {
            self.hook_mismatch("use_future")
        };
    }

    /// Sugar over use_future: consume a stream, calling `on_item` per item
    /// (on the task — capture State handles, not references).
    pub fn use_stream<S, F>(&mut self, make: impl FnOnce() -> S + Send + 'static, mut on_item: F)
    where
        S: Stream + Send + 'static,
        S::Item: Send,
        F: FnMut(S::Item) + Send + 'static,
    {
        self.use_future(move || async move {
            let mut s = std::pin::pin!(make());
            while let Some(item) = s.next().await {
                on_item(item);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::component::Component;
    use crate::element::Element;
    use crate::hooks::Hooks;
    use crate::hooks::state::State;
    use crate::props::TextProps;
    use crate::test_util::Shared;
    use crate::testing::TestTerminal;

    struct Timer;
    #[derive(Clone, PartialEq, Default)]
    struct TimerProps {
        fired: Shared<bool>,
    }
    impl Component for Timer {
        type Props = TimerProps;
        fn render(props: &TimerProps, hooks: &mut Hooks) -> Element {
            let done = hooks.use_state(|| false);
            let d = done.clone();
            let fired = props.fired.clone();
            hooks.use_future(move || async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                *fired.lock() = true;
                d.set(true);
            });
            Element::text(TextProps {
                content: done.get().to_string(),
                ..Default::default()
            })
        }
    }

    struct Gate;
    #[derive(Clone, PartialEq, Default)]
    struct GateProps {
        fired: Shared<bool>,
        show: Shared<Option<State<bool>>>,
    }
    impl Component for Gate {
        type Props = GateProps;
        fn render(props: &GateProps, hooks: &mut Hooks) -> Element {
            let show = hooks.use_state(|| true);
            *props.show.lock() = Some(show.clone());
            if show.get() {
                Element::fragment(vec![Element::component::<Timer>(TimerProps {
                    fired: props.fired.clone(),
                })])
            } else {
                Element::fragment(vec![])
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn future_completes_and_sets_state() {
        let props = TimerProps::default();
        let mut t = TestTerminal::new(10, 1, Element::component::<Timer>(props.clone())).unwrap();
        assert!(t.frame_text().contains("false"));
        tokio::time::sleep(Duration::from_millis(150)).await; // paused clock auto-advances
        t.tick().await.unwrap();
        assert!(t.frame_text().contains("true"));
    }

    #[tokio::test(start_paused = true)]
    async fn unmount_aborts_task() {
        let props = GateProps::default();
        let mut t = TestTerminal::new(10, 1, Element::component::<Gate>(props.clone())).unwrap();
        props.show.lock().clone().unwrap().set(false); // unmount Timer before it fires
        t.tick().await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        t.tick().await.unwrap();
        assert!(!*props.fired.lock());
    }

    struct Streamer;
    #[derive(Clone, PartialEq, Default)]
    struct StreamerProps {
        items: Shared<Vec<i32>>,
    }
    impl Component for Streamer {
        type Props = StreamerProps;
        fn render(props: &StreamerProps, hooks: &mut Hooks) -> Element {
            let items = props.items.clone();
            hooks.use_stream(
                || futures::stream::iter(vec![1, 2, 3]),
                move |item| items.lock().push(item),
            );
            Element::text(TextProps {
                content: "s".into(),
                ..Default::default()
            })
        }
    }

    #[tokio::test(start_paused = true)]
    async fn use_stream_forwards_all_items_in_order() {
        let props = StreamerProps::default();
        let mut t =
            TestTerminal::new(10, 1, Element::component::<Streamer>(props.clone())).unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await; // let the spawned task drain
        t.tick().await.unwrap();
        assert_eq!(*props.items.lock(), vec![1, 2, 3]);
    }

    struct SlowStreamer;
    #[derive(Clone, PartialEq, Default)]
    struct SlowStreamerProps {
        items: Shared<Vec<i32>>,
    }
    impl Component for SlowStreamer {
        type Props = SlowStreamerProps;
        fn render(props: &SlowStreamerProps, hooks: &mut Hooks) -> Element {
            let items = props.items.clone();
            hooks.use_stream(
                || {
                    use futures::StreamExt;
                    futures::stream::iter(vec![1, 2, 3]).then(|i| async move {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        i
                    })
                },
                move |item| items.lock().push(item),
            );
            Element::text(TextProps {
                content: "s".into(),
                ..Default::default()
            })
        }
    }

    struct StreamGate;
    #[derive(Clone, PartialEq, Default)]
    struct StreamGateProps {
        items: Shared<Vec<i32>>,
        show: Shared<Option<State<bool>>>,
    }
    impl Component for StreamGate {
        type Props = StreamGateProps;
        fn render(props: &StreamGateProps, hooks: &mut Hooks) -> Element {
            let show = hooks.use_state(|| true);
            *props.show.lock() = Some(show.clone());
            if show.get() {
                Element::fragment(vec![Element::component::<SlowStreamer>(
                    SlowStreamerProps {
                        items: props.items.clone(),
                    },
                )])
            } else {
                Element::fragment(vec![])
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn use_stream_aborts_on_unmount() {
        let props = StreamGateProps::default();
        let mut t =
            TestTerminal::new(10, 1, Element::component::<StreamGate>(props.clone())).unwrap();
        // Let the stream start draining (first item lands around 100ms) but not finish.
        tokio::time::sleep(Duration::from_millis(150)).await;
        t.tick().await.unwrap();
        props.show.lock().clone().unwrap().set(false); // unmount mid-drain -> abort task
        t.tick().await.unwrap();
        let at_unmount = props.items.lock().len();
        assert!(at_unmount < 3, "stream should still be draining at unmount");
        // Wait well past when the remaining items would have arrived.
        tokio::time::sleep(Duration::from_millis(500)).await;
        t.tick().await.unwrap();
        assert_eq!(
            props.items.lock().len(),
            at_unmount,
            "no further items after the aborted stream"
        );
    }
}
