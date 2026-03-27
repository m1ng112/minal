//! Echo AI Provider Plugin — example custom AI provider for Minal.
//!
//! Demonstrates the AI provider plugin ABI by implementing `minal_ai_complete`
//! and `minal_ai_analyze_error`. The completion function echoes back the last
//! command from the context, and the error analysis returns a fixed suggestion.

use std::alloc::{Layout, alloc};

/// Allocator export required by the Minal plugin ABI.
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
pub extern "C" fn minal_init() {}

/// Return plugin metadata.
#[no_mangle]
pub extern "C" fn minal_info() -> i64 {
    let json = r#"{"name":"echo-ai","version":"0.1.0","type":"ai_provider"}"#;
    pack_string(json)
}

/// AI completion: receives JSON context, returns a completion string.
///
/// This example simply echoes back "echo: <input>" as the completion.
#[no_mangle]
pub extern "C" fn minal_ai_complete(ptr: i32, len: i32) -> i64 {
    let input = read_input(ptr, len);
    let response = format!("echo: {input}");
    pack_string(&response)
}

/// AI error analysis: receives JSON error context, returns a JSON ErrorAnalysis.
#[no_mangle]
pub extern "C" fn minal_ai_analyze_error(ptr: i32, len: i32) -> i64 {
    let _input = read_input(ptr, len);
    let analysis = r#"{"summary":"Error detected by echo-ai plugin","suggestion":"Check the command syntax and try again.","severity":"low","category":"user_error"}"#;
    pack_string(analysis)
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
