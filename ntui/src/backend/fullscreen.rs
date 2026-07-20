use std::io::{self, BufWriter, Stdout, Write};

use crossterm::{cursor, execute, queue, style, terminal};

use crate::backend::Backend;
use crate::backend::ansi::to_ct;
use crate::buffer::CellUpdate;

/// Alternate-screen, raw-mode terminal backend.
/// `flush` coalesces consecutive same-row, same-style, contiguous changed
/// cells into a single MoveTo+style+Print run.
pub struct FullscreenBackend {
    out: BufWriter<Stdout>,
}

// Real-terminal I/O: requires a TTY, so it is exercised by the examples rather
// than unit tests and excluded from coverage.
#[cfg_attr(coverage_nightly, coverage(off))]
impl FullscreenBackend {
    pub fn new() -> Self {
        FullscreenBackend {
            out: BufWriter::new(io::stdout()),
        }
    }
}

impl Default for FullscreenBackend {
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Backend for FullscreenBackend {
    fn size(&self) -> io::Result<(u16, u16)> {
        terminal::size()
    }

    fn enter(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        execute!(
            self.out,
            terminal::EnterAlternateScreen,
            cursor::Hide,
            terminal::Clear(terminal::ClearType::All)
        )
        .inspect_err(|_| {
            // Raw mode is already on and we may have partially entered the
            // alternate screen / hidden the cursor; undo whatever happened so
            // a failed enter never leaks a broken shell.
            let _ = terminal::disable_raw_mode();
            let _ = execute!(self.out, terminal::LeaveAlternateScreen, cursor::Show);
        })
    }

    fn leave(&mut self) -> io::Result<()> {
        execute!(self.out, cursor::Show, terminal::LeaveAlternateScreen)?;
        terminal::disable_raw_mode()
    }

    fn flush(&mut self, updates: &[CellUpdate]) -> io::Result<()> {
        write_coalesced_updates(&mut self.out, updates)?;
        self.out.flush()
    }
}

/// Coalesce consecutive same-row, same-style, contiguous changed cells into a
/// single MoveTo + style set + multi-char `Print`, and write them to `out`.
///
/// `Buffer::diff` emits updates in row-major, left-to-right order, so
/// consecutive entries that share a row, are contiguous in x, and have
/// identical styling can be coalesced this way. Extracted from
/// `FullscreenBackend::flush` (which writes to a real TTY handle and so isn't
/// itself unit-testable) so the coalescing logic can be exercised against a
/// plain `Vec<u8>` in tests.
fn write_coalesced_updates(out: &mut impl Write, updates: &[CellUpdate]) -> io::Result<()> {
    let mut i = 0;
    while i < updates.len() {
        let start = &updates[i];
        let mut run = String::new();
        run.push(start.cell.ch);
        let mut j = i + 1;
        while j < updates.len() {
            let prev = &updates[j - 1];
            let cur = &updates[j];
            let contiguous = cur.y == prev.y && cur.x == prev.x + 1;
            let same_style = cur.cell.fg == start.cell.fg
                && cur.cell.bg == start.cell.bg
                && cur.cell.attrs == start.cell.attrs;
            if !contiguous || !same_style {
                break;
            }
            run.push(cur.cell.ch);
            j += 1;
        }

        let attrs = crate::backend::ansi::ct_attrs(start.cell.attrs);
        queue!(
            out,
            cursor::MoveTo(start.x, start.y),
            style::SetAttribute(style::Attribute::Reset),
            style::SetAttributes(attrs),
            style::SetForegroundColor(to_ct(start.cell.fg)),
            style::SetBackgroundColor(to_ct(start.cell.bg)),
            style::Print(run),
        )?;

        i = j;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Cell;
    use crate::style::{Attrs, Color};

    fn update(x: u16, y: u16, ch: char, fg: Color, bg: Color, attrs: Attrs) -> CellUpdate {
        CellUpdate {
            x,
            y,
            cell: Cell { ch, fg, bg, attrs },
        }
    }

    #[test]
    fn merges_two_contiguous_same_styled_updates_into_one_print() {
        let mut out = Vec::new();
        let updates = [
            update(0, 0, 'h', Color::Red, Color::Reset, Attrs::default()),
            update(1, 0, 'i', Color::Red, Color::Reset, Attrs::default()),
        ];
        write_coalesced_updates(&mut out, &updates).unwrap();
        let s = String::from_utf8(out).unwrap();

        // Exactly one Print payload containing both characters merged.
        assert!(s.contains("hi"));
        // Only one MoveTo (the second cell's coordinates never appear as a
        // separate cursor move) — a single coalesced run.
        assert_eq!(s.matches("\u{1b}[1;1H").count(), 1);
    }

    #[test]
    fn style_change_between_adjacent_updates_breaks_the_run() {
        let mut out = Vec::new();
        let updates = [
            update(0, 0, 'h', Color::Red, Color::Reset, Attrs::default()),
            update(1, 0, 'i', Color::Blue, Color::Reset, Attrs::default()),
        ];
        write_coalesced_updates(&mut out, &updates).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains('h') && s.contains('i'));
        // Not merged into a single "hi" run.
        assert!(!s.contains("hi"));
        // Two separate MoveTo sequences: (0,0) and (1,0).
        assert_eq!(s.matches("\u{1b}[1;1H").count(), 1);
        assert_eq!(s.matches("\u{1b}[1;2H").count(), 1);
    }

    #[test]
    fn row_change_breaks_the_run() {
        let mut out = Vec::new();
        let updates = [
            update(0, 0, 'h', Color::Red, Color::Reset, Attrs::default()),
            update(1, 1, 'i', Color::Red, Color::Reset, Attrs::default()),
        ];
        write_coalesced_updates(&mut out, &updates).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains('h') && s.contains('i'));
        assert!(!s.contains("hi"));
        assert_eq!(s.matches("\u{1b}[1;1H").count(), 1);
        assert_eq!(s.matches("\u{1b}[2;2H").count(), 1);
    }

    #[test]
    fn non_contiguous_x_gap_breaks_the_run() {
        let mut out = Vec::new();
        let updates = [
            update(0, 0, 'h', Color::Red, Color::Reset, Attrs::default()),
            // x jumps from 0 to 2, skipping 1 — not contiguous.
            update(2, 0, 'i', Color::Red, Color::Reset, Attrs::default()),
        ];
        write_coalesced_updates(&mut out, &updates).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains('h') && s.contains('i'));
        assert!(!s.contains("hi"));
        assert_eq!(s.matches("\u{1b}[1;1H").count(), 1);
        assert_eq!(s.matches("\u{1b}[1;3H").count(), 1);
    }
}
