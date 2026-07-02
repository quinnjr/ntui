// Wired into the runtime in a later task; unused-outside-tests until then.
#![allow(dead_code)]

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
        let mut slots = std::mem::take(&mut self.get_mut(id).hooks);
        let child_el = {
            let FiberKind::Component(c) = &self.get(id).kind else {
                unreachable!()
            };
            let name = c.name();
            let mut hooks = Hooks::new(&mut slots, name, id, rt.clone(), first_render);
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
        let mut old_keyed: HashMap<String, FiberId> = HashMap::new();
        let mut old_indexed: Vec<Option<FiberId>> = Vec::new();
        for id in old {
            match self.get(id).key.clone() {
                Some(k) => {
                    old_keyed.insert(k, id);
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
        self.get_mut(parent).children = next;
        // Note: reorder of matched children is structural — flag layout.
        self.layout_dirty = true;
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
}
