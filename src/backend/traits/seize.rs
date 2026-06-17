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
}
