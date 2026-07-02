#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("terminal I/O error: {0}")]
    Io(#[from] std::io::Error),
}
