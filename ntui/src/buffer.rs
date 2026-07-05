use crate::style::{Attrs, Color};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub attrs: Attrs,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            fg: Color::Reset,
            bg: Color::Reset,
            attrs: Attrs::default(),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct CellUpdate {
    pub x: u16,
    pub y: u16,
    pub cell: Cell,
}

#[derive(Clone, Debug)]
pub struct Buffer {
    width: u16,
    height: u16,
    cells: Vec<Cell>,
}

impl Buffer {
    pub fn new(width: u16, height: u16) -> Self {
        Buffer {
            width,
            height,
            cells: vec![Cell::default(); width as usize * height as usize],
        }
    }

    pub fn width(&self) -> u16 {
        self.width
    }
    pub fn height(&self) -> u16 {
        self.height
    }

    fn idx(&self, x: u16, y: u16) -> usize {
        y as usize * self.width as usize + x as usize
    }

    /// Panics if `x`/`y` are out of bounds (unlike `set`, which clips).
    pub fn get(&self, x: u16, y: u16) -> &Cell {
        &self.cells[self.idx(x, y)]
    }

    /// Out-of-bounds writes are silently ignored (paint clips at buffer edge).
    pub fn set(&mut self, x: u16, y: u16, cell: Cell) {
        if x < self.width && y < self.height {
            let i = self.idx(x, y);
            self.cells[i] = cell;
        }
    }

    /// Cells that differ from `prev`. If dimensions differ, every cell.
    pub fn diff(&self, prev: &Buffer) -> Vec<CellUpdate> {
        let mut out = Vec::new();
        let full = self.width != prev.width || self.height != prev.height;
        for y in 0..self.height {
            for x in 0..self.width {
                let cell = self.get(x, y);
                if full || cell != prev.get(x, y) {
                    out.push(CellUpdate { x, y, cell: *cell });
                }
            }
        }
        out
    }

    /// Plain-text grid (styles dropped), rows joined with '\n'. For tests/snapshots.
    pub fn to_text(&self) -> String {
        (0..self.height)
            .map(|y| {
                (0..self.width)
                    .map(|x| self.get(x, y).ch)
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Color;

    #[test]
    fn set_get_roundtrip_and_oob_ignored() {
        let mut b = Buffer::new(4, 2);
        b.set(
            1,
            1,
            Cell {
                ch: 'x',
                fg: Color::Red,
                ..Cell::default()
            },
        );
        b.set(
            99,
            99,
            Cell {
                ch: '!',
                ..Cell::default()
            },
        ); // must not panic
        assert_eq!(b.get(1, 1).ch, 'x');
        assert_eq!(b.get(0, 0).ch, ' ');
    }

    #[test]
    fn diff_reports_only_changed_cells() {
        let prev = Buffer::new(4, 2);
        let mut next = Buffer::new(4, 2);
        next.set(
            2,
            0,
            Cell {
                ch: 'a',
                ..Cell::default()
            },
        );
        let d = next.diff(&prev);
        assert_eq!(
            d,
            vec![CellUpdate {
                x: 2,
                y: 0,
                cell: Cell {
                    ch: 'a',
                    ..Cell::default()
                }
            }]
        );
    }

    #[test]
    fn diff_with_size_change_repaints_everything() {
        let prev = Buffer::new(2, 1);
        let next = Buffer::new(3, 1);
        assert_eq!(next.diff(&prev).len(), 3);
    }

    #[test]
    fn to_text_renders_grid() {
        let mut b = Buffer::new(3, 2);
        b.set(
            0,
            0,
            Cell {
                ch: 'h',
                ..Cell::default()
            },
        );
        b.set(
            1,
            0,
            Cell {
                ch: 'i',
                ..Cell::default()
            },
        );
        assert_eq!(b.to_text(), "hi \n   ");
    }
}
