//! WASI plugin runtime powered by wasmtime.
//!
//! Wraps the wasmtime engine and provides a sandboxed execution environment
//! for each plugin. Plugins get WASI capabilities (stdio, filesystem access
//! scoped to their own directory) and communicate with the host via
//! JSON-serialized messages passed through exported functions.

use std::path::Path;
use std::sync::Arc;

use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::preview1::{self, WasiP1Ctx};
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};

use crate::error::PluginError;
use crate::event::{HookResponse, PluginEvent};

/// Shared wasmtime engine (thread-safe, cheap to clone).
#[derive(Clone)]
pub struct WasiRuntime {
    engine: Arc<Engine>,
}

impl std::fmt::Debug for WasiRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasiRuntime").finish_non_exhaustive()
    }
}

/// Per-plugin WASM instance with its own store and WASI context.
pub struct PluginInstance {
    store: Store<WasiP1Ctx>,
    instance: wasmtime::Instance,
}

impl WasiRuntime {
    /// Create a new WASI runtime with default engine configuration.
    ///
    /// # Errors
    /// Returns `PluginError::Runtime` if the wasmtime engine cannot be created.
    pub fn new() -> Result<Self, PluginError> {
        let engine = Engine::default();
        Ok(Self {
            engine: Arc::new(engine),
        })
    }

    /// Load and instantiate a WASM plugin module.
    ///
    /// The plugin gets WASI access with:
    /// - Inherited stdio (stdout/stderr visible in terminal)
    /// - Read-only access to its own plugin directory
    ///
    /// # Errors
    /// Returns `PluginError::Runtime` on compilation or instantiation failure.
    pub fn load_plugin(
        &self,
        wasm_path: &Path,
        plugin_dir: &Path,
    ) -> Result<PluginInstance, PluginError> {
        let module = Module::from_file(&self.engine, wasm_path).map_err(|e| {
            PluginError::Runtime(format!(
                "failed to compile WASM module {}: {e}",
                wasm_path.display()
            ))
        })?;

        let mut wasi_builder = WasiCtxBuilder::new();
        wasi_builder.inherit_stdio();

        // Grant read-only access to the plugin's own directory.
        if plugin_dir.is_dir() {
            wasi_builder
                .preopened_dir(plugin_dir, "/plugin", DirPerms::READ, FilePerms::READ)
                .map_err(|e| {
                    PluginError::Runtime(format!(
                        "failed to preopen plugin dir {}: {e}",
                        plugin_dir.display()
                    ))
                })?;
        }

        let wasi_ctx = wasi_builder.build_p1();
        let mut store = Store::new(&self.engine, wasi_ctx);

        let mut linker = Linker::new(&self.engine);
        preview1::add_to_linker_sync(&mut linker, |ctx: &mut WasiP1Ctx| ctx)
            .map_err(|e| PluginError::Runtime(format!("failed to link WASI: {e}")))?;

        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| PluginError::Runtime(format!("failed to instantiate module: {e}")))?;

        Ok(PluginInstance { store, instance })
    }
}

impl PluginInstance {
    /// Call the plugin's `minal_init` function if it exists.
    ///
    /// This is called once after loading to let the plugin perform setup.
    ///
    /// # Errors
    /// Returns `PluginError::Call` if the function exists but fails.
    pub fn call_init(&mut self) -> Result<(), PluginError> {
        let func = self.instance.get_func(&mut self.store, "minal_init");
        if let Some(f) = func {
            f.call(&mut self.store, &[], &mut [])
                .map_err(|e| PluginError::Call(format!("minal_init failed: {e}")))?;
        }
        Ok(())
    }

    /// Call the plugin's `minal_info` function to get plugin metadata as JSON.
    ///
    /// The plugin should export `minal_info` which writes JSON to a shared
    /// memory buffer and returns the (offset, length) pair.
    ///
    /// # Errors
    /// Returns `PluginError::Call` if the function fails.
    pub fn call_info(&mut self) -> Result<Option<String>, PluginError> {
        self.call_string_export("minal_info")
    }

    /// Dispatch an event to the plugin and collect the response.
    ///
    /// Serializes the event as JSON, writes it to the plugin's memory,
    /// and calls the appropriate hook function. The plugin may return
    /// a JSON [`HookResponse`].
    ///
    /// # Errors
    /// Returns `PluginError::Call` on invocation failure, or
    /// `PluginError::InvalidResponse` if the response cannot be parsed.
    pub fn dispatch_event(&mut self, event: &PluginEvent) -> Result<HookResponse, PluginError> {
        let func_name = match event {
            PluginEvent::Command { .. } => "minal_on_command",
            PluginEvent::Output { .. } => "minal_on_output",
            PluginEvent::Error { .. } => "minal_on_error",
        };

        let func = self.instance.get_func(&mut self.store, func_name);
        if func.is_none() {
            // Plugin doesn't export this hook; return default (no-op).
            return Ok(HookResponse::default());
        }

        let event_json = serde_json::to_string(event)?;
        let response_json = self.call_string_with_input(func_name, &event_json)?;

        match response_json {
            Some(json) => {
                let resp: HookResponse = serde_json::from_str(&json)
                    .map_err(|e| PluginError::InvalidResponse(format!("{e}: {json}")))?;
                Ok(resp)
            }
            None => Ok(HookResponse::default()),
        }
    }

    /// Call an AI completion function on the plugin.
    ///
    /// The plugin should export `minal_ai_complete(ptr, len) -> (ptr, len)`
    /// accepting a JSON-encoded context and returning a completion string.
    ///
    /// # Errors
    /// Returns `PluginError::Call` on invocation failure.
    pub fn call_ai_complete(&mut self, context_json: &str) -> Result<Option<String>, PluginError> {
        self.call_string_with_input("minal_ai_complete", context_json)
    }

    /// Call an AI error analysis function on the plugin.
    ///
    /// # Errors
    /// Returns `PluginError::Call` on invocation failure.
    pub fn call_ai_analyze_error(
        &mut self,
        error_json: &str,
    ) -> Result<Option<String>, PluginError> {
        self.call_string_with_input("minal_ai_analyze_error", error_json)
    }

    /// Check if the plugin exports a given function.
    pub fn has_export(&mut self, name: &str) -> bool {
        self.instance.get_func(&mut self.store, name).is_some()
    }

    // ── Internal helpers ──────────────────────────────────────────────

    /// Call an exported function that takes no args and returns a string
    /// via the `minal_alloc`/memory protocol.
    fn call_string_export(&mut self, name: &str) -> Result<Option<String>, PluginError> {
        let func = self.instance.get_func(&mut self.store, name);
        let Some(f) = func else {
            return Ok(None);
        };

        let mut results = [wasmtime::Val::I64(0)];
        f.call(&mut self.store, &[], &mut results)
            .map_err(|e| PluginError::Call(format!("{name} failed: {e}")))?;

        // The plugin returns a packed i64: high 32 bits = offset, low 32 bits = length.
        let packed = results[0].i64().unwrap_or(0);
        if packed == 0 {
            return Ok(None);
        }

        let offset = (packed >> 32) as u32 as usize;
        let length = (packed & 0xFFFF_FFFF) as u32 as usize;

        self.read_string_from_memory(offset, length).map(Some)
    }

    /// Call an exported function that takes a JSON string input and returns
    /// a JSON string output via the memory protocol.
    fn call_string_with_input(
        &mut self,
        name: &str,
        input: &str,
    ) -> Result<Option<String>, PluginError> {
        let func = self.instance.get_func(&mut self.store, name);
        let Some(f) = func else {
            return Ok(None);
        };

        // Write input string to plugin memory.
        let (ptr, len) = self.write_string_to_memory(input)?;

        let args = [
            wasmtime::Val::I32(ptr as i32),
            wasmtime::Val::I32(len as i32),
        ];
        let mut results = [wasmtime::Val::I64(0)];
        f.call(&mut self.store, &args, &mut results)
            .map_err(|e| PluginError::Call(format!("{name} failed: {e}")))?;

        let packed = results[0].i64().unwrap_or(0);
        if packed == 0 {
            return Ok(None);
        }

        let offset = (packed >> 32) as u32 as usize;
        let length = (packed & 0xFFFF_FFFF) as u32 as usize;

        self.read_string_from_memory(offset, length).map(Some)
    }

    /// Write a string into the plugin's WASM memory using `minal_alloc`.
    fn write_string_to_memory(&mut self, s: &str) -> Result<(usize, usize), PluginError> {
        let alloc_fn = self
            .instance
            .get_func(&mut self.store, "minal_alloc")
            .ok_or_else(|| PluginError::Call("plugin does not export minal_alloc".to_string()))?;

        let len = s.len();
        let mut results = [wasmtime::Val::I32(0)];
        alloc_fn
            .call(
                &mut self.store,
                &[wasmtime::Val::I32(len as i32)],
                &mut results,
            )
            .map_err(|e| PluginError::Call(format!("minal_alloc failed: {e}")))?;

        let ptr = results[0].i32().unwrap_or(0) as usize;

        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .ok_or_else(|| PluginError::Call("plugin has no exported memory".to_string()))?;

        let data = memory.data_mut(&mut self.store);
        if ptr.checked_add(len).is_none_or(|end| end > data.len()) {
            return Err(PluginError::Call(format!(
                "minal_alloc returned out-of-bounds pointer: ptr={ptr}, len={len}, mem={}",
                data.len()
            )));
        }
        data[ptr..ptr + len].copy_from_slice(s.as_bytes());
        Ok((ptr, len))
    }

    /// Read a UTF-8 string from the plugin's WASM memory.
    fn read_string_from_memory(
        &mut self,
        offset: usize,
        length: usize,
    ) -> Result<String, PluginError> {
        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .ok_or_else(|| PluginError::Call("plugin has no exported memory".to_string()))?;

        let data = memory.data(&self.store);
        if offset
            .checked_add(length)
            .is_none_or(|end| end > data.len())
        {
            return Err(PluginError::InvalidResponse(format!(
                "out-of-bounds read: offset={offset}, length={length}, mem={}",
                data.len()
            )));
        }

        String::from_utf8(data[offset..offset + length].to_vec())
            .map_err(|e| PluginError::InvalidResponse(format!("invalid UTF-8: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_creation_succeeds() {
        let rt = WasiRuntime::new();
        assert!(rt.is_ok());
    }

    #[test]
    fn load_nonexistent_wasm_fails() {
        let rt = WasiRuntime::new().expect("runtime");
        let result = rt.load_plugin(
            Path::new("/nonexistent/plugin.wasm"),
            Path::new("/nonexistent"),
        );
        assert!(result.is_err());
    }
}
