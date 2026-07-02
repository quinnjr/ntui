use std::sync::{Arc, Mutex};

use tokio::sync::mpsc::UnboundedSender;

use crate::fiber::FiberId;
use crate::hooks::{HookSlot, Hooks, Wake};

/// Owned component state. Clonable and Send (if T: Send) so it can be moved
/// into hook-spawned tasks; setting it marks the owning fiber dirty.
pub struct State<T> {
    inner: Arc<Mutex<T>>,
    fiber: FiberId,
    wake: UnboundedSender<Wake>,
}

impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        State {
            inner: self.inner.clone(),
            fiber: self.fiber,
            wake: self.wake.clone(),
        }
    }
}

impl<T: 'static> State<T> {
    pub fn set(&self, value: T) {
        *self.inner.lock().unwrap() = value;
        let _ = self.wake.send(Wake::Dirty(self.fiber));
    }
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        f(&mut self.inner.lock().unwrap());
        let _ = self.wake.send(Wake::Dirty(self.fiber));
    }
}

impl<T: Clone + 'static> State<T> {
    pub fn get(&self) -> T {
        self.inner.lock().unwrap().clone()
    }
}

impl<'a> Hooks<'a> {
    pub fn use_state<T: 'static>(&mut self, init: impl FnOnce() -> T) -> State<T> {
        let fiber = self.fiber_id;
        let wake = self.runtime.wake.clone();
        let name = self.component_name;
        let slot = self.next_slot(|| {
            HookSlot::State(Box::new(State {
                inner: Arc::new(Mutex::new(init())),
                fiber,
                wake,
            }))
        });
        // `#[allow(irrefutable_let_patterns)]`: `HookSlot` has only the
        // `State` variant so far, so this pattern always matches today. It
        // becomes genuinely refutable once more hook variants land (Task
        // 8+), which is why `hook_mismatch` returns `!` rather than being
        // dropped now.
        #[allow(irrefutable_let_patterns)]
        let HookSlot::State(any) = slot else {
            self.hook_mismatch("use_state")
        };
        any.downcast_ref::<State<T>>()
            .unwrap_or_else(|| panic!("ntui: {name}: use_state type changed between renders"))
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
    use crate::element::Element;
    use crate::fiber::FiberTree;
    use crate::hooks::{Hooks, RuntimeHandle, Wake};
    use crate::props::TextProps;
    use crate::test_util::Shared;

    struct Counter;
    #[derive(Clone, PartialEq, Default)]
    struct CounterProps {
        log: Shared<Vec<i32>>,
        handle: Shared<Option<State<i32>>>,
    }
    impl Component for Counter {
        type Props = CounterProps;
        fn render(props: &CounterProps, hooks: &mut Hooks) -> Element {
            let n = hooks.use_state(|| 0);
            props.log.lock().push(n.get());
            *props.handle.lock() = Some(n.clone());
            Element::text(TextProps {
                content: n.get().to_string(),
                ..Default::default()
            })
        }
    }

    #[test]
    fn set_wakes_dirty_and_rerender_sees_new_value() {
        let (rt, mut rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        let props = CounterProps::default();
        let root = tree.mount_root(Element::component::<Counter>(props.clone()), &rt);

        let st = props.handle.lock().clone().unwrap();
        st.set(5);
        assert!(matches!(rx.try_recv(), Ok(Wake::Dirty(id)) if id == root));

        tree.render_fiber(root, &rt);
        assert_eq!(*props.log.lock(), vec![0, 5]);
    }

    #[test]
    fn update_mutates_in_place() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        let props = CounterProps::default();
        let root = tree.mount_root(Element::component::<Counter>(props.clone()), &rt);
        let st = props.handle.lock().clone().unwrap();
        st.update(|n| *n += 3);
        tree.render_fiber(root, &rt);
        assert_eq!(*props.log.lock(), vec![0, 3]);
    }
}
