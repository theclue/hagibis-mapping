pub mod backend;
pub mod config;
pub mod engine;
pub mod error;
pub mod ffi;
pub mod hid;
pub mod logging;
pub mod tui;

mod cli;

/// Run the CLI / TUI event loop.  Blocks until Ctrl+C.
pub fn run() -> Result<(), error::Error> {
    cli::run()
}
