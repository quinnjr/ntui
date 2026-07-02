// Wired into the runtime in a later task; unused-outside-tests until then.
#![allow(dead_code)]

use crate::fiber::{FiberId, FiberKind, FiberTree};
use crate::hooks::{Hooks, RuntimeHandle};

impl FiberTree {
    /// Re-run a component fiber's render fn and integrate its output.
    pub(crate) fn render_fiber(&mut self, id: FiberId, rt: &RuntimeHandle) {
        if !self.contains(id) {
            return; // fiber unmounted between dirty-mark and processing
        }
        if !matches!(self.get(id).kind, FiberKind::Component(_)) {
            return;
        }
        let mut slots = std::mem::take(&mut self.get_mut(id).hooks);
        let child_el = {
            let FiberKind::Component(c) = &self.get(id).kind else {
                unreachable!()
            };
            let name = c.name();
            let mut hooks = Hooks::new(&mut slots, name, id, rt.clone());
            let el = c.render(&mut hooks);
            if hooks.cursor != hooks.slots.len() {
                panic!(
                    "ntui: {} used {} hooks this render but {} previously",
                    name,
                    hooks.cursor,
                    hooks.slots.len()
                );
            }
            el
        };
        self.get_mut(id).hooks = slots;
        // NAIVE (replaced in Task 7): tear down and remount all children.
        let old = std::mem::take(&mut self.get_mut(id).children);
        for c in old {
            self.unmount(c);
        }
        let child = self.mount_element(Some(id), child_el, rt);
        self.get_mut(id).children = vec![child];
    }
}
