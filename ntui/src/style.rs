#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Color {
    #[default]
    Reset,
    Black, Red, Green, Yellow, Blue, Magenta, Cyan, White, DarkGrey,
    Rgb(u8, u8, u8),
    Ansi(u8),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Attrs {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Weight {
    #[default]
    Normal,
    Bold,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BorderStyle {
    #[default]
    None,
    Single,
    Round,
    Double,
}
