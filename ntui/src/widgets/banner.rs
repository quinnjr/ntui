use crate::component::Component;
use crate::element::Element;
use crate::hooks::Hooks;
use crate::props::{FlexDirection, TextProps, ViewProps};
use crate::style::Weight;

/// A bordered, gradient-titled header block — for a splash screen or the top
/// of a full-screen app.
#[derive(Clone, PartialEq, Default)]
pub struct BannerProps {
    pub title: String,
    pub subtitle: String,
}

pub struct Banner;
impl Component for Banner {
    type Props = BannerProps;
    fn render(props: &BannerProps, hooks: &mut Hooks) -> Element {
        let theme = hooks.use_theme();
        let mut children = vec![Element::text(TextProps {
            content: props.title.clone(),
            color_gradient: Some((theme.accent, theme.foreground)),
            weight: Weight::Bold,
            ..Default::default()
        })];
        if !props.subtitle.is_empty() {
            children.push(Element::text(TextProps {
                content: props.subtitle.clone(),
                color: theme.muted,
                ..Default::default()
            }));
        }
        Element::view(
            ViewProps {
                flex_direction: FlexDirection::Column,
                padding: 1,
                border_style: theme.border_style,
                border_color: theme.accent,
                ..Default::default()
            },
            children,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestTerminal;

    #[tokio::test]
    async fn shows_title_and_subtitle_inside_a_border() {
        let t = TestTerminal::new(
            30,
            5,
            Element::component::<Banner>(BannerProps {
                title: "ntui".into(),
                subtitle: "terminal UIs".into(),
            }),
        )
        .unwrap();
        let out = t.frame_text();
        assert!(out.contains("ntui"), "{out:?}");
        assert!(out.contains("terminal UIs"), "{out:?}");
        assert!(out.contains('╭'), "expected a rounded border: {out:?}");
    }

    #[tokio::test]
    async fn empty_subtitle_is_omitted() {
        let t = TestTerminal::new(
            30,
            5,
            Element::component::<Banner>(BannerProps {
                title: "ntui".into(),
                subtitle: String::new(),
            }),
        )
        .unwrap();
        assert!(!t.frame_text().contains("terminal UIs"));
    }
}
