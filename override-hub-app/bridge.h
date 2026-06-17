// C FFI bridge — functions exported by the Rust library (override-hub-rs).
// Used by native GUI frontends (Swift/AppKit, C#/WinForms, etc.).

#ifndef HAGIBIS_H
#define HAGIBIS_H

#ifdef __cplusplus
extern "C" {
#endif

// Start the engine (seize + background poll loop). Returns 0 on success, -1 on error.
int hagibis_start(void);

// Stop the engine and release all devices.
void hagibis_stop(void);

// Returns 1 if the engine is running, 0 otherwise.
int hagibis_is_running(void);

// Write the current engine status as a JSON string into buf (max buf_size bytes).
// Returns number of bytes written (excluding null terminator), or 0 on error.
// JSON fields: running, seized, consumer, keyboard_keys, keyboard_modifiers,
//               focused_app_id, focused_app_name, error
int hagibis_status_json(char *buf, int buf_size);

// Load the current config as a JSON string into buf.
int hagibis_config_json(char *buf, int buf_size);

// Save config from a JSON string. Returns 0 on success, -1 on error.
int hagibis_save_config_json(const char *json);

#ifdef __cplusplus
}
#endif

#endif
