use std::cell::RefCell;
use std::rc::Rc;

pub use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::hooks::{HookSlot, Hooks};

pub struct InputCtx {
    pub(crate) stopped: bool,
}

impl InputCtx {
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
}
