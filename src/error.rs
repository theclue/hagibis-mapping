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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_seize() {
        let e = Error::Seize("device busy".into());
        assert!(e.to_string().contains("seize error"));
        assert!(e.to_string().contains("device busy"));
    }

    #[test]
    fn error_display_config() {
        let e = Error::Config("bad toml".into());
        assert_eq!(e.to_string(), "config error: bad toml");
    }

    #[test]
    fn error_display_io() {
        let e = Error::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "missing"));
        assert!(e.to_string().contains("io error"));
    }

    #[test]
    fn error_from_io() {
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope");
        let e: Error = io.into();
        assert!(matches!(e, Error::Io(_)));
    }
}
