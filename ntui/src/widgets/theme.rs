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
///
/// # When these tokens are not enough
///
/// `Theme` is deliberately small: eight tokens that mean the same thing in
/// every app, so `ntui::widgets` can share a palette without inventing
/// vocabulary for domains it knows nothing about. An app whose colors carry
/// meaning of their own — a monitor that needs distinct shades for user,
/// system, nice, irq and iowait time all at once, an editor with syntax
/// classes, a diff view — will run out of tokens here, and stretching
/// `accent`/`danger`/`success` to cover them loses the meaning.
///
/// The escape hatch is that context is not limited to `Theme`. Define your
/// own palette type and provide it the same way; the two coexist, with
/// `Theme` continuing to style the first-party widgets underneath.
///
/// ```
/// use ntui::{Color, Component, Element, Hooks, props::TextProps};
///
/// #[derive(Clone, Copy, PartialEq)]
/// struct Palette { user: Color, system: Color, iowait: Color }
///
/// struct Meter;
/// # #[derive(Clone, PartialEq, Default)]
/// # struct MeterProps;
/// impl Component for Meter {
///     type Props = MeterProps;
///     fn render(_: &MeterProps, hooks: &mut Hooks) -> Element {
///         // The app's own vocabulary, alongside the widget theme.
///         let palette = hooks.use_context::<Palette>();
///         let color = palette.map(|p| p.user).unwrap_or(Color::Green);
///         Element::text(TextProps { content: "▇▇▇".into(), color, ..Default::default() })
///     }
/// }
/// ```
///
/// Provide it above the tree with
/// `Element::provider(my_palette, vec![...])`, or the `ContextProvider`
/// widget inside `element!`.
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
