use crate::style::{BorderStyle, Color, Weight};

/// A size along one axis: automatically sized to content, a fixed number of
/// terminal cells, or a percentage of the parent's size.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Dimension {
    #[default]
    Auto,
    Cells(u16),
    Percent(f32),
}

/// The axis a `View`'s children are laid out along.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
}

/// Distributes children along the main axis (the one named by `flex_direction`).
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum JustifyContent {
    #[default]
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Aligns children across the cross axis (perpendicular to `flex_direction`).
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AlignItems {
    #[default]
    Stretch,
    Start,
    End,
    Center,
}

/// How content exceeding a `View`'s box is handled.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Overflow {
    #[default]
    /// Content may spill past the box edges and is painted there.
    Visible,
    /// Content is clipped to the box.
    Clip,
    /// Content is clipped to the box and can be scrolled by a scroll offset.
    Scroll,
}

/// Style and layout properties for a `View` box, passed to flexbox layout
/// via `taffy` and then to painting.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ViewProps {
    pub flex_direction: FlexDirection,
    /// Main-axis distribution of children.
    pub justify_content: JustifyContent,
    /// Cross-axis alignment of children.
    pub align_items: AlignItems,
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
    /// How children exceeding the box are handled (clip / scroll).
    pub overflow: Overflow,
}

/// How text overflowing its box width is handled.
#[non_exhaustive]
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
