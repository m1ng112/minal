//! `minal-plugin` — WASI-based plugin system.
//!
//! Provides a sandboxed plugin runtime for extending Minal with:
//! - **Event hooks**: `on_command`, `on_output`, `on_error`
//! - **Custom AI providers**: plugins that implement the `AiProvider` trait
//! - **Plugin manager**: discovery, loading, lifecycle management
//!
//! Plugins are WASM modules (compiled to `wasm32-wasip1`) accompanied by a
//! `plugin.toml` manifest. They run in a WASI sandbox with limited filesystem
//! access and communicate with the host via JSON over a simple ABI.
//!
//! # Plugin ABI
//!
//! Plugins must export the following memory management function:
//!
//! - `minal_alloc(size: i32) -> i32` — allocate `size` bytes, return pointer
//!
//! And optionally export any of:
//!
//! - `minal_init()` — called once after loading
//! - `minal_info() -> i64` — return plugin metadata as packed (ptr, len)
//! - `minal_on_command(ptr: i32, len: i32) -> i64` — command hook
//! - `minal_on_output(ptr: i32, len: i32) -> i64` — output hook
//! - `minal_on_error(ptr: i32, len: i32) -> i64` — error hook
//! - `minal_ai_complete(ptr: i32, len: i32) -> i64` — AI completion
//! - `minal_ai_analyze_error(ptr: i32, len: i32) -> i64` — AI error analysis
//!
//! Return values are packed i64: `(offset << 32) | length`.

pub mod ai_bridge;
mod error;
pub mod event;
pub mod manager;
pub mod manifest;
pub mod runtime;

pub use ai_bridge::WasmAiProvider;
pub use error::PluginError;
pub use event::{HookResponse, PluginEvent};
pub use manager::PluginManager;
pub use manifest::{AiProviderConfig, HookConfig, PluginManifest, PluginMeta};
pub use runtime::WasiRuntime;
