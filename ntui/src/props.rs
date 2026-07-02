use crate::style::{BorderStyle, Color, Weight};

/// A size along one axis: automatically sized to content, a fixed number of
/// terminal cells, or a percentage of the parent's size.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Dimension {
    #[default]
    Auto,
    Cells(u16),
    Percent(f32),
}

/// The axis a `View`'s children are laid out along.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
}

/// Style and layout properties for a `View` box, passed to flexbox layout
/// via `taffy` and then to painting.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ViewProps {
    pub flex_direction: FlexDirection,
    pub flex_grow: f32,
    pub gap: u16,
    pub padding: u16,
    pub margin: u16,
    /// Box width; `Auto` sizes to content/flex.
    pub width: Dimension,
    /// Box height; `Auto` sizes to content/flex.
    pub height: Dimension,
    pub border_style: BorderStyle,
    pub border_color: Color,
    pub background: Color,
}

/// How text overflowing its box width is handled.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TextWrap {
    #[default]
    /// Break onto additional lines at word boundaries.
    Wrap,
    /// Cut off at the box width, discarding the remainder.
    Truncate,
}

/// Style properties for a `Text` leaf.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct TextProps {
    pub content: String,
    pub color: Color,
    pub weight: Weight,
    pub wrap: TextWrap,
}
