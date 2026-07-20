use crate::component::Component;
use crate::element::Element;
use crate::hooks::Hooks;
use crate::props::TextProps;
use crate::style::{Color, Weight};

/// A run of text with its color interpolated left to right between two
/// endpoints. Leaving `from`/`to` unset falls back to the theme's
/// `accent` → `foreground` gradient.
#[derive(Clone, PartialEq, Default)]
pub struct GradientTextProps {
    pub content: String,
    pub from: Option<Color>,
    pub to: Option<Color>,
    pub weight: Weight,
}

pub struct GradientText;
impl Component for GradientText {
    type Props = GradientTextProps;
    fn render(props: &GradientTextProps, hooks: &mut Hooks) -> Element {
        let theme = hooks.use_theme();
        Element::text(TextProps {
            content: props.content.clone(),
            color_gradient: Some((
                props.from.unwrap_or(theme.accent),
                props.to.unwrap_or(theme.foreground),
            )),
            weight: props.weight,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Buffer;
    use crate::fiber::FiberTree;
    use crate::hooks::RuntimeHandle;
    use crate::layout::compute_layout;
    use crate::paint::paint;
    use crate::widgets::theme::Theme;

    #[tokio::test]
    async fn endpoints_land_at_the_first_and_last_character() {
        let theme = Theme::default();
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        tree.mount_root(
            Element::component::<GradientText>(GradientTextProps {
                content: "abc".into(),
                ..Default::default()
            }),
            &rt,
        );
        compute_layout(&mut tree, 3, 1);
        let mut buf = Buffer::new(3, 1);
        paint(&tree, &mut buf);
        // `Color::lerp` always resolves to `Color::Rgb`, so compare via RGB
        // rather than against the (possibly named) theme color variants.
        assert_eq!(buf.get(0, 0).fg.to_rgb(), theme.accent.to_rgb());
        assert_eq!(buf.get(2, 0).fg.to_rgb(), theme.foreground.to_rgb());
    }

    #[tokio::test]
    async fn explicit_endpoints_override_the_theme() {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        tree.mount_root(
            Element::component::<GradientText>(GradientTextProps {
                content: "ab".into(),
                from: Some(Color::Rgb(1, 1, 1)),
                to: Some(Color::Rgb(9, 9, 9)),
                ..Default::default()
            }),
            &rt,
        );
        compute_layout(&mut tree, 2, 1);
        let mut buf = Buffer::new(2, 1);
        paint(&tree, &mut buf);
        assert_eq!(buf.get(0, 0).fg, Color::Rgb(1, 1, 1));
        assert_eq!(buf.get(1, 0).fg, Color::Rgb(9, 9, 9));
    }
}
