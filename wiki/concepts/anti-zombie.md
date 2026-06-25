---
title: Anti-Zombie Safety Architecture
category: concepts
created: 2026-06-25
last_updated: 2026-06-25
source_files:
  - src/backend/macos/seize.rs
  - src/ffi.rs
  - src/cli.rs
  - Cargo.toml
  - HANDOFF.md
related_concepts: []
---

# Anti-Zombie Safety Architecture

The anti-zombie pattern prevents the USB device from remaining in a seized state — unusable by the operating system — after the application crashes or panics. Without this safety net, a process abort would leave a Hagibis hub or similar HID device locked until the user physically unplugs it.

## The Problem: Zombie Device

When the [macOS backend](../modules/backend-macos.md) calls `IOHIDManagerOpen` with `kIOHIDOptionsTypeSeizeDevice`, the operating system grants exclusive access to the HID interface. The device remains seized until one of two events occurs:

- The `IOHIDManager` is explicitly closed via `IOHIDManagerClose`.
- The owning process exits entirely.

If the process is killed with `SIGKILL` (`kill -9`) or aborts due to an unhandled panic with `panic = "abort"`, the `IOHIDManager` is never released. The hub's HID interfaces become **zombie devices** — visible in the system but unusable by any process. The only recovery is a physical unplug and replug of the USB cable.

The HANDOFF.md documents this failure mode explicitly:

> "I processi zombie IOKit (seize bloccante) sopravvivono a kill -9 e richiedono lo scollegamento fisico dell'hub USB."

## Three Safety Layers

The architecture implements three independent safety layers, each addressing a different failure scenario.

### Layer 1: `IOKitManager::Drop`

The primary defense is Rust's `Drop` implementation on `IOKitManager`:

```rust
impl Drop for IOKitManager {
    fn drop(&mut self) {
        self.release();
    }
}
```

The `release()` method performs cleanup in order:

1. Sets `running` flag to `false`.
2. Unschedules the manager from the Core Foundation run loop via `IOHIDManagerUnscheduleFromRunLoop`.
3. Closes the manager via `IOHIDManagerClose`.
4. Releases the manager's retain count via `CFRelease` (balancing the `+1` retain from `IOHIDManagerCreate`).
5. Nullifies the manager pointer for idempotency.
6. Reclaims the `Arc` context pointers handed to the C callbacks, preventing memory leaks.

**Idempotency**: `release()` is idempotent. If a normal stop path calls `release()` explicitly and then `Drop` runs later (e.g., during stack unwinding), the second call is a no-op because the manager pointer is already null.

**Thread confinement**: The `release()` method calls `CFRunLoopGetCurrent()`, so it must execute on the same thread that scheduled the manager. The struct is confined to a single thread (the engine thread or the CLI main thread), and both the explicit stop path and the implicit `Drop` path run on that same thread.

This layer catches panics that occur anywhere in the engine loop body during an unwind — stack unwinding triggers `Drop` for all local variables, including `IOKitManager`.

### Layer 2: `catch_unwind` in the Engine Thread

In the [FFI module](../modules/ffi.md) (`ffi.rs`), the engine thread spawned by `hagibis_start_impl` wraps its main loop in `std::panic::catch_unwind`:

```rust
let handle = std::thread::spawn(move || {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine_loop(&stop);
    }));
    if outcome.is_err() {
        logging::error_log("ffi", "engine thread panicked; device released via Drop");
        let mut st = status();
        *st = Status::idle();
        st.error = "engine panicked".into();
    }
    finished_thread.store(true, std::sync::atomic::Ordering::Relaxed);
});
```

When a panic occurs inside `engine_loop`:

1. `catch_unwind` intercepts the panic and prevents it from aborting the thread.
2. Stack unwinding runs, which drops all local variables — crucially, the `IOKitManager` instance created inside the loop.
3. `IOKitManager::Drop` calls `release()`, which releases the device back to the OS.
4. After the unwind completes, the error branch logs the event and resets the global status to idle.
5. The `finished` atomic is set to `true`, allowing the parent to detect that the engine has terminated.

This layer specifically protects the **threaded FFI path**, where the engine runs on a background thread spawned by a native GUI (Swift, C#, etc.).

### Layer 3: `catch_unwind` at the FFI Boundary

All `#[no_mangle] extern "C"` functions in `ffi.rs` are wrapped in a guard function called `ffi_guard`:

```rust
fn ffi_guard<T>(default: T, f: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(_) => {
            logging::error_log("ffi", "panic caught at FFI boundary");
            default
        }
    }
}
```

Every exported C function delegates to an `_impl` variant through this guard:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn hagibis_start() -> i32 {
    ffi_guard(-1, hagibis_start_impl)
}

#[unsafe(no_mangle)]
pub extern "C" fn hagibis_stop() {
    ffi_guard((), hagibis_stop_impl)
}

#[unsafe(no_mangle)]
pub extern "C" fn hagibis_status_json(buf: *mut c_char, buf_size: i32) -> i32 {
    ffi_guard(0, || hagibis_status_json_impl(buf, buf_size))
}

#[unsafe(no_mangle)]
pub extern "C" fn hagibis_config_json(buf: *mut c_char, buf_size: i32) -> i32 {
    ffi_guard(0, || hagibis_config_json_impl(buf, buf_size))
}

#[unsafe(no_mangle)]
pub extern "C" fn hagibis_reload_config() -> i32 {
    ffi_guard(-1, hagibis_reload_config_impl)
}

#[unsafe(no_mangle)]
pub extern "C" fn hagibis_save_config_json(json_ptr: *const c_char) -> i32 {
    ffi_guard(-1, || hagibis_save_config_json_impl(json_ptr))
}

#[unsafe(no_mangle)]
pub extern "C" fn hagibis_is_running() -> i32 {
    ffi_guard(0, hagibis_is_running_impl)
}
```

This layer is critical because panicking across an FFI boundary is **undefined behavior**. A Rust panic that propagates into C (or Swift, or C#) would corrupt the foreign language's call stack. The `ffi_guard` converts any panic into a logged error and a safe default return value, ensuring the native GUI never receives an unwinding panic.

## Why `panic = "unwind"` Is Critical

Both `catch_unwind` and `Drop` during unwinding depend on Rust's default panic mechanism, which walks the stack and runs destructors. If the project used `panic = "abort"`, these guarantees would break:

- `catch_unwind` would never catch a panic — the process would abort immediately on any panic.
- `Drop` would not run for local variables on the panicking thread — the `IOKitManager` would never release the device.

The `Cargo.toml` explicitly pins both profiles to `panic = "unwind"` with a comment explaining the rationale:

```toml
[profile.dev]
panic = "unwind"

[profile.release]
panic = "unwind"
```

The comment in `Cargo.toml` reads:

> The anti-zombie safety net (IOKitManager Drop + catch_unwind in the engine thread + catch_unwind at the FFI boundary) all depend on unwinding. Pin it explicitly so a future `panic = "abort"` cannot silently reintroduce the "device stays seized until physical unplug" hazard.

## CLI Path Safety

The [CLI entry point](../modules/cli.md) in `cli.rs` runs on the main thread without `catch_unwind`. Its safety relies on:

1. **`IOKitManager::Drop`**: The `run()` function creates `seize_backend` (an `IOKitManager`) as a local variable. If the function panics, stack unwinding drops the variable and releases the device.
2. **Ctrl+C handler**: The `running()` function installs a signal handler via the `ctrlc` crate that sets a flag to break the main loop. If the handler cannot be installed, the `Drop` on `seize_backend` still releases the device on process exit.
3. **Explicit release on normal exit**: The `run()` function calls `seize_backend.release()` before returning successfully.

The comment in `cli.rs` notes the Ctrl+C handler failure case explicitly:

> Don't panic if the handler can't be installed — just warn and fall back to "stop with kill/SIGTERM". The IOKitManager Drop still releases the device on exit, so this is non-fatal.

## Testing Considerations

Testing the anti-zombie safety net requires verifying that each layer catches its target failure mode:

- **Drop during unwind**: Construct a test that triggers a panic inside the engine loop while the `IOKitManager` is active. Verify that `release()` is called by observing the IOKit state (e.g., attempting to open the device after the panic and confirming it succeeds).
- **FFI boundary safety**: Call each `extern "C"` function from a C or Swift harness and confirm that a panic inside the implementation returns the expected error value instead of aborting.
- **`catch_unwind` in engine thread**: Induce a panic inside `engine_loop` and confirm that the thread's `finished` flag is set and the status is reset to idle.
- **`panic = "unwind"` enforcement**: Run a CI check that greps `Cargo.toml` to confirm both profiles still use `panic = "unwind"`, preventing a future maintainer from inadvertently removing the setting.
- **Poison recovery**: All `Mutex` locks in the FFI module use `unwrap_or_else(|e| e.into_inner())` to recover from poisoned mutexes. The HID report callback in `seize.rs` also uses this pattern because a panic cannot corrupt the `VecDeque<Report>`.
