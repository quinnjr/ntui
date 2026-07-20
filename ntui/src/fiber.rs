use std::any::TypeId;
use std::collections::HashMap;
use std::rc::Rc;

use crate::component::AnyComponent;
use crate::element::{Element, Node};
use crate::hooks::{HookSlot, RuntimeHandle};
use crate::props::{TextProps, ViewProps};

pub(crate) type FiberId = usize;

pub(crate) type ContextMap = HashMap<TypeId, Rc<dyn std::any::Any>>;

pub(crate) enum FiberKind {
    View(ViewProps),
    Text(TextProps),
    Fragment,
    Provider {
        type_id: TypeId,
        value: Rc<dyn std::any::Any>,
    },
    Component(Box<dyn AnyComponent>),
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(crate) struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

pub(crate) struct Fiber {
    pub kind: FiberKind,
    pub key: Option<String>,
    pub hooks: Vec<HookSlot>,
    pub children: Vec<FiberId>,
    pub parent: Option<FiberId>,
    pub layout: Rect,
    /// Text fibers only: the lines to paint, wrapped/truncated at the final
    /// resolved layout width. Filled by `compute_layout` each pass and read by
    /// `paint` so text is not re-wrapped on every frame. `None` for non-Text
    /// fibers and until the first layout pass.
    pub wrapped: Option<Vec<String>>,
    pub rendered_once: bool,
}

pub(crate) struct FiberTree {
    fibers: HashMap<FiberId, Fiber>,
    next_id: FiberId,
    pub root: Option<FiberId>,
    /// Set on any structural change or host-prop change; cleared by layout.
    pub layout_dirty: bool,
    /// Count of currently-mounted `Provider` fibers, maintained by
    /// `mount_element`/`unmount`. Lets `context_for` short-circuit the
    /// ancestor walk when the tree has no providers at all.
    provider_count: usize,
}

impl FiberTree {
    pub fn new() -> Self {
        FiberTree {
            fibers: HashMap::new(),
            next_id: 0,
            root: None,
            layout_dirty: false,
            provider_count: 0,
        }
    }

    pub fn get(&self, id: FiberId) -> &Fiber {
        self.fibers
            .get(&id)
            .unwrap_or_else(|| panic!("ntui: no fiber with id {id}"))
    }
    pub fn get_mut(&mut self, id: FiberId) -> &mut Fiber {
        self.fibers
            .get_mut(&id)
            .unwrap_or_else(|| panic!("ntui: no fiber with id {id}"))
    }
    pub fn contains(&self, id: FiberId) -> bool {
        self.fibers.contains_key(&id)
    }
    // Used by unit tests only until later tasks (e.g. devtools/debug output) consume it.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.fibers.len()
    }

    // Used by unit tests only until later tasks (e.g. devtools/debug output) consume it.
    #[allow(dead_code)]
    pub fn kind_name(&self, id: FiberId) -> &'static str {
        match self.get(id).kind {
            FiberKind::View(_) => "View",
            FiberKind::Text(_) => "Text",
            FiberKind::Fragment => "Fragment",
            FiberKind::Provider { .. } => "Provider",
            FiberKind::Component(_) => "Component",
        }
    }

    pub fn mount_root(&mut self, el: Element, rt: &RuntimeHandle) -> FiberId {
        let id = self.mount_element(None, el, rt);
        self.root = Some(id);
        id
    }

    pub(crate) fn mount_element(
        &mut self,
        parent: Option<FiberId>,
        el: Element,
        rt: &RuntimeHandle,
    ) -> FiberId {
        let id = self.next_id;
        self.next_id += 1;
        let (kind, child_els) = match el.node {
            Node::View { props, children } => (FiberKind::View(props), children),
            Node::Text { props } => (FiberKind::Text(props), Vec::new()),
            Node::Fragment { children } => (FiberKind::Fragment, children),
            Node::Provider {
                type_id,
                value,
                children,
            } => (FiberKind::Provider { type_id, value }, children),
            Node::Component(c) => (FiberKind::Component(c), Vec::new()),
        };
        let is_component = matches!(kind, FiberKind::Component(_));
        if matches!(kind, FiberKind::Provider { .. }) {
            self.provider_count += 1;
        }
        self.fibers.insert(
            id,
            Fiber {
                kind,
                key: el.key,
                hooks: Vec::new(),
                children: Vec::new(),
                parent,
                layout: Rect::default(),
                wrapped: None,
                rendered_once: false,
            },
        );
        self.layout_dirty = true;
        if is_component {
            self.render_fiber(id, rt); // reconciler.rs
        } else {
            let kids: Vec<FiberId> = child_els
                .into_iter()
                .map(|c| self.mount_element(Some(id), c, rt))
                .collect();
            self.get_mut(id).children = kids;
        }
        id
    }

    /// Children-first teardown, then hook teardown for this fiber.
    ///
    /// Precondition: the caller must have already removed `id` from its parent's
    /// `children` (or be unmounting the root/an orphaned subtree).
    pub(crate) fn unmount(&mut self, id: FiberId) {
        let children = self.get(id).children.clone();
        for c in children {
            self.unmount(c);
        }
        let fiber = self
            .fibers
            .remove(&id)
            .unwrap_or_else(|| panic!("ntui: no fiber with id {id}"));
        if matches!(fiber.kind, FiberKind::Provider { .. }) {
            self.provider_count -= 1;
        }
        for slot in fiber.hooks {
            slot.unmount();
        }
        self.layout_dirty = true;
    }

    /// Run pending effects depth-first (parents before children), running the
    /// previous cleanup before each rerun. Called after each commit.
    pub(crate) fn flush_effects(&mut self) {
        let Some(root) = self.root else { return };
        let mut order = Vec::new();
        self.collect_dfs(root, &mut order);
        for id in order {
            // If user code panics mid-processing, this fiber's taken hook slots are dropped
            // un-restored; acceptable because a panic tears down the whole app (see RestoreGuard
            // in runtime.rs).
            let mut slots = std::mem::take(&mut self.get_mut(id).hooks);
            for slot in &mut slots {
                if let crate::hooks::HookSlot::Effect(e) = slot
                    && let Some(pending) = e.pending.take()
                {
                    if let Some(cleanup) = e.cleanup.take() {
                        cleanup();
                    }
                    e.cleanup = pending().0;
                }
            }
            self.get_mut(id).hooks = slots;
        }
    }

    fn collect_dfs(&self, id: FiberId, out: &mut Vec<FiberId>) {
        out.push(id);
        for c in &self.get(id).children {
            self.collect_dfs(*c, out);
        }
    }

    /// Deepest-first (post-order DFS) collection of input handlers, so that
    /// events reach the most nested `use_input` first and bubble outward.
    pub(crate) fn collect_input_handlers(&self) -> Vec<crate::hooks::input::InputHandler> {
        let mut out = Vec::new();
        if let Some(root) = self.root {
            self.collect_handlers_post(root, &mut out);
        }
        out
    }

    fn collect_handlers_post(&self, id: FiberId, out: &mut Vec<crate::hooks::input::InputHandler>) {
        for c in &self.get(id).children {
            self.collect_handlers_post(*c, out);
        }
        for slot in &self.get(id).hooks {
            if let HookSlot::Input(h) = slot {
                out.push(h.clone());
            }
        }
    }

    /// Resolve a fiber's context by walking ancestors, computed fresh each
    /// render so it's always current. Walking child→root and keeping the
    /// first entry per type (`or_insert`) means the nearest provider wins,
    /// in a single pass with no intermediate chain allocation.
    pub(crate) fn context_for(&self, id: FiberId) -> Rc<ContextMap> {
        if self.provider_count == 0 {
            return Rc::new(ContextMap::new());
        }
        let mut map = ContextMap::new();
        let mut cur = self.get(id).parent;
        while let Some(p) = cur {
            if let FiberKind::Provider { type_id, value } = &self.get(p).kind {
                map.entry(*type_id).or_insert_with(|| value.clone());
            }
            cur = self.get(p).parent;
        }
        Rc::new(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
    use crate::element::Element;
    use crate::hooks::state::State;
    use crate::hooks::{Hooks, RuntimeHandle};
    use crate::props::{TextProps, ViewProps};
    use crate::test_util::Shared;

    struct App;
    #[derive(Clone, PartialEq, Default)]
    struct AppProps;
    impl Component for App {
        type Props = AppProps;
        fn render(_: &AppProps, _: &mut Hooks) -> Element {
            Element::view(
                ViewProps::default(),
                vec![
                    Element::text(TextProps {
                        content: "a".into(),
                        ..Default::default()
                    }),
                    Element::fragment(vec![Element::text(TextProps {
                        content: "b".into(),
                        ..Default::default()
                    })]),
                ],
            )
        }
    }

    #[test]
    fn mount_builds_tree_through_components_and_fragments() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        let root = tree.mount_root(Element::component::<App>(AppProps), &rt);
        assert_eq!(tree.kind_name(root), "Component");
        let view = tree.get(root).children[0];
        assert_eq!(tree.kind_name(view), "View");
        let kids = tree.get(view).children.clone();
        assert_eq!(kids.len(), 2);
        assert_eq!(tree.kind_name(kids[0]), "Text");
        assert_eq!(tree.kind_name(kids[1]), "Fragment");
        assert_eq!(tree.len(), 5);
        assert!(tree.layout_dirty);
    }

    #[test]
    fn unmount_removes_whole_subtree() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        let root = tree.mount_root(Element::component::<App>(AppProps), &rt);
        tree.unmount(root);
        assert_eq!(tree.len(), 0);
    }

    #[test]
    fn empty_tree_effects_and_handlers_are_noops() {
        let mut tree = FiberTree::new();
        tree.flush_effects(); // no root -> early return
        assert!(tree.collect_input_handlers().is_empty());
    }

    #[test]
    #[should_panic(expected = "no fiber with id")]
    fn get_missing_fiber_panics() {
        let tree = FiberTree::new();
        let _ = tree.get(999);
    }

    #[test]
    #[should_panic(expected = "no fiber with id")]
    fn get_mut_missing_fiber_panics() {
        let mut tree = FiberTree::new();
        let _ = tree.get_mut(999);
    }

    struct Consumer;
    #[derive(Clone, PartialEq, Default)]
    struct ConsumerProps {
        value_out: Shared<Option<u32>>,
    }
    impl Component for Consumer {
        type Props = ConsumerProps;
        fn render(props: &ConsumerProps, hooks: &mut Hooks) -> Element {
            let value = hooks.use_context::<u32>();
            *props.value_out.lock() = value.map(|v| *v);
            Element::text(TextProps {
                content: "consumer".into(),
                ..Default::default()
            })
        }
    }

    struct NestedProviderHost;
    #[derive(Clone, PartialEq, Default)]
    struct NestedProviderHostProps {
        show_inner_out: Shared<Option<State<bool>>>,
        consumer_value_out: Shared<Option<u32>>,
    }
    impl Component for NestedProviderHost {
        type Props = NestedProviderHostProps;
        fn render(props: &NestedProviderHostProps, hooks: &mut Hooks) -> Element {
            let show_inner = hooks.use_state(|| true);
            *props.show_inner_out.lock() = Some(show_inner.clone());
            let consumer = Element::component::<Consumer>(ConsumerProps {
                value_out: props.consumer_value_out.clone(),
            });
            let inner = if show_inner.get() {
                Element::provider(2u32, vec![consumer])
            } else {
                consumer
            };
            Element::provider(1u32, vec![inner])
        }
    }

    /// Pins `provider_count`'s mount/unmount bookkeeping: two nested `Provider`
    /// fibers of the same `TypeId` mount to a count of 2 with the consumer
    /// seeing the nearest (inner) value; unmounting just the inner provider
    /// (via re-render, not a direct `unmount` call) must decrement the count
    /// to exactly 1 and flip the consumer over to the outer provider's value,
    /// proving `context_for`'s short-circuit stays accurate as providers
    /// actually leave the tree.
    #[test]
    fn provider_count_tracks_mount_and_unmount() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        std::mem::forget(_rx);
        let mut tree = FiberTree::new();
        let props = NestedProviderHostProps::default();
        let root = tree.mount_root(Element::component::<NestedProviderHost>(props.clone()), &rt);

        assert_eq!(tree.provider_count, 2);
        assert_eq!(*props.consumer_value_out.lock(), Some(2));

        let show_inner = props.show_inner_out.lock().clone().unwrap();
        show_inner.set(false);
        tree.render_fiber(root, &rt);

        assert_eq!(tree.provider_count, 1);
        assert_eq!(*props.consumer_value_out.lock(), Some(1));
    }
}
