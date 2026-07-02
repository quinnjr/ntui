use crate::style::{BorderStyle, Color, Weight};

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Dimension {
    #[default]
    Auto,
    Cells(u16),
    Percent(f32),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct ViewProps {
    pub flex_direction: FlexDirection,
    pub flex_grow: f32,
    pub gap: u16,
    pub padding: u16,
    pub margin: u16,
    pub width: Dimension,
    pub height: Dimension,
    pub border_style: BorderStyle,
    pub border_color: Color,
    pub background: Color,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TextWrap {
    #[default]
    Wrap,
    Truncate,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct TextProps {
    pub content: String,
    pub color: Color,
    pub weight: Weight,
    pub wrap: TextWrap,
}
