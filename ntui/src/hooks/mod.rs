use crate::fiber::FiberId;

pub mod app;
pub mod context;
pub mod effect;
pub mod input;
pub mod scroll;
pub mod scrollback;
pub mod state;
pub mod task;

pub(crate) enum HookSlot {
    State(Box<dyn std::any::Any>), // holds a State<T>
    Effect(effect::EffectSlot),
    Input(input::InputHandler),
    Task(tokio::task::JoinHandle<()>),
}

impl HookSlot {
    /// Runs teardown for this slot (cleanups, task aborts). Arms added per hook task.
    pub(crate) fn unmount(self) {
        match self {
            HookSlot::State(_) => {}
            HookSlot::Effect(mut e) => {
                if let Some(c) = e.cleanup.take() {
                    c();
                }
            }
            HookSlot::Input(_) => {}
            HookSlot::Task(handle) => handle.abort(),
        }
    }
}

#[derive(Debug)]
pub(crate) enum Wake {
    Dirty(FiberId),
    Redraw, // full re-render from the root
    Exit,
}

#[derive(Clone)]
pub(crate) struct RuntimeHandle {
    pub wake: tokio::sync::mpsc::UnboundedSender<Wake>,
    pub size: std::sync::Arc<std::sync::Mutex<(u16, u16)>>,
    /// Queue of elements to commit to terminal scrollback (inline mode only).
    /// `Rc`/`RefCell` because it never crosses threads — the render loop is
    /// single-threaded and this handle is only touched during renders.
    pub scrollback: std::rc::Rc<std::cell::RefCell<Vec<crate::element::Element>>>,
}

impl RuntimeHandle {
    #[cfg(test)]
    pub(crate) fn test_handle() -> (Self, tokio::sync::mpsc::UnboundedReceiver<Wake>) {
        let (wake, rx) = tokio::sync::mpsc::unbounded_channel();
        (
            RuntimeHandle {
                wake,
                size: std::sync::Arc::new(std::sync::Mutex::new((80, 24))),
                scrollback: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            },
            rx,
        )
    }
}

/// Handle passed to every component render. Hook identity = call order.
pub struct Hooks<'a> {
    pub(crate) slots: &'a mut Vec<HookSlot>,
    pub(crate) cursor: usize,
    pub(crate) component_name: &'static str,
    pub(crate) fiber_id: FiberId,
    pub(crate) runtime: RuntimeHandle,
    pub(crate) first_render: bool,
    pub(crate) context: std::rc::Rc<crate::fiber::ContextMap>,
}

impl<'a> Hooks<'a> {
    pub(crate) fn new(
        slots: &'a mut Vec<HookSlot>,
        component_name: &'static str,
        fiber_id: FiberId,
        runtime: RuntimeHandle,
        first_render: bool,
        context: std::rc::Rc<crate::fiber::ContextMap>,
    ) -> Self {
        Hooks {
            slots,
            cursor: 0,
            component_name,
            fiber_id,
            runtime,
            first_render,
            context,
        }
    }

    /// Advance the hook cursor; create the slot on first render.
    pub(crate) fn next_slot(&mut self, create: impl FnOnce() -> HookSlot) -> &mut HookSlot {
        if self.cursor == self.slots.len() {
            if self.first_render {
                self.slots.push(create());
            } else {
                panic!(
                    "ntui: {}: more hooks called than in the previous render (slot {}) — hooks must run unconditionally in the same order every render",
                    self.component_name, self.cursor
                );
            }
        }
        let i = self.cursor;
        self.cursor += 1;
        &mut self.slots[i]
    }

    pub(crate) fn hook_mismatch(&self, expected: &'static str) -> ! {
        panic!(
            "ntui: hook order violation in {}: slot {} is not {} — hooks must run in the same order every render",
            self.component_name,
            self.cursor - 1,
            expected
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::component::Component;
    use crate::element::Element;
    use crate::fiber::FiberTree;
    use crate::hooks::{Hooks, RuntimeHandle};
    use crate::props::ViewProps;
    use crate::test_util::Shared;

    type RenderFn = fn(&Shared<u8>, &mut Hooks) -> Element;

    // Drives a component through two renders, flipping `phase` between them so a
    // component can call different hooks on the second render (a violation).
    fn drive(render: RenderFn, phase: Shared<u8>) {
        #[derive(Clone, PartialEq, Default)]
        struct P {
            phase: Shared<u8>,
            f: Shared<Option<RenderFn>>,
        }
        struct C;
        impl Component for C {
            type Props = P;
            fn render(props: &P, hooks: &mut Hooks) -> Element {
                let f = (*props.f.lock()).unwrap();
                f(&props.phase, hooks)
            }
        }
        let (rt, rx) = RuntimeHandle::test_handle();
        std::mem::forget(rx);
        let mut tree = FiberTree::new();
        let props = P {
            phase: phase.clone(),
            f: Shared::default(),
        };
        *props.f.lock() = Some(render);
        let root = tree.mount_root(Element::component::<C>(props.clone()), &rt);
        *phase.lock() = 1; // second render takes the other branch
        tree.render_fiber(root, &rt);
    }

    #[test]
    #[should_panic(expected = "more hooks called than in the previous render")]
    fn extra_hook_on_rerender_panics() {
        drive(
            |phase, hooks| {
                hooks.use_state(|| 0);
                if *phase.lock() == 1 {
                    hooks.use_state(|| 0); // a second hook only on rerender
                }
                Element::view(ViewProps::default(), vec![])
            },
            Shared::default(),
        );
    }

    #[test]
    #[should_panic(expected = "hooks this render but")]
    fn fewer_hooks_on_rerender_panics() {
        drive(
            |phase, hooks| {
                hooks.use_state(|| 0);
                if *phase.lock() == 0 {
                    hooks.use_state(|| 0); // present only on the first render
                }
                Element::view(ViewProps::default(), vec![])
            },
            Shared::default(),
        );
    }

    // Each swap replaces the slot-0 hook type on rerender, tripping the
    // `let HookSlot::X = slot else { hook_mismatch(..) }` arm of the named hook.
    #[test]
    #[should_panic(expected = "hook order violation")]
    fn use_effect_slot_type_mismatch_panics() {
        drive(
            |phase, hooks| {
                if *phase.lock() == 0 {
                    hooks.use_state(|| 0);
                } else {
                    hooks.use_effect((), || {});
                }
                Element::view(ViewProps::default(), vec![])
            },
            Shared::default(),
        );
    }

    #[test]
    #[should_panic(expected = "hook order violation")]
    fn use_state_slot_type_mismatch_panics() {
        drive(
            |phase, hooks| {
                if *phase.lock() == 0 {
                    hooks.use_effect((), || {});
                } else {
                    hooks.use_state(|| 0);
                }
                Element::view(ViewProps::default(), vec![])
            },
            Shared::default(),
        );
    }

    #[test]
    #[should_panic(expected = "hook order violation")]
    fn use_input_slot_type_mismatch_panics() {
        drive(
            |phase, hooks| {
                if *phase.lock() == 0 {
                    hooks.use_state(|| 0);
                } else {
                    hooks.use_input(|_, _| {});
                }
                Element::view(ViewProps::default(), vec![])
            },
            Shared::default(),
        );
    }

    #[test]
    #[should_panic(expected = "hook order violation")]
    fn use_future_slot_type_mismatch_panics() {
        drive(
            |phase, hooks| {
                if *phase.lock() == 0 {
                    hooks.use_state(|| 0);
                } else {
                    hooks.use_future(|| async {});
                }
                Element::view(ViewProps::default(), vec![])
            },
            Shared::default(),
        );
    }

    #[test]
    #[should_panic(expected = "use_state type changed")]
    fn use_state_type_change_panics() {
        drive(
            |phase, hooks| {
                if *phase.lock() == 0 {
                    hooks.use_state(|| 0i32);
                } else {
                    hooks.use_state(String::new);
                }
                Element::view(ViewProps::default(), vec![])
            },
            Shared::default(),
        );
    }

    #[test]
    #[should_panic(expected = "use_effect deps type changed")]
    fn use_effect_deps_type_change_panics() {
        drive(
            |phase, hooks| {
                if *phase.lock() == 0 {
                    hooks.use_effect(0i32, || {});
                } else {
                    hooks.use_effect(String::new(), || {});
                }
                Element::view(ViewProps::default(), vec![])
            },
            Shared::default(),
        );
    }
}
