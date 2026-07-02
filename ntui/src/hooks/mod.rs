// Hook plumbing is wired end-to-end once the runtime (later tasks) drives the
// fiber tree; until then several items here are only exercised by unit tests.
#![allow(dead_code)]

use crate::fiber::FiberId;

pub(crate) enum HookSlot {} // variants arrive with each hook

impl HookSlot {
    /// Runs teardown for this slot (cleanups, task aborts). Arms added per hook task.
    pub(crate) fn unmount(self) {
        match self {}
    }
}

#[derive(Debug)]
pub(crate) enum Wake {
    Dirty(FiberId),
    Exit,
}

#[derive(Clone)]
pub(crate) struct RuntimeHandle {
    pub wake: tokio::sync::mpsc::UnboundedSender<Wake>,
}

impl RuntimeHandle {
    #[cfg(test)]
    pub(crate) fn test_handle() -> (Self, tokio::sync::mpsc::UnboundedReceiver<Wake>) {
        let (wake, rx) = tokio::sync::mpsc::unbounded_channel();
        (RuntimeHandle { wake }, rx)
    }
}

/// Handle passed to every component render. Hook identity = call order.
pub struct Hooks<'a> {
    pub(crate) slots: &'a mut Vec<HookSlot>,
    pub(crate) cursor: usize,
    pub(crate) component_name: &'static str,
    pub(crate) fiber_id: FiberId,
    pub(crate) runtime: RuntimeHandle,
}

impl<'a> Hooks<'a> {
    pub(crate) fn new(
        slots: &'a mut Vec<HookSlot>,
        component_name: &'static str,
        fiber_id: FiberId,
        runtime: RuntimeHandle,
    ) -> Self {
        Hooks {
            slots,
            cursor: 0,
            component_name,
            fiber_id,
            runtime,
        }
    }

    /// Advance the hook cursor; create the slot on first render.
    #[allow(unreachable_code)]
    pub(crate) fn next_slot(&mut self, create: impl FnOnce() -> HookSlot) -> &mut HookSlot {
        if self.cursor == self.slots.len() {
            self.slots.push(create());
        }
        let i = self.cursor;
        self.cursor += 1;
        self.slots.get_mut(i).unwrap_or_else(|| {
            panic!(
                "ntui: {}: more hooks called than previous render (slot {})",
                self.component_name, i
            )
        })
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
