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

    /// [`Hooks::use_future`]'s deps-keyed sibling: spawn `make()` after
    /// commit, and whenever `deps` changes between renders abort the current
    /// task and spawn a fresh one. The task is also aborted on unmount.
    /// Composite over [`Hooks::use_effect`], so the spawn/abort timing
    /// follows effect timing (after commit, cleanup-before-rerun).
    ///
    /// Use this instead of `use_future` when the work is a function of some
    /// input that can change — a timer keyed on a `Duration` prop, an
    /// animation driver keyed on its target — so stale work stops instead of
    /// racing the replacement. Communicate back through `State<T>` handles;
    /// panics inside the task are swallowed, same caveat as `use_future`.
    pub fn use_task<D, F, Fut>(&mut self, deps: D, make: F)
    where
        D: PartialEq + 'static,
        F: FnOnce() -> Fut + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.use_effect(deps, move || {
            let handle = tokio::spawn(make());
            move || handle.abort()
        });
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

    struct Keyed;
    #[derive(Clone, PartialEq, Default)]
    struct KeyedProps {
        key: Shared<Option<State<u32>>>,
        fired: Shared<Vec<u32>>,
    }
    impl Component for Keyed {
        type Props = KeyedProps;
        fn render(props: &KeyedProps, hooks: &mut Hooks) -> Element {
            let key = hooks.use_state(|| 0u32);
            *props.key.lock() = Some(key.clone());
            let k = key.get();
            let fired = props.fired.clone();
            hooks.use_task(k, move || async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                fired.lock().push(k);
            });
            Element::text(TextProps {
                content: k.to_string(),
                ..Default::default()
            })
        }
    }

    #[tokio::test(start_paused = true)]
    async fn use_task_restarts_on_deps_change_aborting_the_old_task() {
        let props = KeyedProps::default();
        let mut t = TestTerminal::new(10, 1, Element::component::<Keyed>(props.clone())).unwrap();
        // Change deps at 50ms, before the key=0 task's 100ms sleep elapses:
        // that task must be aborted, so only key=1 ever fires.
        tokio::time::sleep(Duration::from_millis(50)).await;
        props.key.lock().clone().unwrap().set(1);
        t.tick().await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        t.tick().await.unwrap();
        assert_eq!(*props.fired.lock(), vec![1]);
    }

    #[tokio::test(start_paused = true)]
    async fn use_task_with_stable_deps_spawns_exactly_once() {
        let props = KeyedProps::default();
        let mut t = TestTerminal::new(10, 1, Element::component::<Keyed>(props.clone())).unwrap();
        // Re-render with unchanged deps (same key value) — no respawn, so
        // the timer isn't reset and still fires exactly once at 100ms.
        let key = props.key.lock().clone().unwrap();
        tokio::time::sleep(Duration::from_millis(60)).await;
        key.set(0);
        t.tick().await.unwrap();
        tokio::time::sleep(Duration::from_millis(60)).await;
        t.tick().await.unwrap();
        assert_eq!(*props.fired.lock(), vec![0]);
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
