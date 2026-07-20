use crate::component::Component;
use crate::element::Element;
use crate::hooks::Hooks;
use crate::props::{Anchor, TextProps, ViewProps};

/// A single-line hint pinned to a screen corner. Unlike a real tooltip in a
/// mouse-driven UI, this can't attach to "just above the hovered button" —
/// ntui has no pointer/hover concept — so the caller picks a screen
/// [`Anchor`] instead of a target widget.
#[derive(Clone, PartialEq, Default)]
pub struct TooltipProps {
    pub message: String,
    pub anchor: Anchor,
}

pub struct Tooltip;
impl Component for Tooltip {
    type Props = TooltipProps;
    fn render(props: &TooltipProps, hooks: &mut Hooks) -> Element {
        let theme = hooks.use_theme();
        Element::view(
            ViewProps {
                overlay: Some(props.anchor),
                padding: 0,
                background: theme.accent,
                ..Default::default()
            },
            vec![Element::text(TextProps {
                content: props.message.clone(),
                color: theme.surface,
                ..Default::default()
            })],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestTerminal;

    #[tokio::test]
    async fn shows_the_message_at_the_top_left_anchor() {
        let t = TestTerminal::new(
            20,
            5,
            Element::component::<Tooltip>(TooltipProps {
                message: "hint".into(),
                anchor: Anchor::TopLeft,
            }),
        )
        .unwrap();
        let frame = t.frame_text();
        let lines: Vec<&str> = frame.lines().collect();
        // EDGE_MARGIN (layout.rs) is 1 cell in from both edges.
        assert!(
            lines[1].starts_with(" hint"),
            "expected \"hint\" one cell in on the second line, got {:?}",
            lines[1]
        );
        for (i, line) in lines.iter().enumerate() {
            if i != 1 {
                assert!(
                    !line.contains("hint"),
                    "unexpected hint on line {i}: {line:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn shows_the_message_at_the_bottom_right_anchor() {
        let t = TestTerminal::new(
            20,
            5,
            Element::component::<Tooltip>(TooltipProps {
                message: "hint".into(),
                anchor: Anchor::BottomRight,
            }),
        )
        .unwrap();
        let frame = t.frame_text();
        let lines: Vec<&str> = frame.lines().collect();
        // width=20, message width=4, EDGE_MARGIN=1: expect x = 20-4-1 = 15;
        // height=5, box height=1: expect y = 5-1-1 = 3.
        let target_line = lines[3];
        assert!(
            target_line.trim_end().ends_with("hint"),
            "expected \"hint\" near the right edge on line 3, got {target_line:?}"
        );
        for (i, line) in lines.iter().enumerate() {
            if i != 3 {
                assert!(
                    !line.contains("hint"),
                    "unexpected hint on line {i}: {line:?}"
                );
            }
        }
    }
}
