use crate::error::Error;
use crate::hid::Report;

/// Abstraction over platform-specific HID device seizing and reading.
pub trait HIDBackend {
    /// Seize the hub interfaces so the OS no longer receives original reports.
    fn seize(&mut self) -> Result<(), Error>;

    /// Poll for HID reports.
    ///
    /// Runs the platform event loop for up to `timeout_ms` milliseconds.
    /// Returns the next report if one arrived, `None` on timeout.
    fn run_once(&mut self, timeout_ms: u32) -> Result<Option<Report>, Error>;

    /// Release all held resources.
    fn release(&mut self);

    /// Returns `false` when the seized device has been physically removed.
    /// Default implementation always returns `true` (no detection).
    /// Platform backends that support device-notification APIs should
    /// override this to return live state from an arrival/removal message.
    fn is_device_present(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubBackend;
    impl HIDBackend for StubBackend {
        fn seize(&mut self) -> Result<(), Error> {
            Ok(())
        }
        fn run_once(&mut self, _timeout_ms: u32) -> Result<Option<Report>, Error> {
            Ok(None)
        }
        fn release(&mut self) {}
    }

    #[test]
    fn is_device_present_defaults_to_true() {
        let backend = StubBackend;
        assert!(backend.is_device_present());
    }
}
