use std::any::TypeId;
use std::rc::Rc;

use crate::hooks::Hooks;

impl<'a> Hooks<'a> {
    /// Value from the nearest enclosing `Element::provider::<T>` /
    /// `ContextProvider` above this component. Not slot-based.
    ///
    /// v1 limitation: a provider value change reaches children through the
    /// normal re-render flow; a child skipped by props_eq sees the new value
    /// on its next render, not immediately.
    pub fn use_context<T: 'static>(&mut self) -> Option<Rc<T>> {
        self.context
            .get(&TypeId::of::<T>())
            .cloned()
            .and_then(|v| v.downcast::<T>().ok())
    }
}

#[cfg(test)]
mod tests {
    use crate::component::Component;
    use crate::element::Element;
    use crate::hooks::Hooks;
    use crate::props::TextProps;
    use crate::testing::TestTerminal;

    #[derive(Debug, PartialEq)]
    struct Theme(&'static str);

    struct Reader;
    #[derive(Clone, PartialEq, Default)]
    struct ReaderProps;
    impl Component for Reader {
        type Props = ReaderProps;
        fn render(_: &ReaderProps, hooks: &mut Hooks) -> Element {
            let theme = hooks.use_context::<Theme>();
            let name = theme.map(|t| t.0).unwrap_or("none");
            Element::text(TextProps {
                content: format!("theme={name}"),
                ..Default::default()
            })
        }
    }

    #[tokio::test]
    async fn nearest_provider_wins() {
        let el = Element::provider(
            Theme("dark"),
            vec![Element::fragment(vec![Element::provider(
                Theme("light"),
                vec![Element::component::<Reader>(ReaderProps)],
            )])],
        );
        let t = TestTerminal::new(20, 1, el).unwrap();
        assert!(t.frame_text().contains("theme=light"));
    }

    #[tokio::test]
    async fn missing_provider_yields_none() {
        let t = TestTerminal::new(20, 1, Element::component::<Reader>(ReaderProps)).unwrap();
        assert!(t.frame_text().contains("theme=none"));
    }
}
