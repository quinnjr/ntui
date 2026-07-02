/// A terminal color: a named ANSI color, the terminal's default (`Reset`),
/// a 24-bit RGB value, or an indexed ANSI 256 color.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Color {
    #[default]
    Reset,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    DarkGrey,
    Rgb(u8, u8, u8),
    Ansi(u8),
}

/// Text rendering attributes independent of color/weight.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Attrs {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
}

/// Font weight for a `Text` element.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Weight {
    #[default]
    Normal,
    Bold,
}

/// The line style drawn around a `View`'s border; `None` draws no border.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BorderStyle {
    #[default]
    None,
    Single,
    Round,
    Double,
}
