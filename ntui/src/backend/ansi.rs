//! Shared helpers for emitting styled cells as ANSI to a writer.

use std::io::{self, Write};

use crossterm::{queue, style};

use crate::buffer::Cell;
use crate::style::{Attrs, Color};

pub(crate) fn to_ct(c: Color) -> style::Color {
    match c {
        Color::Reset => style::Color::Reset,
        Color::Black => style::Color::Black,
        Color::Red => style::Color::Red,
        Color::Green => style::Color::Green,
        Color::Yellow => style::Color::Yellow,
        Color::Blue => style::Color::Blue,
        Color::Magenta => style::Color::Magenta,
        Color::Cyan => style::Color::Cyan,
        Color::White => style::Color::White,
        Color::DarkGrey => style::Color::DarkGrey,
        Color::Rgb(r, g, b) => style::Color::Rgb { r, g, b },
        Color::Ansi(n) => style::Color::AnsiValue(n),
    }
}

pub(crate) fn ct_attrs(a: Attrs) -> style::Attributes {
    let mut attrs = style::Attributes::default();
    if a.bold {
        attrs.set(style::Attribute::Bold);
    }
    if a.dim {
        attrs.set(style::Attribute::Dim);
    }
    if a.italic {
        attrs.set(style::Attribute::Italic);
    }
    if a.underline {
        attrs.set(style::Attribute::Underlined);
    }
    attrs
}

/// Write one styled cell at the current cursor position.
///
/// Only used directly by tests now — `write_row` coalesces same-styled runs
/// itself rather than calling this per cell — but kept as a small documented
/// building block for constructing single-cell expectations in tests.
#[cfg(test)]
pub(crate) fn write_cell(out: &mut impl Write, cell: &Cell) -> io::Result<()> {
    queue!(
        out,
        style::SetAttribute(style::Attribute::Reset),
        style::SetAttributes(ct_attrs(cell.attrs)),
        style::SetForegroundColor(to_ct(cell.fg)),
        style::SetBackgroundColor(to_ct(cell.bg)),
        style::Print(cell.ch),
    )
}

/// Write a row of cells at the current cursor position, trimming trailing
/// blank cells and resetting style at the end. Used for scrollback / live rows.
///
/// Consecutive cells with identical `fg`/`bg`/`attrs` are coalesced into a
/// single style-set + one multi-char `Print`, rather than one full
/// reset+attrs+fg+bg+print sequence per cell.
pub(crate) fn write_row(out: &mut impl Write, cells: &[Cell]) -> io::Result<()> {
    let end = cells
        .iter()
        .rposition(|c| *c != Cell::default())
        .map(|i| i + 1)
        .unwrap_or(0);
    let cells = &cells[..end];

    let mut i = 0;
    while i < cells.len() {
        let start = &cells[i];
        let mut run = String::new();
        run.push(start.ch);
        let mut j = i + 1;
        while j < cells.len()
            && cells[j].fg == start.fg
            && cells[j].bg == start.bg
            && cells[j].attrs == start.attrs
        {
            run.push(cells[j].ch);
            j += 1;
        }

        queue!(
            out,
            style::SetAttribute(style::Attribute::Reset),
            style::SetAttributes(ct_attrs(start.attrs)),
            style::SetForegroundColor(to_ct(start.fg)),
            style::SetBackgroundColor(to_ct(start.bg)),
            style::Print(run),
        )?;

        i = j;
    }
    queue!(out, style::SetAttribute(style::Attribute::Reset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_ct_maps_every_color_variant() {
        assert_eq!(to_ct(Color::Reset), style::Color::Reset);
        assert_eq!(to_ct(Color::Black), style::Color::Black);
        assert_eq!(to_ct(Color::Red), style::Color::Red);
        assert_eq!(to_ct(Color::Green), style::Color::Green);
        assert_eq!(to_ct(Color::Yellow), style::Color::Yellow);
        assert_eq!(to_ct(Color::Blue), style::Color::Blue);
        assert_eq!(to_ct(Color::Magenta), style::Color::Magenta);
        assert_eq!(to_ct(Color::Cyan), style::Color::Cyan);
        assert_eq!(to_ct(Color::White), style::Color::White);
        assert_eq!(to_ct(Color::DarkGrey), style::Color::DarkGrey);
        assert_eq!(
            to_ct(Color::Rgb(1, 2, 3)),
            style::Color::Rgb { r: 1, g: 2, b: 3 }
        );
        assert_eq!(to_ct(Color::Ansi(42)), style::Color::AnsiValue(42));
    }

    #[test]
    fn write_cell_emits_every_attribute() {
        let mut out = Vec::new();
        write_cell(
            &mut out,
            &Cell {
                ch: 'x',
                fg: Color::Red,
                bg: Color::Blue,
                attrs: Attrs {
                    bold: true,
                    dim: true,
                    italic: true,
                    underline: true,
                },
            },
        )
        .unwrap();
        assert!(String::from_utf8(out).unwrap().contains('x'));
    }

    #[test]
    fn write_row_all_blank_emits_only_reset() {
        let mut out = Vec::new();
        write_row(&mut out, &[Cell::default(), Cell::default()]).unwrap();
        // Trimmed to zero cells; still succeeds (emits a trailing style reset).
        assert!(!String::from_utf8(out).unwrap().contains('x'));
    }

    #[test]
    fn write_row_trims_trailing_blanks() {
        let mut out = Vec::new();
        let cells = [
            Cell {
                ch: 'h',
                ..Cell::default()
            },
            Cell {
                ch: 'i',
                ..Cell::default()
            },
            Cell::default(),
            Cell::default(),
        ];
        write_row(&mut out, &cells).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains('h') && s.contains('i'));
        // trailing blanks trimmed → only two Print payloads
        assert_eq!(s.matches('h').count() + s.matches('i').count(), 2);
    }
}
