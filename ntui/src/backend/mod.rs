use std::io;

use crate::buffer::CellUpdate;

pub mod test;
pub use test::TestBackend;

pub trait Backend {
    fn size(&self) -> io::Result<(u16, u16)>;
    fn enter(&mut self) -> io::Result<()>;
    fn leave(&mut self) -> io::Result<()>;
    fn flush(&mut self, updates: &[CellUpdate]) -> io::Result<()>;
}
