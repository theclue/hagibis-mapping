/// Unified error type for the application.
#[derive(Debug)]
pub enum Error {
    /// IOKit / Win32 HID seizing failed.
    Seize(String),
    /// CGEvent / SendInput injection failed.
    Inject(String),
    /// Config file error (parse, read, write).
    Config(String),
    /// I/O error (file system).
    Io(std::io::Error),
    /// Generic runtime error.
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Seize(msg)  => write!(f, "seize error: {}", msg),
            Error::Inject(msg) => write!(f, "inject error: {}", msg),
            Error::Config(msg) => write!(f, "config error: {}", msg),
            Error::Io(e)       => write!(f, "io error: {}", e),
            Error::Other(msg)  => write!(f, "{}", msg),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self { Error::Io(e) }
}
