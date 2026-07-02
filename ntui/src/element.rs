use std::any::TypeId;
use std::rc::Rc;

use crate::component::{AnyComponent, Component, TypedComponent};
use crate::props::{TextProps, ViewProps};

pub struct Element {
    pub key: Option<String>,
    pub node: Node,
}

pub enum Node {
    View {
        props: ViewProps,
        children: Vec<Element>,
    },
    Text {
        props: TextProps,
    },
    Fragment {
        children: Vec<Element>,
    },
    Provider {
        type_id: TypeId,
        value: Rc<dyn std::any::Any>,
        children: Vec<Element>,
    },
    Component(Box<dyn AnyComponent>),
}

impl Element {
    pub fn view(props: ViewProps, children: Vec<Element>) -> Self {
        Element {
            key: None,
            node: Node::View { props, children },
        }
    }
    pub fn text(props: TextProps) -> Self {
        Element {
            key: None,
            node: Node::Text { props },
        }
    }
    pub fn fragment(children: Vec<Element>) -> Self {
        Element {
            key: None,
            node: Node::Fragment { children },
        }
    }
    pub fn provider<T: 'static>(value: T, children: Vec<Element>) -> Self {
        Element {
            key: None,
            node: Node::Provider {
                type_id: TypeId::of::<T>(),
                value: Rc::new(value),
                children,
            },
        }
    }
    pub fn component<C: Component>(props: C::Props) -> Self {
        Element {
            key: None,
            node: Node::Component(Box::new(TypedComponent::<C>::new(props))),
        }
    }
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
    use crate::hooks::Hooks;
    use crate::props::TextProps;

    struct Greeting;
    #[derive(Clone, PartialEq, Default)]
    struct GreetingProps {
        name: String,
    }
    impl Component for Greeting {
        type Props = GreetingProps;
        fn render(props: &GreetingProps, _hooks: &mut Hooks) -> Element {
            Element::text(TextProps {
                content: format!("hi {}", props.name),
                ..Default::default()
            })
        }
    }

    #[test]
    fn component_element_is_type_erased_and_props_compare() {
        let a = Element::component::<Greeting>(GreetingProps { name: "a".into() });
        let a2 = Element::component::<Greeting>(GreetingProps { name: "a".into() });
        let b = Element::component::<Greeting>(GreetingProps { name: "b".into() });
        let (Node::Component(ca), Node::Component(ca2), Node::Component(cb)) =
            (&a.node, &a2.node, &b.node)
        else {
            panic!("expected components")
        };
        assert_eq!(ca.component_type(), cb.component_type());
        assert!(ca.props_eq(ca2.as_ref()));
        assert!(!ca.props_eq(cb.as_ref()));
    }

    #[test]
    fn with_key_sets_key() {
        let el = Element::fragment(vec![]).with_key("row-1");
        assert_eq!(el.key.as_deref(), Some("row-1"));
    }
}
