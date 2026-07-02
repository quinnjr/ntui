use crate::fiber::FiberId;

pub mod app;
pub mod effect;
pub mod input;
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
}

impl RuntimeHandle {
    #[cfg(test)]
    pub(crate) fn test_handle() -> (Self, tokio::sync::mpsc::UnboundedReceiver<Wake>) {
        let (wake, rx) = tokio::sync::mpsc::unbounded_channel();
        (
            RuntimeHandle {
                wake,
                size: std::sync::Arc::new(std::sync::Mutex::new((80, 24))),
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
}

impl<'a> Hooks<'a> {
    pub(crate) fn new(
        slots: &'a mut Vec<HookSlot>,
        component_name: &'static str,
        fiber_id: FiberId,
        runtime: RuntimeHandle,
        first_render: bool,
    ) -> Self {
        Hooks {
            slots,
            cursor: 0,
            component_name,
            fiber_id,
            runtime,
            first_render,
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
