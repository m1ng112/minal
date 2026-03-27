//! Hello Plugin — minimal Minal plugin example.
//!
//! Demonstrates the plugin ABI by subscribing to `on_command` and `on_error`
//! hooks. Returns a greeting message for every command and a diagnostic
//! message for every error.

use std::alloc::{Layout, alloc};

/// Allocator export required by the Minal plugin ABI.
///
/// The host calls this to reserve memory in the plugin before writing
/// JSON event data.
#[no_mangle]
pub extern "C" fn minal_alloc(size: i32) -> i32 {
    if size <= 0 {
        return 0;
    }
    // SAFETY: size > 0 guarantees a valid layout.
    unsafe {
        let layout = Layout::from_size_align_unchecked(size as usize, 1);
        alloc(layout) as i32
    }
}

/// Called once after the plugin is loaded.
#[no_mangle]
pub extern "C" fn minal_init() {
    // Nothing to initialize for this example.
}

/// Return plugin metadata as a packed `(ptr << 32) | len` i64.
#[no_mangle]
pub extern "C" fn minal_info() -> i64 {
    let json = r#"{"name":"hello-plugin","version":"0.1.0"}"#;
    pack_string(json)
}

/// Command hook: receives a JSON `PluginEvent::Command`, returns a `HookResponse`.
#[no_mangle]
pub extern "C" fn minal_on_command(ptr: i32, len: i32) -> i64 {
    let input = read_input(ptr, len);
    let response = format!(
        r#"{{"suppress":false,"message":"[hello-plugin] saw command: {}"}}"#,
        escape_json_string(&input)
    );
    pack_string(&response)
}

/// Error hook: receives a JSON `PluginEvent::Error`, returns a `HookResponse`.
#[no_mangle]
pub extern "C" fn minal_on_error(ptr: i32, len: i32) -> i64 {
    let input = read_input(ptr, len);
    let response = format!(
        r#"{{"suppress":false,"message":"[hello-plugin] saw error: {}"}}"#,
        escape_json_string(&input)
    );
    pack_string(&response)
}

// ── Helpers ─────────────────────────────────────────────────────────

fn read_input(ptr: i32, len: i32) -> String {
    if ptr <= 0 || len <= 0 {
        return String::new();
    }
    // SAFETY: The host writes valid UTF-8 JSON into the allocated region.
    unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        String::from_utf8_lossy(slice).into_owned()
    }
}

fn pack_string(s: &str) -> i64 {
    let bytes = s.as_bytes();
    let ptr = minal_alloc(bytes.len() as i32);
    if ptr == 0 {
        return 0;
    }
    // SAFETY: minal_alloc returned a valid pointer for `bytes.len()` bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
    }
    ((ptr as i64) << 32) | (bytes.len() as i64)
}

fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
