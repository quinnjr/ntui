use std::any::Any;

use crate::hooks::{HookSlot, Hooks};

/// What an effect returns. `().into()` = no cleanup; any `FnOnce()` = cleanup.
pub struct Cleanup(pub(crate) Option<Box<dyn FnOnce()>>);

impl From<()> for Cleanup {
    fn from(_: ()) -> Self {
        Cleanup(None)
    }
}
impl<F: FnOnce() + 'static> From<F> for Cleanup {
    fn from(f: F) -> Self {
        Cleanup(Some(Box::new(f)))
    }
}

pub(crate) struct EffectSlot {
    /// Box<Option<D>>; None until first run so the first render always fires.
    deps: Box<dyn Any>,
    pub(crate) cleanup: Option<Box<dyn FnOnce()>>,
    /// Deferred effect body, executed by FiberTree::flush_effects after commit.
    pub(crate) pending: Option<Box<dyn FnOnce() -> Cleanup>>,
}

impl<'a> Hooks<'a> {
    /// Schedules `effect` to run after commit whenever `deps` changes
    /// (compared to the previous render), including on first mount. If the
    /// previous run returned a cleanup, it runs before the new invocation
    /// and on unmount.
    pub fn use_effect<D, C>(&mut self, deps: D, effect: impl FnOnce() -> C + 'static)
    where
        D: PartialEq + 'static,
        C: Into<Cleanup>,
    {
        let name = self.component_name;
        let slot = self.next_slot(|| {
            HookSlot::Effect(EffectSlot {
                deps: Box::new(None::<D>),
                cleanup: None,
                pending: None,
            })
        });
        let HookSlot::Effect(e) = slot else {
            self.hook_mismatch("use_effect")
        };
        let prev = e.deps.downcast_mut::<Option<D>>().unwrap_or_else(|| {
            panic!("ntui: {name}: use_effect deps type changed between renders")
        });
        if prev.as_ref() != Some(&deps) {
            *prev = Some(deps);
            e.pending = Some(Box::new(move || effect().into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::component::Component;
    use crate::element::Element;
    use crate::fiber::FiberTree;
    use crate::hooks::state::State;
    use crate::hooks::{Hooks, RuntimeHandle};
    use crate::props::TextProps;
    use crate::test_util::Shared;

    struct Fx;
    #[derive(Clone, PartialEq, Default)]
    struct FxProps {
        log: Shared<Vec<String>>,
        n_out: Shared<Option<State<i32>>>,
    }
    impl Component for Fx {
        type Props = FxProps;
        fn render(props: &FxProps, hooks: &mut Hooks) -> Element {
            let n = hooks.use_state(|| 0);
            *props.n_out.lock() = Some(n.clone());
            let v = n.get();
            let log = props.log.clone();
            hooks.use_effect(v, move || {
                log.lock().push(format!("run{v}"));
                let log = log.clone();
                move || log.lock().push(format!("clean{v}"))
            });
            Element::text(TextProps {
                content: v.to_string(),
                ..Default::default()
            })
        }
    }

    #[test]
    fn effect_runs_after_flush_cleans_on_deps_change_and_unmount() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        std::mem::forget(_rx);
        let mut tree = FiberTree::new();
        let props = FxProps::default();
        let root = tree.mount_root(Element::component::<Fx>(props.clone()), &rt);

        assert!(props.log.lock().is_empty()); // effects don't run during render
        tree.flush_effects();
        assert_eq!(*props.log.lock(), vec!["run0"]);

        tree.flush_effects(); // no deps change -> no rerun
        assert_eq!(*props.log.lock(), vec!["run0"]);

        props.n_out.lock().clone().unwrap().set(1);
        tree.render_fiber(root, &rt);
        tree.flush_effects();
        assert_eq!(*props.log.lock(), vec!["run0", "clean0", "run1"]);

        tree.unmount(root);
        assert_eq!(*props.log.lock(), vec!["run0", "clean0", "run1", "clean1"]);
    }
}
