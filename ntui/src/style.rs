/// A terminal color: a named ANSI color, the terminal's default (`Reset`),
/// a 24-bit RGB value, or an indexed ANSI 256 color.
#[non_exhaustive]
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

impl Color {
    /// Resolves this color to 24-bit RGB for blending. Named ANSI colors use
    /// their standard terminal RGB values; `Ansi(n)` is approximated via the
    /// standard 256-color palette; `Reset` falls back to a mid-grey, since it
    /// has no fixed color of its own.
    pub fn to_rgb(self) -> (u8, u8, u8) {
        match self {
            Color::Reset => (128, 128, 128),
            Color::Black => (0, 0, 0),
            Color::Red => (205, 49, 49),
            Color::Green => (13, 188, 121),
            Color::Yellow => (229, 229, 16),
            Color::Blue => (36, 114, 200),
            Color::Magenta => (188, 63, 188),
            Color::Cyan => (17, 168, 205),
            Color::White => (229, 229, 229),
            Color::DarkGrey => (102, 102, 102),
            Color::Rgb(r, g, b) => (r, g, b),
            Color::Ansi(n) => ansi_256_to_rgb(n),
        }
    }

    /// Linearly interpolates between two colors in RGB space. `t` is clamped
    /// to `[0.0, 1.0]`; the result is always [`Color::Rgb`].
    pub fn lerp(a: Color, b: Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let (ar, ag, ab) = a.to_rgb();
        let (br, bg, bb) = b.to_rgb();
        let mix = |x: u8, y: u8| -> u8 {
            (x as f32 + (y as f32 - x as f32) * t)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        Color::Rgb(mix(ar, br), mix(ag, bg), mix(ab, bb))
    }
}

/// Approximates a 256-color ANSI index as 24-bit RGB (standard xterm palette).
fn ansi_256_to_rgb(n: u8) -> (u8, u8, u8) {
    const RAMP: [u8; 6] = [0, 95, 135, 175, 215, 255];
    match n {
        0..=15 => {
            // Standard + bright 16-color table (xterm defaults).
            const BASE: [(u8, u8, u8); 16] = [
                (0, 0, 0),
                (205, 49, 49),
                (13, 188, 121),
                (229, 229, 16),
                (36, 114, 200),
                (188, 63, 188),
                (17, 168, 205),
                (229, 229, 229),
                (102, 102, 102),
                (241, 76, 76),
                (35, 209, 139),
                (245, 245, 67),
                (59, 142, 234),
                (214, 112, 214),
                (41, 184, 219),
                (255, 255, 255),
            ];
            BASE[n as usize]
        }
        16..=231 => {
            let i = n - 16;
            let r = RAMP[(i / 36) as usize];
            let g = RAMP[((i / 6) % 6) as usize];
            let b = RAMP[(i % 6) as usize];
            (r, g, b)
        }
        232..=255 => {
            let level = 8 + (n - 232) * 10;
            (level, level, level)
        }
    }
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
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Weight {
    #[default]
    Normal,
    Bold,
}

/// The line style drawn around a `View`'s border; `None` draws no border.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BorderStyle {
    #[default]
    None,
    Single,
    Round,
    Double,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_at_endpoints_returns_the_endpoint_colors() {
        let a = Color::Rgb(10, 20, 30);
        let b = Color::Rgb(200, 100, 50);
        assert_eq!(Color::lerp(a, b, 0.0), a);
        assert_eq!(Color::lerp(a, b, 1.0), b);
    }

    #[test]
    fn lerp_midpoint_averages_channels() {
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(100, 200, 255);
        assert_eq!(Color::lerp(a, b, 0.5), Color::Rgb(50, 100, 128));
    }

    #[test]
    fn lerp_clamps_out_of_range_t() {
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(255, 255, 255);
        assert_eq!(Color::lerp(a, b, -1.0), a);
        assert_eq!(Color::lerp(a, b, 2.0), b);
    }

    #[test]
    fn to_rgb_passes_through_rgb_variant() {
        assert_eq!(Color::Rgb(1, 2, 3).to_rgb(), (1, 2, 3));
    }

    #[test]
    fn ansi_256_cube_resolves_known_indices() {
        // Cube origin corner (16 = i=0 -> RAMP[0]=0 for r,g,b).
        assert_eq!(Color::Ansi(16).to_rgb(), (0, 0, 0));
        // Cube max corner (231 = i=215 -> RAMP[5]=255 for r,g,b).
        assert_eq!(Color::Ansi(231).to_rgb(), (255, 255, 255));
        // Base-table spot check: index 1 ("red") is a direct lookup, matching
        // the BASE table entry (and the Color::Red variant's own RGB above).
        assert_eq!(Color::Ansi(1).to_rgb(), (205, 49, 49));
    }

    #[test]
    fn ansi_256_grey_ramp_is_monochrome() {
        let (r, g, b) = Color::Ansi(232).to_rgb();
        assert_eq!(r, g);
        assert_eq!(g, b);
        let (r2, _, _) = Color::Ansi(255).to_rgb();
        assert!(r2 > r, "grey ramp should climb toward white");
    }
}
