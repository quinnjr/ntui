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

    struct MultiFx;
    #[derive(Clone, PartialEq, Default)]
    struct MultiFxProps {
        log: Shared<Vec<String>>,
        dep_a: Shared<Option<State<i32>>>,
    }
    impl Component for MultiFx {
        type Props = MultiFxProps;
        fn render(props: &MultiFxProps, hooks: &mut Hooks) -> Element {
            let a = hooks.use_state(|| 0);
            *props.dep_a.lock() = Some(a.clone());
            let av = a.get();

            let log_a = props.log.clone();
            hooks.use_effect(av, move || {
                log_a.lock().push("runA".into());
                let log_a = log_a.clone();
                move || log_a.lock().push("cleanA".into())
            });

            // Second effect with a fixed dep: only ever runs once.
            let log_b = props.log.clone();
            hooks.use_effect(0, move || {
                log_b.lock().push("runB".into());
                let log_b = log_b.clone();
                move || log_b.lock().push("cleanB".into())
            });

            Element::text(TextProps {
                content: av.to_string(),
                ..Default::default()
            })
        }
    }

    #[test]
    fn multiple_effects_track_deps_independently() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        std::mem::forget(_rx);
        let mut tree = FiberTree::new();
        let props = MultiFxProps::default();
        let root = tree.mount_root(Element::component::<MultiFx>(props.clone()), &rt);

        // (a) Both effects run on mount in declaration order.
        tree.flush_effects();
        assert_eq!(*props.log.lock(), vec!["runA", "runB"]);

        // (c) Changing only the first effect's dep re-runs only that effect
        // (its cleanup then its body); the second effect is untouched.
        props.log.lock().clear();
        props.dep_a.lock().clone().unwrap().set(1);
        tree.render_fiber(root, &rt);
        tree.flush_effects();
        assert_eq!(*props.log.lock(), vec!["cleanA", "runA"]);

        // (b) On unmount, cleanups run in declaration (forward) order — the impl
        // iterates the fiber's hook slots front-to-back in HookSlot::unmount.
        props.log.lock().clear();
        tree.unmount(root);
        assert_eq!(*props.log.lock(), vec!["cleanA", "cleanB"]);
    }

    struct Child;
    #[derive(Clone, PartialEq, Default)]
    struct ChildProps {
        log: Shared<Vec<String>>,
    }
    impl Component for Child {
        type Props = ChildProps;
        fn render(props: &ChildProps, hooks: &mut Hooks) -> Element {
            let log = props.log.clone();
            hooks.use_effect((), move || {
                log.lock().push("child".into());
            });
            Element::text(TextProps::default())
        }
    }

    struct Parent;
    #[derive(Clone, PartialEq, Default)]
    struct ParentProps {
        log: Shared<Vec<String>>,
    }
    impl Component for Parent {
        type Props = ParentProps;
        fn render(props: &ParentProps, hooks: &mut Hooks) -> Element {
            let log = props.log.clone();
            hooks.use_effect((), move || {
                log.lock().push("parent".into());
            });
            Element::view(
                crate::props::ViewProps::default(),
                vec![Element::component::<Child>(ChildProps {
                    log: props.log.clone(),
                })],
            )
        }
    }

    /// Pins `flush_effects`'s documented "parents before children" ordering
    /// guarantee across *different* fibers (not just multiple effects on the
    /// same fiber, which the tests above already cover). A depth-only sort
    /// (rather than the DFS-order walk `flush_effects` actually uses) would
    /// satisfy this alone but silently scramble same-depth sibling order —
    /// see the reverted attempt in this task's history.
    #[test]
    fn parent_effect_runs_before_child_effect_across_fibers() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        std::mem::forget(_rx);
        let mut tree = FiberTree::new();
        let log = Shared::default();
        let props = ParentProps { log };
        tree.mount_root(Element::component::<Parent>(props.clone()), &rt);

        tree.flush_effects();
        assert_eq!(*props.log.lock(), vec!["parent", "child"]);
    }
}
