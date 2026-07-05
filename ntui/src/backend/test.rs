use std::io;

use crate::backend::Backend;
use crate::buffer::{Buffer, CellUpdate};

/// In-memory backend: applies flushed updates to a Buffer for assertions.
pub struct TestBackend {
    pub buffer: Buffer,
    pub lifecycle: Vec<&'static str>,
}

impl TestBackend {
    pub fn new(width: u16, height: u16) -> Self {
        TestBackend {
            buffer: Buffer::new(width, height),
            lifecycle: Vec::new(),
        }
    }

    pub fn to_text(&self) -> String {
        self.buffer.to_text()
    }
}

impl Backend for TestBackend {
    fn size(&self) -> io::Result<(u16, u16)> {
        Ok((self.buffer.width(), self.buffer.height()))
    }
    fn enter(&mut self) -> io::Result<()> {
        self.lifecycle.push("enter");
        Ok(())
    }
    fn leave(&mut self) -> io::Result<()> {
        self.lifecycle.push("leave");
        Ok(())
    }
    fn flush(&mut self, updates: &[CellUpdate]) -> io::Result<()> {
        for u in updates {
            self.buffer.set(u.x, u.y, u.cell.clone());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Backend;
    use crate::buffer::{Cell, CellUpdate};

    #[test]
    fn flush_applies_updates_and_records_lifecycle() {
        let mut be = TestBackend::new(3, 1);
        be.enter().unwrap();
        be.flush(&[
            CellUpdate {
                x: 0,
                y: 0,
                cell: Cell {
                    ch: 'o',
                    ..Cell::default()
                },
            },
            CellUpdate {
                x: 1,
                y: 0,
                cell: Cell {
                    ch: 'k',
                    ..Cell::default()
                },
            },
        ])
        .unwrap();
        be.leave().unwrap();
        assert_eq!(be.to_text(), "ok ");
        assert_eq!(be.lifecycle, vec!["enter", "leave"]);
    }

    #[test]
    fn size_reports_dimensions() {
        let be = TestBackend::new(7, 3);
        assert_eq!(be.size().unwrap(), (7, 3));
    }
}
