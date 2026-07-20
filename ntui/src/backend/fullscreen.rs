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
        // `Buffer::diff` emits updates in row-major, left-to-right order, so
        // consecutive entries that share a row, are contiguous in x, and have
        // identical styling can be coalesced into a single styled run: one
        // MoveTo + one style set + one multi-char Print.
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
                self.out,
                cursor::MoveTo(start.x, start.y),
                style::SetAttribute(style::Attribute::Reset),
                style::SetAttributes(attrs),
                style::SetForegroundColor(to_ct(start.cell.fg)),
                style::SetBackgroundColor(to_ct(start.cell.bg)),
                style::Print(run),
            )?;

            i = j;
        }
        self.out.flush()
    }
}
