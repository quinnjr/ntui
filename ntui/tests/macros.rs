// `Element` and `Hooks` are referenced in the `#[component]` fn signatures
// below at the source-text level, but the macro re-quotes those signature
// positions using fully-qualified `::ntui::Element` / `::ntui::Hooks` paths
// rather than the user's local idents, so rustc sees these two imports as
// structurally unused despite being semantically required by the surface
// syntax the macro accepts.
#[allow(unused_imports)]
use ntui::{Element, FlexDirection, Hooks, Node, component, element};

#[derive(Clone, PartialEq, Default)]
struct BadgeProps {
    label: String,
}

/// Renders a bracketed label, e.g. `[hello]`.
#[component]
fn Badge(props: &BadgeProps, _hooks: &mut Hooks) -> Element {
    element! { Text(content: format!("[{}]", props.label)) }
}

#[component]
fn NoProps(_hooks: &mut Hooks) -> Element {
    element! { Text(content: "static") }
}

#[test]
#[allow(clippy::useless_vec)]
fn element_macro_builds_hosts_components_keys_and_splices() {
    let items = vec!["x", "y"];
    let el = element! {
        View(flex_direction: FlexDirection::Column, gap: (1)) {
            Badge(label: "hello", key: "b1")
            NoProps
            #(items.iter().map(|i| element! { Text(content: *i, key: *i) }))
        }
    };
    let Node::View { props, children } = &el.node else {
        panic!("expected View")
    };
    assert_eq!(props.gap, 1);
    assert_eq!(children.len(), 4);
    assert_eq!(children[0].key.as_deref(), Some("b1"));
    assert!(matches!(children[1].node, Node::Component(_)));
    assert_eq!(children[2].key.as_deref(), Some("x"));
}

#[test]
fn bare_component_form_works_as_render_root() {
    let el = element!(NoProps);
    assert!(matches!(el.node, Node::Component(_)));
}
