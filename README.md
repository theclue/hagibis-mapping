# Hagibis Mapping

Cross-platform application that remaps Hagibis UC-1102AG USB hub buttons
to arbitrary keyboard shortcuts, media keys, and mouse gestures.

## Features

- **Seize & remap** the hub's physical controls (2 buttons, rotary knob, Play/Pause)
- **macOS GUI** — menu-bar app with live monitor overlay and editor (Windows GUI incoming)
- **CLI + TUI** — terminal interface for remote monitoring (and because it's nice!)
- **Per-app profiles** — different mappings for different foreground applications

## Quick build (macOS)

(pre-compiled packages for both macOS and Windows will follow)

```bash
./configure
make dist          # builds Rust crate + .app bundle + DMG
open gui/HagibisMapping.app
```

or 

```bash
./configure
make install        # builds Rust crate + .app bundle + moves into /Applications
```

First launch on macOS is a bit tedious: you must enter an Administrator password,
then grant permissions twice. You are going to seize an USB device from the OS, so there
is no other way. It sounds invasive, but it's not. It's just annoying. Sorry!

## CLI / TUI

```bash
make build
./target/debug/hagibis_hub_mapper   # TUI mode
```

Press Ctrl+C to stop.  Run with logging:

```bash
RUST_LOG=debug ./target/debug/hagibis_hub_mapper
```

## Configuration

The config file is generated automatically on first run.  Edit it directly,
or use the GUI editor (click any pill in the Show Monitor window).

```toml
[default.button_top_left]
type = "Keyboard"
binding = "Shift+Cmd+N"
label = "New Window"

[default.knob_cw]
type = "MediaKey"
key_type = 0   # Vol+

[[profiles]]
app_id = "com.apple.Terminal"
app_name = "Terminal"

[profiles.mappings]
button_top_left = { type = "Keyboard", binding = "Cmd+N" }
```
It's safe to edit the configuration file directly if you feel comfortable with Toml syntax.
Just follow the instructions on the file.

## Building from source

Dependencies: Rust toolchain, Xcode Command Line Tools (macOS), GNU autotools.

```bash
./configure
make          # Rust crate only
make gui      # macOS GUI (requires swiftc / clang)
make dist     # .app bundle + DMG
```

All available targets

```bash
make help
```

## License

MIT — see [LICENSE](LICENSE).
