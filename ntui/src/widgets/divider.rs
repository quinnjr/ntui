use crate::component::Component;
use crate::element::Element;
use crate::hooks::Hooks;
use crate::props::{AlignItems, FlexDirection, TextProps, TextWrap, ViewProps};

// A short, fixed-width flanking rule either side of a label. Deliberately
// not `flex_grow`-to-fill: `Text`'s min-content size under taffy's flex
// algorithm is its full (unwrapped) content length regardless of
// `TextWrap::Truncate`, so a `flex_grow` sibling holding a long rule string
// would refuse to shrink below that and starve the label instead of
// flanking it. The bare (unlabeled) divider below doesn't have this problem
// since it's the sole child, so it can just size the rule to the terminal
// width directly instead of relying on flex/truncation.
const FLANK: &str = "───";

/// A horizontal rule, optionally with a label flanked by short rules.
#[derive(Clone, PartialEq, Default)]
pub struct DividerProps {
    pub label: String,
}

pub struct Divider;
impl Component for Divider {
    type Props = DividerProps;
    fn render(props: &DividerProps, hooks: &mut Hooks) -> Element {
        let theme = hooks.use_theme();
        if props.label.is_empty() {
            let (width, _) = hooks.use_terminal_size();
            return Element::text(TextProps {
                content: "─".repeat(width as usize),
                color: theme.border,
                wrap: TextWrap::Truncate,
                ..Default::default()
            });
        }
        let flank = || {
            Element::text(TextProps {
                content: FLANK.to_string(),
                color: theme.border,
                ..Default::default()
            })
        };
        Element::view(
            ViewProps {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                gap: 1,
                ..Default::default()
            },
            vec![
                flank(),
                Element::text(TextProps {
                    content: props.label.clone(),
                    color: theme.muted,
                    ..Default::default()
                }),
                flank(),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestTerminal;

    #[tokio::test]
    async fn bare_divider_fills_the_width() {
        let t = TestTerminal::new(
            10,
            1,
            Element::component::<Divider>(DividerProps::default()),
        )
        .unwrap();
        assert_eq!(t.frame_text(), "─".repeat(10));
    }

    #[tokio::test]
    async fn labeled_divider_shows_the_label_between_rules() {
        let t = TestTerminal::new(
            11,
            1,
            Element::component::<Divider>(DividerProps { label: "hi".into() }),
        )
        .unwrap();
        let out = t.frame_text();
        assert_eq!(out, "─── hi ─── ");
    }
}
