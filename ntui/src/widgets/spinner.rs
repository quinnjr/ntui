use std::time::Duration;

use crate::component::Component;
use crate::element::Element;
use crate::hooks::Hooks;
use crate::props::{FlexDirection, TextProps, ViewProps};
use crate::style::Color;

const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// An animated braille spinner, optionally followed by a label.
#[derive(Clone, PartialEq, Default)]
pub struct SpinnerProps {
    /// Text shown after the spinning glyph; omitted entirely if empty.
    pub label: String,
    /// Overrides the theme's accent color for the spinning glyph.
    pub color: Option<Color>,
}

pub struct Spinner;
impl Component for Spinner {
    type Props = SpinnerProps;
    fn render(props: &SpinnerProps, hooks: &mut Hooks) -> Element {
        let frame = hooks.use_state(|| 0usize);
        let f = frame.clone();
        hooks.use_interval(Duration::from_millis(80), move || {
            f.update(|n| *n = (*n + 1) % FRAMES.len());
        });
        let theme = hooks.use_theme();

        let mut children = vec![Element::text(TextProps {
            content: FRAMES[frame.get()].to_string(),
            color: props.color.unwrap_or(theme.accent),
            ..Default::default()
        })];
        if !props.label.is_empty() {
            children.push(Element::text(TextProps {
                content: props.label.clone(),
                color: theme.foreground,
                ..Default::default()
            }));
        }

        Element::view(
            ViewProps {
                flex_direction: FlexDirection::Row,
                gap: 1,
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

    #[tokio::test(start_paused = true)]
    async fn advances_frames_and_shows_the_label() {
        let mut t = TestTerminal::new(
            20,
            1,
            Element::component::<Spinner>(SpinnerProps {
                label: "loading".into(),
                ..Default::default()
            }),
        )
        .unwrap();
        assert!(t.frame_text().contains(FRAMES[0]));
        assert!(t.frame_text().contains("loading"));

        tokio::time::sleep(Duration::from_millis(90)).await;
        t.tick().await.unwrap();
        assert!(t.frame_text().contains(FRAMES[1]));
    }

    #[tokio::test(start_paused = true)]
    async fn empty_label_shows_only_the_glyph() {
        let t = TestTerminal::new(
            10,
            1,
            Element::component::<Spinner>(SpinnerProps::default()),
        )
        .unwrap();
        assert_eq!(t.frame_text().trim(), FRAMES[0]);
    }
}
