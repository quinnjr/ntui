use std::io::{self, BufWriter, Stdout, Write};

use crossterm::{cursor, execute, queue, style, terminal};

use crate::backend::Backend;
use crate::backend::ansi::to_ct;
use crate::buffer::CellUpdate;

/// Alternate-screen, raw-mode terminal backend.
/// v1 emits one MoveTo+style+Print per changed cell; batching styled runs is a
/// later optimization.
pub struct FullscreenBackend {
    out: BufWriter<Stdout>,
}

impl FullscreenBackend {
    pub fn new() -> Self {
        FullscreenBackend {
            out: BufWriter::new(io::stdout()),
        }
    }
}

impl Default for FullscreenBackend {
    fn default() -> Self {
        Self::new()
    }
}

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
            // Raw mode is already on; undo it so a failed enter never leaks a broken shell.
            let _ = terminal::disable_raw_mode();
        })
    }

    fn leave(&mut self) -> io::Result<()> {
        execute!(self.out, cursor::Show, terminal::LeaveAlternateScreen)?;
        terminal::disable_raw_mode()
    }

    fn flush(&mut self, updates: &[CellUpdate]) -> io::Result<()> {
        for u in updates {
            let mut attrs = style::Attributes::default();
            if u.cell.attrs.bold {
                attrs.set(style::Attribute::Bold);
            }
            if u.cell.attrs.dim {
                attrs.set(style::Attribute::Dim);
            }
            if u.cell.attrs.italic {
                attrs.set(style::Attribute::Italic);
            }
            if u.cell.attrs.underline {
                attrs.set(style::Attribute::Underlined);
            }
            queue!(
                self.out,
                cursor::MoveTo(u.x, u.y),
                style::SetAttribute(style::Attribute::Reset),
                style::SetAttributes(attrs),
                style::SetForegroundColor(to_ct(u.cell.fg)),
                style::SetBackgroundColor(to_ct(u.cell.bg)),
                style::Print(u.cell.ch),
            )?;
        }
        self.out.flush()
    }
}
