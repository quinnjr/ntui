//! [`Hooks::use_memo`]: keep a computed value across renders.

use std::any::Any;

use crate::hooks::{HookSlot, Hooks};

/// What a memo slot holds: the deps it was computed for, and the value.
///
/// Both are erased, so a hook-order violation shows up as a failed
/// downcast rather than a type error; [`Hooks::use_memo`] turns either one
/// into a named panic.
pub(crate) struct MemoSlot {
    pub(crate) deps: Box<dyn Any>,
    pub(crate) value: Box<dyn Any>,
}

impl<'a> Hooks<'a> {
    /// Compute `f` on the first render, and again only when `deps` change.
    ///
    /// A component re-renders for many reasons — a keystroke, a parent's
    /// state change, a resize — and most of them do not affect any given
    /// derived value. Without a memo, work proportional to the *input* runs
    /// on every one of those renders even when the input has not moved.
    /// That is invisible for a formatted string and expensive for a
    /// several-hundred-element filter-and-sort.
    ///
    /// `deps` is compared with `PartialEq`, so keep it small — the point is
    /// to spend a cheap comparison instead of an expensive recompute. When
    /// the input is a large shared payload, depend on its identity rather
    /// than its contents by wrapping it in [`Shared`](crate::Shared), whose
    /// `PartialEq` is a pointer comparison.
    ///
    /// The value is cloned out on every render, so `T` wants to be cheap to
    /// clone — again, [`Shared<T>`](crate::Shared) for anything large.
    ///
    /// ```
    /// # use ntui::{Component, Element, Hooks, Shared, props::TextProps};
    /// # struct Table;
    /// # #[derive(Clone, PartialEq, Default)]
    /// # struct TableProps { rows: Shared<Vec<String>>, query: String }
    /// # impl Component for Table {
    /// #   type Props = TableProps;
    /// fn render(props: &TableProps, hooks: &mut Hooks) -> Element {
    ///     // Re-filters only when the source list or the query actually
    ///     // changes — not on every unrelated keystroke.
    ///     let matches = hooks.use_memo(
    ///         (props.rows.clone(), props.query.clone()),
    ///         || Shared::new(
    ///             props.rows.iter().filter(|r| r.contains(&props.query))
    ///                 .cloned().collect::<Vec<_>>(),
    ///         ),
    ///     );
    ///     Element::text(TextProps {
    ///         content: format!("{} matches", matches.len()),
    ///         ..Default::default()
    ///     })
    /// }
    /// # }
    /// ```
    ///
    /// Like every hook, this must be called unconditionally and in the same
    /// order on every render; calling it with a different `D` or `T` at the
    /// same slot panics with the component's name rather than silently
    /// recomputing.
    ///
    /// The memoized value is dropped when the component unmounts, but
    /// nothing is aborted — a `JoinHandle` belongs in
    /// [`use_future`](Hooks::use_future), not in a memo.
    pub fn use_memo<D, T>(&mut self, deps: D, f: impl FnOnce() -> T) -> T
    where
        D: PartialEq + 'static,
        T: Clone + 'static,
    {
        let mut f = Some(f);
        let mut deps = Some(deps);
        // Captured before `next_slot` borrows the slot vector, so a
        // diagnostic can name the offending component the way every other
        // hook's does.
        let component = self.component_name;

        let slot = self.next_slot(|| {
            HookSlot::Memo(MemoSlot {
                deps: Box::new(deps.take().unwrap()),
                value: Box::new((f.take().unwrap())()),
            })
        });

        let HookSlot::Memo(memo) = slot else {
            self.hook_mismatch("use_memo")
        };

        // On the render that created the slot, `deps`/`f` were consumed
        // above and the stored value is already correct.
        if let Some(deps) = deps {
            // Diagnosed rather than absorbed: a failed downcast here means
            // the hook was called with a different type than last render,
            // which is a hook-order violation. Treating it as "deps
            // changed" would silently rewrite the slot and hide the bug —
            // every sibling hook panics on exactly this.
            let previous = memo.deps.downcast_ref::<D>().unwrap_or_else(|| {
                panic!(
                    "ntui: {component}: use_memo deps type changed between renders — hooks must run in the same order every render"
                )
            });
            if *previous != deps {
                memo.deps = Box::new(deps);
                memo.value = Box::new((f.take().unwrap())());
            }
        }

        memo.value
            .downcast_ref::<T>()
            .unwrap_or_else(|| {
                panic!(
                    "ntui: {component}: use_memo value type changed between renders — hooks must run in the same order every render"
                )
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell as StdCell;
    use std::rc::Rc;

    use crate::component::Component;
    use crate::element::Element;
    use crate::hooks::Hooks;
    use crate::props::TextProps;
    use crate::test_util::Shared;
    use crate::testing::TestTerminal;
    use crate::{KeyCode, testing::render_once};

    /// Counts how many times the memo body ran, and echoes the memo's value.
    struct Counter;
    #[derive(Clone, PartialEq, Default)]
    struct CounterProps {
        runs: Shared<usize>,
    }
    impl Component for Counter {
        type Props = CounterProps;
        fn render(props: &CounterProps, hooks: &mut Hooks) -> Element {
            // Unrelated state: changing it re-renders without touching deps.
            let tick = hooks.use_state(|| 0i32);
            // Deps that only change when `key` does.
            let key = hooks.use_state(|| 0i32);

            let runs = props.runs.clone();
            let current = key.get();
            let value = hooks.use_memo(current, move || {
                *runs.lock() += 1;
                current * 10
            });

            let (t, k) = (tick.clone(), key.clone());
            hooks.use_input(move |ev, _| match ev.code {
                KeyCode::Char('t') => t.update(|v| *v += 1),
                KeyCode::Char('k') => k.update(|v| *v += 1),
                _ => {}
            });

            Element::text(TextProps {
                content: format!("value={value} tick={}", tick.get()),
                ..Default::default()
            })
        }
    }

    #[tokio::test]
    async fn computes_once_on_the_first_render() {
        let runs = Shared::default();
        let el = Element::component::<Counter>(CounterProps { runs: runs.clone() });

        let t = TestTerminal::new(30, 1, el).unwrap();

        assert_eq!(*runs.lock(), 1);
        assert!(t.frame_text().contains("value=0"));
    }

    #[tokio::test]
    async fn does_not_recompute_when_an_unrelated_render_happens() {
        // The whole point: a keystroke that changes something else must not
        // pay for this value again.
        let runs = Shared::default();
        let el = Element::component::<Counter>(CounterProps { runs: runs.clone() });
        let mut t = TestTerminal::new(30, 1, el).unwrap();

        t.send_key(KeyCode::Char('t')).unwrap();
        t.send_key(KeyCode::Char('t')).unwrap();

        assert!(t.frame_text().contains("tick=2"), "it did re-render");
        assert_eq!(*runs.lock(), 1, "but the memo body ran only once");
    }

    #[tokio::test]
    async fn recomputes_when_the_deps_change() {
        let runs = Shared::default();
        let el = Element::component::<Counter>(CounterProps { runs: runs.clone() });
        let mut t = TestTerminal::new(30, 1, el).unwrap();

        t.send_key(KeyCode::Char('k')).unwrap();

        assert_eq!(*runs.lock(), 2);
        assert!(t.frame_text().contains("value=10"), "{}", t.frame_text());
    }

    #[test]
    fn returns_the_computed_value_on_a_detached_render() {
        struct Once;
        #[derive(Clone, PartialEq, Default)]
        struct OnceProps;
        impl Component for Once {
            type Props = OnceProps;
            fn render(_: &OnceProps, hooks: &mut Hooks) -> Element {
                let doubled = hooks.use_memo(21, || 21 * 2);
                Element::text(TextProps {
                    content: format!("{doubled}"),
                    ..Default::default()
                })
            }
        }

        let el = render_once::<Once>(&OnceProps);

        let crate::element::Node::Text { props } = el.node else {
            panic!("expected text")
        };
        assert_eq!(props.content, "42");
    }

    #[tokio::test]
    async fn holds_a_value_that_is_not_partial_eq() {
        // Only the deps need comparing; the value does not.
        struct Holder;
        #[derive(Clone, PartialEq, Default)]
        struct HolderProps;
        #[derive(Clone)]
        struct NotComparable(Rc<StdCell<u32>>);

        impl Component for Holder {
            type Props = HolderProps;
            fn render(_: &HolderProps, hooks: &mut Hooks) -> Element {
                let held = hooks.use_memo(0u8, || NotComparable(Rc::new(StdCell::new(7))));
                Element::text(TextProps {
                    content: format!("held={}", held.0.get()),
                    ..Default::default()
                })
            }
        }

        let t = TestTerminal::new(20, 1, Element::component::<Holder>(HolderProps)).unwrap();

        assert!(t.frame_text().contains("held=7"));
    }
}
