// The fiber tree is only driven end-to-end once the runtime (later tasks)
// exists; until then several items here are only exercised by unit tests.
#![allow(dead_code)]

use std::any::TypeId;
use std::collections::HashMap;
use std::rc::Rc;

use crate::component::AnyComponent;
use crate::element::{Element, Node};
use crate::hooks::{HookSlot, RuntimeHandle};
use crate::props::{TextProps, ViewProps};

pub(crate) type FiberId = usize;

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
    pub rendered_once: bool,
}

pub(crate) struct FiberTree {
    fibers: HashMap<FiberId, Fiber>,
    next_id: FiberId,
    pub root: Option<FiberId>,
    /// Set on any structural change or host-prop change; cleared by layout.
    pub layout_dirty: bool,
}

impl FiberTree {
    pub fn new() -> Self {
        FiberTree {
            fibers: HashMap::new(),
            next_id: 0,
            root: None,
            layout_dirty: false,
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
    pub fn len(&self) -> usize {
        self.fibers.len()
    }

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
        self.fibers.insert(
            id,
            Fiber {
                kind,
                key: el.key,
                hooks: Vec::new(),
                children: Vec::new(),
                parent,
                layout: Rect::default(),
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
        for slot in fiber.hooks {
            slot.unmount();
        }
        self.layout_dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
    use crate::element::Element;
    use crate::hooks::{Hooks, RuntimeHandle};
    use crate::props::{TextProps, ViewProps};

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
}
