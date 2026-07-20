use crate::hooks::Hooks;
use crate::style::{BorderStyle, Color};

/// A small set of named color/border tokens shared by `ntui::widgets`, so a
/// screen full of widgets reads as one coherent palette instead of each
/// widget hardcoding its own colors.
///
/// Provide a custom theme to a subtree with
/// `element! { ContextProvider(value: my_theme) { ... } }`; widgets read the
/// nearest one via [`use_theme`], falling back to [`Theme::default`] when
/// none is provided.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Theme {
    /// The theme's signature color: focus rings, active/selected state,
    /// primary buttons, gradient endpoints.
    pub accent: Color,
    /// A raised surface behind widget content (cards, inputs, table rows).
    pub surface: Color,
    /// Default border color for unfocused widgets.
    pub border: Color,
    /// De-emphasized text (placeholders, disabled labels, captions).
    pub muted: Color,
    /// Primary body text color.
    pub foreground: Color,
    /// Negative/destructive state (errors, failed progress).
    pub danger: Color,
    /// Positive/complete state (success, finished progress).
    pub success: Color,
    /// Default border style for widgets that draw a box.
    pub border_style: BorderStyle,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            accent: Color::Rgb(124, 58, 237),
            surface: Color::Rgb(30, 30, 36),
            border: Color::DarkGrey,
            muted: Color::Rgb(140, 140, 150),
            foreground: Color::White,
            danger: Color::Rgb(220, 38, 38),
            success: Color::Rgb(34, 197, 94),
            border_style: BorderStyle::Round,
        }
    }
}

impl<'a> Hooks<'a> {
    /// The nearest [`Theme`] provided by an ancestor
    /// `ContextProvider(value: ...)`, or [`Theme::default`] if none is set.
    pub fn use_theme(&mut self) -> Theme {
        self.use_context::<Theme>().map(|t| *t).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
    use crate::element::Element;
    use crate::props::TextProps;
    use crate::testing::TestTerminal;

    struct Reader;
    #[derive(Clone, PartialEq, Default)]
    struct ReaderProps;
    impl Component for Reader {
        type Props = ReaderProps;
        fn render(_: &ReaderProps, hooks: &mut Hooks) -> Element {
            let theme = hooks.use_theme();
            Element::text(TextProps {
                content: format!("{:?}", theme.accent),
                ..Default::default()
            })
        }
    }

    #[tokio::test]
    async fn falls_back_to_default_without_a_provider() {
        let t = TestTerminal::new(40, 1, Element::component::<Reader>(ReaderProps)).unwrap();
        assert!(
            t.frame_text()
                .contains(&format!("{:?}", Theme::default().accent))
        );
    }

    #[tokio::test]
    async fn nearest_provided_theme_wins() {
        let custom = Theme {
            accent: Color::Rgb(1, 2, 3),
            ..Theme::default()
        };
        let el = Element::provider(custom, vec![Element::component::<Reader>(ReaderProps)]);
        let t = TestTerminal::new(40, 1, el).unwrap();
        assert!(t.frame_text().contains("Rgb(1, 2, 3)"));
    }
}
