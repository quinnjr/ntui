use std::io;

use crate::buffer::CellUpdate;

pub(crate) mod ansi;
pub mod fullscreen;
pub mod inline;
pub mod test;
pub use fullscreen::FullscreenBackend;
pub use inline::InlineBackend;
pub use test::TestBackend;

/// Terminal output target. `flush` receives only cells that changed since the previous frame.
pub trait Backend {
    /// Current size as (columns, rows).
    fn size(&self) -> io::Result<(u16, u16)>;
    /// Prepare the target for drawing. MUST leave the screen cleared: the first frame is diffed against an all-blank buffer.
    fn enter(&mut self) -> io::Result<()>;
    /// Restore the target to its pre-`enter` state.
    fn leave(&mut self) -> io::Result<()>;
    /// Apply a batch of changed cells.
    fn flush(&mut self, updates: &[CellUpdate]) -> io::Result<()>;
    /// Set the system clipboard to `text` (OSC 52). Default is a no-op so
    /// backends without a real terminal (or apps that never copy) don't pay
    /// for the support; best-effort — the terminal emulator may ignore the
    /// sequence.
    fn copy_to_clipboard(&mut self, _text: &str) -> io::Result<()> {
        Ok(())
    }
}
