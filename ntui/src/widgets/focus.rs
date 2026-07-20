use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::fiber::FiberId;
use crate::hooks::Hooks;
use crate::hooks::input::{KeyCode, KeyModifiers};
use crate::hooks::state::State;

struct FocusState {
    /// Registration order, used for Tab/Shift-Tab cycling.
    order: Vec<FiberId>,
    current: Option<FiberId>,
    /// Per-widget wake handle: calling `.set(())` marks exactly that widget's
    /// fiber dirty, independent of whether its own props changed — context
    /// reads alone don't trigger a re-render (see `use_context`'s doc note).
    pokes: HashMap<FiberId, State<()>>,
}

/// A registry of focusable widgets sharing Tab/Shift-Tab navigation, created
/// by [`Hooks::use_focus_scope`] and read by [`Hooks::use_focusable`] via
/// context. Cheap to clone (an `Rc` internally); all clones share one scope.
#[derive(Clone)]
pub struct FocusScopeHandle(Rc<RefCell<FocusState>>);

impl FocusScopeHandle {
    fn new() -> Self {
        FocusScopeHandle(Rc::new(RefCell::new(FocusState {
            order: Vec::new(),
            current: None,
            pokes: HashMap::new(),
        })))
    }

    fn register(&self, id: FiberId, poke: State<()>) {
        let mut s = self.0.borrow_mut();
        if !s.order.contains(&id) {
            s.order.push(id);
        }
        s.pokes.insert(id, poke.clone());
        if s.current.is_none() {
            s.current = Some(id);
            // The registering widget already rendered `is_focused = false`
            // this frame (registration runs in a deferred effect, after the
            // render that called `use_focusable`); wake it once more now
            // that it's the default focus, so that render reflects it.
            poke.set(());
        }
    }

    fn unregister(&self, id: FiberId) {
        let mut s = self.0.borrow_mut();
        s.order.retain(|x| *x != id);
        s.pokes.remove(&id);
        if s.current == Some(id) {
            s.current = s.order.first().copied();
            if let Some(next) = s.current
                && let Some(poke) = s.pokes.get(&next)
            {
                poke.set(());
            }
        }
    }

    fn is_focused(&self, id: FiberId) -> bool {
        self.0.borrow().current == Some(id)
    }

    fn set_focused(&self, id: FiberId) {
        let mut s = self.0.borrow_mut();
        let prev = s.current;
        if prev == Some(id) {
            return;
        }
        s.current = Some(id);
        if let Some(p) = prev
            && let Some(poke) = s.pokes.get(&p)
        {
            poke.set(());
        }
        if let Some(poke) = s.pokes.get(&id) {
            poke.set(());
        }
    }

    /// Moves focus to the next (`forward`) or previous registered widget, in
    /// registration order, wrapping around at the ends.
    fn cycle(&self, forward: bool) {
        let mut s = self.0.borrow_mut();
        if s.order.is_empty() {
            return;
        }
        let len = s.order.len();
        let cur_idx = s
            .current
            .and_then(|c| s.order.iter().position(|x| *x == c))
            .unwrap_or(0);
        let next_idx = if forward {
            (cur_idx + 1) % len
        } else {
            (cur_idx + len - 1) % len
        };
        let prev = s.current;
        let next = s.order[next_idx];
        s.current = Some(next);
        if prev != Some(next) {
            if let Some(p) = prev
                && let Some(poke) = s.pokes.get(&p)
            {
                poke.set(());
            }
            if let Some(poke) = s.pokes.get(&next) {
                poke.set(());
            }
        }
    }
}

/// Whether a widget mounted via [`Hooks::use_focusable`] currently holds
/// focus, and a way to claim it.
pub struct Focus {
    id: FiberId,
    is_focused: bool,
    scope: Option<FocusScopeHandle>,
}

impl Focus {
    /// True if this widget instance is the currently focused one within its
    /// enclosing [`FocusScopeHandle`]. Always `false` with no enclosing scope.
    pub fn is_focused(&self) -> bool {
        self.is_focused
    }

    /// Claims focus for this widget instance. A no-op with no enclosing
    /// scope (e.g. the widget is used outside `use_focus_scope`).
    pub fn claim(&self) {
        if let Some(scope) = &self.scope {
            scope.set_focused(self.id);
        }
    }
}

impl<'a> Hooks<'a> {
    /// Creates (on first render) or returns the [`FocusScopeHandle`] owned by
    /// this component, and registers Tab / Shift-Tab handling that cycles
    /// focus among every [`Hooks::use_focusable`] registered under it.
    ///
    /// Make the handle reachable to descendants with
    /// `element! { ContextProvider(value: scope) { ... } }`.
    pub fn use_focus_scope(&mut self) -> FocusScopeHandle {
        let handle = self.use_state(FocusScopeHandle::new).get();
        let h = handle.clone();
        self.use_input(move |ev, ctx| match ev.code {
            KeyCode::BackTab => {
                h.cycle(false);
                ctx.stop_propagation();
            }
            KeyCode::Tab => {
                h.cycle(!ev.modifiers.contains(KeyModifiers::SHIFT));
                ctx.stop_propagation();
            }
            _ => {}
        });
        handle
    }

    /// Registers this component instance as a focusable participant in the
    /// nearest ancestor [`Hooks::use_focus_scope`] (found via context), for
    /// as long as it stays mounted. With no enclosing scope, the returned
    /// [`Focus`] always reports `is_focused() == false`.
    pub fn use_focusable(&mut self) -> Focus {
        let id = self.fiber_id;
        let poke = self.use_state(|| ());
        let scope = self
            .use_context::<FocusScopeHandle>()
            .map(|rc| (*rc).clone());

        let register_scope = scope.clone();
        self.use_effect((), move || {
            if let Some(s) = &register_scope {
                s.register(id, poke);
            }
            let unregister_scope = register_scope.clone();
            move || {
                if let Some(s) = unregister_scope {
                    s.unregister(id);
                }
            }
        });

        let is_focused = scope.as_ref().is_some_and(|s| s.is_focused(id));
        Focus {
            id,
            is_focused,
            scope,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
    use crate::element::Element;
    use crate::props::TextProps;
    use crate::testing::TestTerminal;

    struct Item;
    #[derive(Clone, PartialEq, Default)]
    struct ItemProps {
        label: &'static str,
        claim_on_mount: bool,
    }
    impl Component for Item {
        type Props = ItemProps;
        fn render(props: &ItemProps, hooks: &mut Hooks) -> Element {
            let focus = hooks.use_focusable();
            let is_focused = focus.is_focused();
            if props.claim_on_mount {
                hooks.use_effect((), move || focus.claim());
            }
            Element::text(TextProps {
                content: format!("{}:{}", props.label, is_focused),
                ..Default::default()
            })
        }
    }

    struct Scope;
    #[derive(Clone, PartialEq, Default)]
    struct ScopeProps {
        second_claims_on_mount: bool,
    }
    impl Component for Scope {
        type Props = ScopeProps;
        fn render(props: &ScopeProps, hooks: &mut Hooks) -> Element {
            let scope = hooks.use_focus_scope();
            Element::provider(
                scope,
                vec![Element::fragment(vec![
                    Element::component::<Item>(ItemProps {
                        label: "a",
                        claim_on_mount: false,
                    }),
                    Element::component::<Item>(ItemProps {
                        label: "b",
                        claim_on_mount: props.second_claims_on_mount,
                    }),
                ])],
            )
        }
    }

    #[tokio::test]
    async fn first_registered_widget_is_focused_by_default() {
        let t =
            TestTerminal::new(40, 2, Element::component::<Scope>(ScopeProps::default())).unwrap();
        assert!(t.frame_text().contains("a:true"));
        assert!(t.frame_text().contains("b:false"));
    }

    #[tokio::test]
    async fn tab_cycles_focus_forward_and_wraps() {
        let mut t =
            TestTerminal::new(40, 2, Element::component::<Scope>(ScopeProps::default())).unwrap();
        t.send_key(KeyCode::Tab).unwrap();
        assert!(t.frame_text().contains("a:false"));
        assert!(t.frame_text().contains("b:true"));
        t.send_key(KeyCode::Tab).unwrap();
        assert!(t.frame_text().contains("a:true"));
        assert!(t.frame_text().contains("b:false"));
    }

    #[tokio::test]
    async fn shift_tab_cycles_focus_backward() {
        let mut t =
            TestTerminal::new(40, 2, Element::component::<Scope>(ScopeProps::default())).unwrap();
        t.send_key(KeyCode::BackTab).unwrap();
        assert!(t.frame_text().contains("a:false"));
        assert!(t.frame_text().contains("b:true"));
    }

    #[tokio::test]
    async fn without_a_scope_focusable_never_reports_focused() {
        let t = TestTerminal::new(
            40,
            1,
            Element::component::<Item>(ItemProps {
                label: "solo",
                claim_on_mount: false,
            }),
        )
        .unwrap();
        assert!(t.frame_text().contains("solo:false"));
    }

    #[tokio::test]
    async fn claim_moves_focus_to_this_widget_and_unfocuses_the_previous_one() {
        let t = TestTerminal::new(
            40,
            2,
            Element::component::<Scope>(ScopeProps {
                second_claims_on_mount: true,
            }),
        )
        .unwrap();
        assert!(t.frame_text().contains("a:false"));
        assert!(t.frame_text().contains("b:true"));
    }

    #[tokio::test]
    async fn claim_without_a_scope_is_a_no_op() {
        let t = TestTerminal::new(
            40,
            1,
            Element::component::<Item>(ItemProps {
                label: "solo",
                claim_on_mount: true,
            }),
        )
        .unwrap();
        assert!(t.frame_text().contains("solo:false"));
    }
}
