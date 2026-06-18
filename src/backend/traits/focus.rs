/// Information about the currently focused (foreground) application.
#[derive(Debug, Clone)]
pub struct FocusedApp {
    /// macOS: bundle identifier (`com.apple.Safari`)
    /// Windows: executable basename (`chrome.exe`)
    pub id: String,
    /// Human-readable display name.
    pub name: String,
}

/// Query the foreground application.
///
/// Implementations: `backend::macos::focus::NSWorkspaceFocus`,
///                   `backend::windows::focus::Win32Focus`.
pub trait FocusQuery {
    /// Return the application currently in focus, or `None` if unavailable.
    fn focused_app(&self) -> Option<FocusedApp>;
}
