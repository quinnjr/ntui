//! Inline (non-alternate-screen) rendering: finished output is printed
//! permanently and scrolls into the terminal's real scrollback, while a live
//! region at the bottom is redrawn in place each frame.

use std::io::{self, BufWriter, Stdout, Write};

use crossterm::{cursor, execute, queue, terminal};

use crate::backend::ansi::write_row;
use crate::buffer::Cell;

/// The terminal operations the inline runtime needs. Implemented by
/// [`InlineBackend`] for a real terminal and by a recording sink in tests.
pub(crate) trait InlineSink {
    fn size(&self) -> io::Result<(u16, u16)>;
    fn enter(&mut self) -> io::Result<()>;
    fn leave(&mut self) -> io::Result<()>;
    /// Print `rows` permanently above the live region — they scroll into the
    /// terminal's scrollback. Erases the current live region first.
    fn commit(&mut self, rows: &[Vec<Cell>]) -> io::Result<()>;
    /// Redraw the live region in place below any committed content.
    fn present(&mut self, rows: &[Vec<Cell>]) -> io::Result<()>;
}

/// Inline terminal backend. Enables raw mode but does **not** switch to the
/// alternate screen, so committed output becomes real, scrollable terminal
/// history. The cursor invariant between calls: it sits at the top-left of the
/// live region.
pub struct InlineBackend {
    out: BufWriter<Stdout>,
}

// Real-terminal I/O: requires a TTY, exercised by the examples and excluded
// from coverage. The inline commit/present logic is unit-tested via RecordingSink.
#[cfg_attr(coverage_nightly, coverage(off))]
impl InlineBackend {
    pub fn new() -> Self {
        InlineBackend {
            out: BufWriter::new(io::stdout()),
        }
    }
}

impl Default for InlineBackend {
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl InlineSink for InlineBackend {
    fn size(&self) -> io::Result<(u16, u16)> {
        terminal::size()
    }

    fn enter(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        // Start the live region on a fresh line at column 0; no screen clear —
        // whatever is already in the terminal stays as scrollback above us.
        execute!(self.out, cursor::Hide, cursor::MoveToColumn(0)).inspect_err(|_| {
            let _ = terminal::disable_raw_mode();
        })
    }

    fn leave(&mut self) -> io::Result<()> {
        // Erase the live region, leaving only the committed scrollback, then
        // restore the cursor so the shell prompt resumes right after it.
        execute!(
            self.out,
            terminal::Clear(terminal::ClearType::FromCursorDown),
            cursor::Show,
        )?;
        terminal::disable_raw_mode()
    }

    fn commit(&mut self, rows: &[Vec<Cell>]) -> io::Result<()> {
        queue!(
            self.out,
            terminal::Clear(terminal::ClearType::FromCursorDown)
        )?;
        for row in rows {
            write_row(&mut self.out, row)?;
            // Permanent newline: at the bottom of the screen this scrolls the
            // committed line up into the terminal's scrollback.
            queue!(self.out, crossterm::style::Print("\r\n"))?;
        }
        self.out.flush()
        // Cursor now sits at the new top-left of the live region.
    }

    fn present(&mut self, rows: &[Vec<Cell>]) -> io::Result<()> {
        // Erase the previous live region, then draw the new one.
        queue!(
            self.out,
            terminal::Clear(terminal::ClearType::FromCursorDown)
        )?;
        for (i, row) in rows.iter().enumerate() {
            write_row(&mut self.out, row)?;
            if i + 1 < rows.len() {
                queue!(self.out, crossterm::style::Print("\r\n"))?;
            }
        }
        // Return the cursor to the top-left of the live region.
        if rows.len() > 1 {
            queue!(self.out, cursor::MoveUp(rows.len() as u16 - 1))?;
        }
        queue!(self.out, cursor::MoveToColumn(0))?;
        self.out.flush()
    }
}

/// Extract a buffer's rows as owned cell vectors (for the inline runtime).
pub(crate) fn buffer_rows(buf: &crate::buffer::Buffer) -> Vec<Vec<Cell>> {
    (0..buf.height())
        .map(|y| (0..buf.width()).map(|x| *buf.get(x, y)).collect())
        .collect()
}

#[cfg(test)]
pub(crate) struct RecordingSink {
    pub committed: Vec<String>,
    pub live: Vec<String>,
    pub size: (u16, u16),
    pub lifecycle: Vec<&'static str>,
}

#[cfg(test)]
impl RecordingSink {
    pub fn new(w: u16, h: u16) -> Self {
        RecordingSink {
            committed: Vec::new(),
            live: Vec::new(),
            size: (w, h),
            lifecycle: Vec::new(),
        }
    }

    fn rows_to_text(rows: &[Vec<Cell>]) -> Vec<String> {
        rows.iter()
            .map(|r| {
                r.iter()
                    .map(|c| c.ch)
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }
}

#[cfg(test)]
impl InlineSink for RecordingSink {
    fn size(&self) -> io::Result<(u16, u16)> {
        Ok(self.size)
    }
    fn enter(&mut self) -> io::Result<()> {
        self.lifecycle.push("enter");
        Ok(())
    }
    fn leave(&mut self) -> io::Result<()> {
        self.lifecycle.push("leave");
        Ok(())
    }
    fn commit(&mut self, rows: &[Vec<Cell>]) -> io::Result<()> {
        self.committed.extend(Self::rows_to_text(rows));
        Ok(())
    }
    fn present(&mut self, rows: &[Vec<Cell>]) -> io::Result<()> {
        self.live = Self::rows_to_text(rows);
        Ok(())
    }
}
