use crate::hooks::scroll::Scroll;
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

/// Where an overlay `View` (see [`ViewProps::overlay`]) is placed against
/// the viewport. Corner anchors sit one cell in from both edges.
///
/// Setting `overlay` paints a `View` outside its normal place in the tree,
/// after everything else, positioned against the whole viewport instead of
/// its parent's box. Doesn't reserve space in its parent's layout (it's
/// taken out of flow entirely) and isn't clipped by any ancestor. Used by
/// [`crate::widgets::Modal`]/[`crate::widgets::Toast`]/[`crate::widgets::Tooltip`];
/// still just a `View` prop, not a new element kind.
///
/// Nesting an overlay `View` inside another overlay `View` isn't supported —
/// the inner one is not painted. In debug builds (including `cargo test` and
/// CI) this panics via a `debug_assert!` so misuse is caught during
/// development; release builds silently drop the inner overlay instead. None
/// of the built-in overlay widgets nest, so this only matters for custom
/// overlay compositions.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Anchor {
    #[default]
    Center,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
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

/// The axis a background or text gradient is interpolated across.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum GradientDirection {
    #[default]
    Horizontal,
    Vertical,
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
    /// When set, overrides `background` with a fill that interpolates
    /// between the two colors across the box, in `GradientDirection`.
    pub background_gradient: Option<(Color, Color, GradientDirection)>,
    /// How children exceeding the box are handled (clip / scroll).
    pub overflow: Overflow,
    /// Scroll position for an [`Overflow::Scroll`] box. Obtain via
    /// [`use_scroll`](crate::Hooks::use_scroll); layout feeds content/viewport
    /// sizes back into it and paint applies its offset.
    pub scroll: Option<Scroll>,
    /// See [`Anchor`]. `None` (the default) is normal, in-flow painting.
    pub overlay: Option<Anchor>,
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
    /// When set, overrides `color` with a fill interpolated between the two
    /// colors across each line's characters, left to right.
    pub color_gradient: Option<(Color, Color)>,
}
