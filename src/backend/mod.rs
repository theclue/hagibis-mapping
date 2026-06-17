#[cfg(target_os = "macos")]
pub mod macos;
pub mod traits;
#[cfg(target_os = "windows")]
pub mod windows;

pub use traits::{FocusQuery, HIDBackend, Injector};
