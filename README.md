# Hagibis Mapping

Cross-platform application that remaps Hagibis UC-1102AG USB hub buttons
to arbitrary keyboard shortcuts, media keys, and mouse gestures.

## Features

- **Seize & remap** the hub's physical controls (2 buttons, rotary knob, Play/Pause)
- **macOS GUI** — menu-bar app with live monitor overlay on hub photo
- **CLI + TUI** — terminal interface for headless / remote use
- **Per-app profiles** — different mappings for different foreground applications
- **Live config editor** — click any pill in the monitor to remap a control
- **Auto plug/unplug** — engine detects device arrival and removal
- **TOML-driven** — human-readable config at `~/Library/Application Support/override-hub/config.toml`

## Quick start (macOS)

```bash
./configure
make dist          # builds Rust crate + .app bundle + DMG
open gui/HagibisMapping.app
```

Grant permissions on first launch; the app elevates itself via AEWP
(admin password).  The DMG at `gui/HagibisMapping-0.1.0.dmg` can be
distributed.

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
or use the GUI binding editor (click any pill in the Show Monitor overlay).

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

## Building from source

Dependencies: Rust toolchain, Xcode Command Line Tools (macOS).

```bash
./configure
make          # Rust crate only
make gui      # macOS GUI (requires swiftc / clang)
make dist     # .app bundle + DMG
```

## License

MIT — see [LICENSE](LICENSE).
