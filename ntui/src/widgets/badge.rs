use crate::component::Component;
use crate::element::Element;
use crate::hooks::Hooks;
use crate::props::{TextProps, ViewProps};
use crate::style::Weight;

/// A `Badge`'s semantic color, independent of the theme's accent hue.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tone {
    #[default]
    Neutral,
    Accent,
    Success,
    Danger,
}

/// A small colored label chip, e.g. a status tag or count.
#[derive(Clone, PartialEq, Default)]
pub struct BadgeProps {
    pub label: String,
    pub tone: Tone,
}

pub struct Badge;
impl Component for Badge {
    type Props = BadgeProps;
    fn render(props: &BadgeProps, hooks: &mut Hooks) -> Element {
        let theme = hooks.use_theme();
        let (bg, fg) = match props.tone {
            Tone::Neutral => (theme.surface, theme.foreground),
            Tone::Accent => (theme.accent, theme.surface),
            Tone::Success => (theme.success, theme.surface),
            Tone::Danger => (theme.danger, theme.surface),
        };
        Element::view(
            ViewProps {
                padding: 1,
                background: bg,
                ..Default::default()
            },
            vec![Element::text(TextProps {
                content: props.label.clone(),
                color: fg,
                weight: Weight::Bold,
                ..Default::default()
            })],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestTerminal;
    use crate::widgets::theme::Theme;

    #[tokio::test]
    async fn danger_tone_uses_the_theme_danger_color() {
        let theme = Theme::default();
        let (rt, _rx) = crate::hooks::RuntimeHandle::test_handle();
        let mut tree = crate::fiber::FiberTree::new();
        tree.mount_root(
            Element::component::<Badge>(BadgeProps {
                label: "x".into(),
                tone: Tone::Danger,
            }),
            &rt,
        );
        crate::layout::compute_layout(&mut tree, 5, 3);
        let mut buf = crate::buffer::Buffer::new(5, 3);
        crate::paint::paint(&tree, &mut buf);
        assert_eq!(buf.get(1, 1).bg, theme.danger);
    }

    #[tokio::test]
    async fn label_is_visible() {
        let t = TestTerminal::new(
            10,
            3,
            Element::component::<Badge>(BadgeProps {
                label: "new".into(),
                tone: Tone::Neutral,
            }),
        )
        .unwrap();
        assert!(t.frame_text().contains("new"));
    }
}
