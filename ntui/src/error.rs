/// Errors that can occur while rendering or driving an ntui app.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("terminal I/O error: {0}")]
    Io(#[from] std::io::Error),
}
