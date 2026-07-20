use std::time::Duration;

use crate::component::Component;
use crate::element::Element;
use crate::hooks::Hooks;
use crate::props::{Dimension, FlexDirection, GradientDirection, ViewProps};
use crate::style::Color;

/// A horizontal progress bar. `value` is clamped to `0.0..=1.0`.
#[derive(Clone, PartialEq, Default)]
pub struct ProgressBarProps {
    pub value: f32,
    /// Total width in cells; `0` defaults to 20.
    pub width: u16,
    /// Overrides the theme accent as a flat fill color.
    pub color: Option<Color>,
    /// Overrides `color` with a horizontal fill gradient.
    pub gradient: Option<(Color, Color)>,
    /// Smoothly animate toward `value` instead of jumping.
    pub animate: bool,
}

pub struct ProgressBar;
impl Component for ProgressBar {
    type Props = ProgressBarProps;
    fn render(props: &ProgressBarProps, hooks: &mut Hooks) -> Element {
        let theme = hooks.use_theme();
        let target = props.value.clamp(0.0, 1.0);
        let value = if props.animate {
            hooks.use_tween(target, Duration::from_millis(220))
        } else {
            target
        };

        let width = if props.width == 0 { 20 } else { props.width };
        let filled = ((width as f32) * value).round().clamp(0.0, width as f32) as u16;
        let empty = width - filled;

        let gradient = props
            .gradient
            .map(|(from, to)| (from, to, GradientDirection::Horizontal));
        let fill = Element::view(
            ViewProps {
                width: Dimension::Cells(filled),
                height: Dimension::Cells(1),
                background: if gradient.is_some() {
                    Color::Reset
                } else {
                    props.color.unwrap_or(theme.accent)
                },
                background_gradient: gradient,
                ..Default::default()
            },
            vec![],
        );
        let track = Element::view(
            ViewProps {
                width: Dimension::Cells(empty),
                height: Dimension::Cells(1),
                background: theme.surface,
                ..Default::default()
            },
            vec![],
        );

        Element::view(
            ViewProps {
                flex_direction: FlexDirection::Row,
                width: Dimension::Cells(width),
                height: Dimension::Cells(1),
                ..Default::default()
            },
            vec![fill, track],
        )
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

    fn render(props: ProgressBarProps, w: u16) -> Buffer {
        let (rt, _rx) = RuntimeHandle::test_handle();
        let mut tree = FiberTree::new();
        tree.mount_root(Element::component::<ProgressBar>(props), &rt);
        compute_layout(&mut tree, w, 1);
        let mut buf = Buffer::new(w, 1);
        paint(&tree, &mut buf);
        buf
    }

    #[tokio::test]
    async fn half_value_fills_half_the_width() {
        let theme = crate::widgets::theme::Theme::default();
        let buf = render(
            ProgressBarProps {
                value: 0.5,
                width: 10,
                ..Default::default()
            },
            10,
        );
        for x in 0..5 {
            assert_eq!(buf.get(x, 0).bg, theme.accent, "cell {x} should be filled");
        }
        for x in 5..10 {
            assert_eq!(buf.get(x, 0).bg, theme.surface, "cell {x} should be track");
        }
    }

    #[tokio::test]
    async fn value_is_clamped_to_the_valid_range() {
        let theme = crate::widgets::theme::Theme::default();
        let buf = render(
            ProgressBarProps {
                value: 5.0, // above 1.0, must clamp
                width: 5,
                ..Default::default()
            },
            5,
        );
        for x in 0..5 {
            assert_eq!(buf.get(x, 0).bg, theme.accent, "fully filled at cell {x}");
        }

        let buf = render(
            ProgressBarProps {
                value: -1.0, // below 0.0, must clamp
                width: 5,
                ..Default::default()
            },
            5,
        );
        for x in 0..5 {
            assert_eq!(buf.get(x, 0).bg, theme.surface, "fully empty at cell {x}");
        }
    }

    #[tokio::test]
    async fn explicit_color_overrides_the_theme_accent() {
        let buf = render(
            ProgressBarProps {
                value: 1.0,
                width: 4,
                color: Some(Color::Rgb(9, 9, 9)),
                ..Default::default()
            },
            4,
        );
        assert_eq!(buf.get(0, 0).bg, Color::Rgb(9, 9, 9));
    }

    #[tokio::test]
    async fn gradient_overrides_flat_color() {
        let from = Color::Rgb(255, 0, 0);
        let to = Color::Rgb(0, 0, 255);
        let buf = render(
            ProgressBarProps {
                value: 1.0,
                width: 4,
                gradient: Some((from, to)),
                ..Default::default()
            },
            4,
        );
        let start = buf.get(0, 0).bg;
        let end = buf.get(3, 0).bg;
        assert_ne!(start, end, "gradient should vary across the fill");
        assert_eq!(start, from);
        assert_eq!(end, to);
    }

    struct Animated;
    #[derive(Clone, PartialEq, Default)]
    struct AnimatedProps {
        width: u16,
        target: crate::test_util::Shared<Option<crate::hooks::state::State<f32>>>,
    }
    impl Component for Animated {
        type Props = AnimatedProps;
        fn render(props: &AnimatedProps, hooks: &mut Hooks) -> Element {
            let target = hooks.use_state(|| 0.0f32);
            *props.target.lock() = Some(target.clone());
            Element::component::<ProgressBar>(ProgressBarProps {
                value: target.get(),
                width: props.width,
                animate: true,
                ..Default::default()
            })
        }
    }

    #[tokio::test(start_paused = true)]
    async fn animated_progress_bar_moves_toward_the_target_over_time() {
        use crate::backend::TestBackend;
        use crate::runtime::AppCore;

        let width = 10;
        let props = AnimatedProps {
            width,
            ..Default::default()
        };
        let mut core = AppCore::new(Element::component::<Animated>(props.clone()), (width, 1));
        let mut backend = TestBackend::new(width, 1);
        core.process_wakes();
        core.draw(&mut backend).unwrap();

        let theme = crate::widgets::theme::Theme::default();
        let filled_at = |buf: &Buffer| {
            (0..buf.width())
                .filter(|&x| buf.get(x, 0).bg == theme.accent)
                .count()
        };

        // Retarget to the full value, then check partway through the 220ms tween.
        props.target.lock().clone().unwrap().set(1.0);
        core.process_wakes();
        core.draw(&mut backend).unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        core.process_wakes();
        core.draw(&mut backend).unwrap();
        let mid = filled_at(&backend.buffer);
        assert!(
            mid > 0 && mid < width as usize,
            "should be partway filled: {mid}"
        );

        // Past the full duration: fully filled.
        tokio::time::sleep(Duration::from_millis(300)).await;
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        core.process_wakes();
        core.draw(&mut backend).unwrap();
        let end = filled_at(&backend.buffer);
        assert_eq!(
            end, width as usize,
            "should settle at the full target width"
        );
    }
}
