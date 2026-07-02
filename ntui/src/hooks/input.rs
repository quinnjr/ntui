use std::cell::RefCell;
use std::rc::Rc;

pub use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::hooks::{HookSlot, Hooks};

/// Passed to `use_input` handlers alongside each key event, letting a
/// handler stop the event from reaching ancestor handlers.
pub struct InputCtx {
    pub(crate) stopped: bool,
}

impl InputCtx {
    /// Prevents this key event from bubbling to handlers further up the
    /// tree (dispatch is deepest-first).
    pub fn stop_propagation(&mut self) {
        self.stopped = true;
    }
}

pub(crate) type InputHandler = Rc<RefCell<dyn FnMut(KeyEvent, &mut InputCtx)>>;

impl<'a> Hooks<'a> {
    /// The handler is replaced on every render so it always captures fresh state.
    pub fn use_input(&mut self, handler: impl FnMut(KeyEvent, &mut InputCtx) + 'static) {
        let h: InputHandler = Rc::new(RefCell::new(handler));
        let slot = self.next_slot(|| HookSlot::Input(h.clone()));
        let HookSlot::Input(existing) = slot else {
            self.hook_mismatch("use_input")
        };
        *existing = h;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
    use crate::element::Element;
    use crate::hooks::Hooks;
    use crate::hooks::state::State;
    use crate::props::TextProps;
    use crate::test_util::Shared;
    use crate::testing::TestTerminal;

    struct Inner;
    #[derive(Clone, PartialEq, Default)]
    struct InnerProps {
        log: Shared<Vec<String>>,
        stop: bool,
    }
    impl Component for Inner {
        type Props = InnerProps;
        fn render(props: &InnerProps, hooks: &mut Hooks) -> Element {
            let log = props.log.clone();
            let stop = props.stop;
            hooks.use_input(move |ev, ctx| {
                log.lock().push(format!("inner:{:?}", ev.code));
                if stop {
                    ctx.stop_propagation();
                }
            });
            Element::text(TextProps {
                content: "inner".into(),
                ..Default::default()
            })
        }
    }

    struct Outer;
    #[derive(Clone, PartialEq, Default)]
    struct OuterProps {
        log: Shared<Vec<String>>,
        stop_inner: bool,
    }
    impl Component for Outer {
        type Props = OuterProps;
        fn render(props: &OuterProps, hooks: &mut Hooks) -> Element {
            let log = props.log.clone();
            hooks.use_input(move |ev, _| log.lock().push(format!("outer:{:?}", ev.code)));
            Element::fragment(vec![Element::component::<Inner>(InnerProps {
                log: props.log.clone(),
                stop: props.stop_inner,
            })])
        }
    }

    #[tokio::test]
    async fn deepest_handler_runs_first_then_bubbles() {
        let log = Shared::default();
        let mut t = TestTerminal::new(
            10,
            2,
            Element::component::<Outer>(OuterProps {
                log: log.clone(),
                stop_inner: false,
            }),
        )
        .unwrap();
        t.send_key(KeyCode::Char('x')).unwrap();
        assert_eq!(*log.lock(), vec!["inner:Char('x')", "outer:Char('x')"]);
    }

    #[tokio::test]
    async fn stop_propagation_blocks_ancestors() {
        let log = Shared::default();
        let mut t = TestTerminal::new(
            10,
            2,
            Element::component::<Outer>(OuterProps {
                log: log.clone(),
                stop_inner: true,
            }),
        )
        .unwrap();
        t.send_key(KeyCode::Char('x')).unwrap();
        assert_eq!(*log.lock(), vec!["inner:Char('x')"]);
    }

    struct Fresh;
    #[derive(Clone, PartialEq, Default)]
    struct FreshProps {
        seen: Shared<Vec<i32>>,
        handle: Shared<Option<State<i32>>>,
    }
    impl Component for Fresh {
        type Props = FreshProps;
        fn render(props: &FreshProps, hooks: &mut Hooks) -> Element {
            let n = hooks.use_state(|| 0);
            *props.handle.lock() = Some(n.clone());
            let cur = n.get();
            let seen = props.seen.clone();
            // Captures `cur` by value: a stale handler slot would keep recording 0.
            hooks.use_input(move |_ev, _| seen.lock().push(cur));
            Element::text(TextProps {
                content: cur.to_string(),
                ..Default::default()
            })
        }
    }

    #[tokio::test]
    async fn use_input_handler_captures_fresh_state_after_rerender() {
        let props = FreshProps::default();
        let mut t = TestTerminal::new(10, 1, Element::component::<Fresh>(props.clone())).unwrap();

        t.send_key(KeyCode::Char('a')).unwrap();
        assert_eq!(*props.seen.lock(), vec![0]); // handler saw the initial state

        props.handle.lock().clone().unwrap().set(42); // re-render with new state
        t.tick().await.unwrap();
        t.send_key(KeyCode::Char('b')).unwrap();
        assert_eq!(*props.seen.lock(), vec![0, 42]); // handler slot was replaced, sees fresh value
    }
}
