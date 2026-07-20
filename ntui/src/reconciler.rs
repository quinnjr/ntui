use crate::element::Element;
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
        let first_render = !self.get(id).rendered_once;
        let context = self.context_for(id);
        // If user code panics mid-processing, this fiber's taken hook slots are dropped
        // un-restored; acceptable because a panic tears down the whole app (see RestoreGuard
        // in runtime.rs).
        let mut slots = std::mem::take(&mut self.get_mut(id).hooks);
        let child_el = {
            let FiberKind::Component(c) = &self.get(id).kind else {
                unreachable!()
            };
            let name = c.name();
            let mut hooks = Hooks::new(&mut slots, name, id, rt.clone(), first_render, context);
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
        self.get_mut(id).rendered_once = true;
        self.reconcile_children(id, vec![child_el], rt);
    }
}

use std::collections::HashMap;

use crate::element::Node;

impl FiberTree {
    /// React-style child diffing: match by key when present, else index+type.
    pub(crate) fn reconcile_children(
        &mut self,
        parent: FiberId,
        new: Vec<Element>,
        rt: &RuntimeHandle,
    ) {
        let old = std::mem::take(&mut self.get_mut(parent).children);
        let old_order = old.clone();
        let mut old_keyed: HashMap<String, FiberId> = HashMap::new();
        let mut old_indexed: Vec<Option<FiberId>> = Vec::new();
        for id in old {
            match self.get(id).key.clone() {
                Some(k) => {
                    if let Some(displaced) = old_keyed.insert(k, id) {
                        // Duplicate sibling keys: keep the last occurrence, drop the earlier fiber.
                        self.unmount(displaced);
                    }
                }
                None => old_indexed.push(Some(id)),
            }
        }

        let mut next = Vec::with_capacity(new.len());
        let mut unkeyed_i = 0usize;
        for el in new {
            let candidate = match &el.key {
                Some(k) => old_keyed.remove(k),
                None => {
                    let c = old_indexed.get_mut(unkeyed_i).and_then(|s| s.take());
                    unkeyed_i += 1;
                    c
                }
            };
            let id = match candidate {
                Some(old_id) if self.same_kind(old_id, &el.node) => {
                    self.update_fiber(old_id, el, rt);
                    old_id
                }
                Some(old_id) => {
                    self.unmount(old_id);
                    self.mount_element(Some(parent), el, rt)
                }
                None => self.mount_element(Some(parent), el, rt),
            };
            next.push(id);
        }

        for id in old_keyed.into_values() {
            self.unmount(id);
        }
        for id in old_indexed.into_iter().flatten() {
            self.unmount(id);
        }
        // A pure reorder of surviving children is structural even though no
        // individual fiber's props changed, so it isn't caught by the
        // `changed` guards in `update_fiber`. Mounts/unmounts already flag
        // layout via `mount_element`/`unmount`; only flag here when the
        // surviving children's order actually moved.
        let reordered = next != old_order;
        self.get_mut(parent).children = next;
        if reordered {
            self.layout_dirty = true;
        }
    }

    fn same_kind(&self, id: FiberId, node: &Node) -> bool {
        match (&self.get(id).kind, node) {
            (FiberKind::View(_), Node::View { .. }) => true,
            (FiberKind::Text(_), Node::Text { .. }) => true,
            (FiberKind::Fragment, Node::Fragment { .. }) => true,
            (FiberKind::Provider { type_id, .. }, Node::Provider { type_id: t, .. }) => {
                type_id == t
            }
            (FiberKind::Component(c), Node::Component(n)) => {
                c.component_type() == n.component_type()
            }
            _ => false,
        }
    }

    /// Apply a matched element to an existing fiber and recurse.
    fn update_fiber(&mut self, id: FiberId, el: Element, rt: &RuntimeHandle) {
        self.get_mut(id).key = el.key;
        match el.node {
            Node::View { props, children } => {
                let changed = {
                    let FiberKind::View(old) = &mut self.get_mut(id).kind else {
                        unreachable!()
                    };
                    if *old != props {
                        *old = props;
                        true
                    } else {
                        false
                    }
                };
                if changed {
                    self.layout_dirty = true;
                }
                self.reconcile_children(id, children, rt);
            }
            Node::Text { props } => {
                let changed = {
                    let FiberKind::Text(old) = &mut self.get_mut(id).kind else {
                        unreachable!()
                    };
                    if *old != props {
                        *old = props;
                        true
                    } else {
                        false
                    }
                };
                if changed {
                    self.layout_dirty = true;
                }
            }
            Node::Fragment { children } => self.reconcile_children(id, children, rt),
            Node::Provider {
                value, children, ..
            } => {
                let FiberKind::Provider { value: v, .. } = &mut self.get_mut(id).kind else {
                    unreachable!()
                };
                *v = value; // dyn Any values can't be compared; children always reconcile
                self.reconcile_children(id, children, rt);
            }
            Node::Component(c) => {
                let equal = {
                    let FiberKind::Component(old) = &self.get(id).kind else {
                        unreachable!()
                    };
                    old.props_eq(c.as_ref())
                };
                if !equal {
                    let FiberKind::Component(old) = &mut self.get_mut(id).kind else {
                        unreachable!()
                    };
                    *old = c;
                    self.render_fiber(id, rt);
                }
            }
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

    struct Item;
    #[derive(Clone, PartialEq, Default)]
    struct ItemProps {
        id: String,
        render_log: Shared<Vec<String>>,
    }
    impl Component for Item {
        type Props = ItemProps;
        fn render(props: &ItemProps, _: &mut Hooks) -> Element {
            props.render_log.lock().push(props.id.clone());
            Element::text(TextProps {
                content: props.id.clone(),
                ..Default::default()
            })
        }
    }

    struct List;
    #[derive(Clone, PartialEq, Default)]
    struct ListProps {
        keys_out: Shared<Option<State<Vec<String>>>>,
        render_log: Shared<Vec<String>>,
    }
    impl Component for List {
        type Props = ListProps;
        fn render(props: &ListProps, hooks: &mut Hooks) -> Element {
            let keys = hooks.use_state(|| vec!["a".to_string(), "b".to_string()]);
            *props.keys_out.lock() = Some(keys.clone());
            Element::fragment(
                keys.get()
                    .into_iter()
                    .map(|k| {
                        Element::component::<Item>(ItemProps {
                            id: k.clone(),
                            render_log: props.render_log.clone(),
                        })
                        .with_key(k)
                    })
                    .collect(),
            )
        }
    }

    fn mount_list() -> (FiberTree, RuntimeHandle, ListProps, usize) {
        let (rt, _rx) = RuntimeHandle::test_handle();
        std::mem::forget(_rx); // keep channel open for the test
        let mut tree = FiberTree::new();
        let props = ListProps::default();
        let root = tree.mount_root(Element::component::<List>(props.clone()), &rt);
        (tree, rt, props, root)
    }

    #[test]
    fn keyed_reorder_preserves_fibers() {
        let (mut tree, rt, props, root) = mount_list();
        let fragment = tree.get(root).children[0];
        let before = tree.get(fragment).children.clone();

        let keys = props.keys_out.lock().clone().unwrap();
        keys.set(vec!["b".to_string(), "a".to_string()]);
        tree.render_fiber(root, &rt);

        let after = tree.get(tree.get(root).children[0]).children.clone();
        assert_eq!(after, vec![before[1], before[0]]); // same fibers, swapped order
    }

    #[test]
    fn equal_props_skip_child_render() {
        let (mut tree, rt, props, root) = mount_list();
        props.render_log.lock().clear();
        tree.render_fiber(root, &rt); // parent re-renders, child props unchanged
        assert!(props.render_log.lock().is_empty());
    }

    #[test]
    fn removed_key_unmounts_subtree() {
        let (mut tree, rt, props, root) = mount_list();
        let count_before = tree.len();
        let keys = props.keys_out.lock().clone().unwrap();
        keys.set(vec!["b".to_string()]);
        tree.render_fiber(root, &rt);
        assert!(tree.len() < count_before);
    }

    // ---- regression tests: duplicate keys, kind mismatch, unkeyed
    // matching, in-place prop updates, and provider value swaps. ----

    struct KeyedList;
    #[derive(Clone, PartialEq, Default)]
    struct KeyedListProps {
        initial: Vec<String>,
        keys_out: Shared<Option<State<Vec<String>>>>,
    }
    impl Component for KeyedList {
        type Props = KeyedListProps;
        fn render(props: &KeyedListProps, hooks: &mut Hooks) -> Element {
            let keys = hooks.use_state(|| props.initial.clone());
            *props.keys_out.lock() = Some(keys.clone());
            Element::fragment(
                keys.get()
                    .into_iter()
                    .map(|k| {
                        Element::component::<Item>(ItemProps {
                            id: k.clone(),
                            render_log: Shared::default(),
                        })
                        .with_key(k)
                    })
                    .collect(),
            )
        }
    }

    fn mount_keyed_list(initial: Vec<String>) -> (FiberTree, RuntimeHandle, KeyedListProps, usize) {
        let (rt, _rx) = RuntimeHandle::test_handle();
        std::mem::forget(_rx);
        let mut tree = FiberTree::new();
        let props = KeyedListProps {
            initial,
            keys_out: Shared::default(),
        };
        let root = tree.mount_root(Element::component::<KeyedList>(props.clone()), &rt);
        (tree, rt, props, root)
    }

    #[test]
    fn duplicate_keys_do_not_leak_fibers() {
        let (mut tree, rt, props, root) = mount_keyed_list(vec!["x".to_string(), "x".to_string()]);

        let keys = props.keys_out.lock().clone().unwrap();
        keys.set(vec!["x".to_string()]);
        tree.render_fiber(root, &rt);
        let len_after_shrink = tree.len();

        // component fiber + fragment fiber + single Item component fiber +
        // its Text child == 4.
        assert_eq!(len_after_shrink, 4);

        // Repeated no-op re-renders must not leak fibers (monotonic growth).
        for _ in 0..5 {
            keys.set(vec!["x".to_string()]);
            tree.render_fiber(root, &rt);
            assert_eq!(tree.len(), len_after_shrink);
        }
    }

    struct ToggleKind;
    #[derive(Clone, PartialEq, Default)]
    struct ToggleKindProps {
        is_text_out: Shared<Option<State<bool>>>,
    }
    impl Component for ToggleKind {
        type Props = ToggleKindProps;
        fn render(props: &ToggleKindProps, hooks: &mut Hooks) -> Element {
            let is_text = hooks.use_state(|| true);
            *props.is_text_out.lock() = Some(is_text.clone());
            let child = if is_text.get() {
                Element::text(TextProps {
                    content: "t".into(),
                    ..Default::default()
                })
                .with_key("k")
            } else {
                Element::view(crate::props::ViewProps::default(), vec![]).with_key("k")
            };
            Element::fragment(vec![child])
        }
    }

    #[test]
    fn keyed_kind_mismatch_remounts() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        std::mem::forget(_rx);
        let mut tree = FiberTree::new();
        let props = ToggleKindProps::default();
        let root = tree.mount_root(Element::component::<ToggleKind>(props.clone()), &rt);
        let fragment = tree.get(root).children[0];
        let old_id = tree.get(fragment).children[0];
        assert_eq!(tree.kind_name(old_id), "Text");

        let is_text = props.is_text_out.lock().clone().unwrap();
        is_text.set(false);
        tree.render_fiber(root, &rt);

        let new_id = tree.get(fragment).children[0];
        assert_ne!(new_id, old_id);
        assert!(!tree.contains(old_id));
        assert_eq!(tree.kind_name(new_id), "View");
    }

    struct UnkeyedList;
    #[derive(Clone, PartialEq, Default)]
    struct UnkeyedListProps {
        count_out: Shared<Option<State<usize>>>,
    }
    impl Component for UnkeyedList {
        type Props = UnkeyedListProps;
        fn render(props: &UnkeyedListProps, hooks: &mut Hooks) -> Element {
            let count = hooks.use_state(|| 3usize);
            *props.count_out.lock() = Some(count.clone());
            Element::fragment(
                (0..count.get())
                    .map(|i| {
                        Element::text(TextProps {
                            content: i.to_string(),
                            ..Default::default()
                        })
                    })
                    .collect(),
            )
        }
    }

    #[test]
    fn unkeyed_children_match_by_index() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        std::mem::forget(_rx);
        let mut tree = FiberTree::new();
        let props = UnkeyedListProps::default();
        let root = tree.mount_root(Element::component::<UnkeyedList>(props.clone()), &rt);
        let fragment = tree.get(root).children[0];
        let before = tree.get(fragment).children.clone();
        assert_eq!(before.len(), 3);

        let count = props.count_out.lock().clone().unwrap();
        count.set(3);
        tree.render_fiber(root, &rt);
        assert_eq!(tree.get(fragment).children, before);

        count.set(2);
        tree.render_fiber(root, &rt);
        let after = tree.get(fragment).children.clone();
        assert_eq!(after, vec![before[0], before[1]]);
        assert!(!tree.contains(before[2]));
    }

    struct PaddedView;
    #[derive(Clone, PartialEq, Default)]
    struct PaddedViewProps {
        padding_out: Shared<Option<State<u16>>>,
    }
    impl Component for PaddedView {
        type Props = PaddedViewProps;
        fn render(props: &PaddedViewProps, hooks: &mut Hooks) -> Element {
            let padding = hooks.use_state(|| 0u16);
            *props.padding_out.lock() = Some(padding.clone());
            Element::view(
                crate::props::ViewProps {
                    padding: padding.get(),
                    ..Default::default()
                },
                vec![Element::text(TextProps {
                    content: "child".into(),
                    ..Default::default()
                })],
            )
        }
    }

    #[test]
    fn view_prop_update_in_place_sets_layout_dirty() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        std::mem::forget(_rx);
        let mut tree = FiberTree::new();
        let props = PaddedViewProps::default();
        let root = tree.mount_root(Element::component::<PaddedView>(props.clone()), &rt);
        let view_id = tree.get(root).children[0];

        let padding = props.padding_out.lock().clone().unwrap();
        tree.layout_dirty = false;
        padding.set(5);
        tree.render_fiber(root, &rt);
        assert_eq!(tree.get(root).children[0], view_id);
        assert!(tree.layout_dirty);

        // No-op re-render (no state change): must not flag layout dirty.
        tree.layout_dirty = false;
        tree.render_fiber(root, &rt);
        assert!(!tree.layout_dirty);
    }

    struct ProviderHost;
    #[derive(Clone, PartialEq, Default)]
    struct ProviderHostProps {
        value_out: Shared<Option<State<u32>>>,
    }
    impl Component for ProviderHost {
        type Props = ProviderHostProps;
        fn render(props: &ProviderHostProps, hooks: &mut Hooks) -> Element {
            let value = hooks.use_state(|| 1u32);
            *props.value_out.lock() = Some(value.clone());
            Element::provider(
                value.get(),
                vec![Element::text(TextProps {
                    content: "child".into(),
                    ..Default::default()
                })],
            )
        }
    }

    #[test]
    fn provider_value_swap_reconciles_children() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        std::mem::forget(_rx);
        let mut tree = FiberTree::new();
        let props = ProviderHostProps::default();
        let root = tree.mount_root(Element::component::<ProviderHost>(props.clone()), &rt);
        let provider_id = tree.get(root).children[0];
        assert_eq!(tree.kind_name(provider_id), "Provider");

        let value = props.value_out.lock().clone().unwrap();
        value.set(2);
        tree.render_fiber(root, &rt);

        assert_eq!(tree.get(root).children[0], provider_id);
        let crate::fiber::FiberKind::Provider { value: v, .. } = &tree.get(provider_id).kind else {
            panic!("expected Provider fiber");
        };
        assert_eq!(*v.downcast_ref::<u32>().unwrap(), 2);
    }

    #[test]
    fn render_fiber_guards_missing_id_and_non_component() {
        use crate::props::ViewProps;
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        let root = tree.mount_root(Element::view(ViewProps::default(), vec![]), &rt);
        tree.render_fiber(root, &rt); // non-component fiber -> early return
        tree.render_fiber(9999, &rt); // missing id -> early return
    }
}
