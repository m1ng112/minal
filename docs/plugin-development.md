# Minal Plugin Development Guide

This guide explains how to build WASI-based plugins for the Minal terminal
emulator. Plugins run in a sandboxed WebAssembly environment and can extend
Minal with event hooks and custom AI providers.

## Overview

Minal plugins are compiled as WebAssembly modules targeting `wasm32-wasip1`.
Each plugin lives in its own directory alongside a `plugin.toml` manifest that
declares metadata, event subscriptions, and optional AI provider capabilities.

```
my-plugin/
├── plugin.toml          # Plugin manifest (required)
├── plugin.wasm          # Compiled WASM module (or custom path)
└── data/                # Optional data files (read-only access)
```

## Quick Start

### 1. Create a Rust library

```bash
cargo new --lib my-plugin
cd my-plugin
```

Set the crate type in `Cargo.toml`:

```toml
[lib]
crate-type = ["cdylib"]
```

### 2. Write the plugin manifest

Create `plugin.toml`:

```toml
[plugin]
name = "my-plugin"
version = "0.1.0"
description = "My first Minal plugin"
author = "Your Name"
wasm_path = "target/wasm32-wasip1/release/my_plugin.wasm"

[hooks]
on_command = true
on_output = false
on_error = true
```

### 3. Implement the ABI

```rust
use std::alloc::{Layout, alloc};

#[no_mangle]
pub extern "C" fn minal_alloc(size: i32) -> i32 {
    if size <= 0 { return 0; }
    unsafe {
        let layout = Layout::from_size_align_unchecked(size as usize, 1);
        alloc(layout) as i32
    }
}

#[no_mangle]
pub extern "C" fn minal_init() {
    // One-time initialization
}

#[no_mangle]
pub extern "C" fn minal_on_command(ptr: i32, len: i32) -> i64 {
    let input = read_input(ptr, len);
    let response = r#"{"suppress":false,"message":"Hello from my-plugin!"}"#;
    pack_string(response)
}
```

### 4. Build and install

```bash
# Add the WASI target (one-time)
rustup target add wasm32-wasip1

# Build
cargo build --target wasm32-wasip1 --release

# Install (copy the directory to the plugin path)
mkdir -p ~/.config/minal/plugins/my-plugin
cp plugin.toml ~/.config/minal/plugins/my-plugin/
cp target/wasm32-wasip1/release/my_plugin.wasm \
   ~/.config/minal/plugins/my-plugin/plugin.wasm
```

### 5. Enable plugins in Minal config

Add to `~/.config/minal/minal.toml`:

```toml
[plugins]
enabled = true
plugin_dirs = ["~/.config/minal/plugins"]
```

## Plugin ABI Reference

### Required Export

| Function | Signature | Description |
|----------|-----------|-------------|
| `minal_alloc` | `(size: i32) -> i32` | Allocate `size` bytes in plugin memory, return pointer |

### Optional Exports

| Function | Signature | Description |
|----------|-----------|-------------|
| `minal_init` | `() -> ()` | Called once after loading |
| `minal_info` | `() -> i64` | Return plugin metadata as packed `(ptr, len)` |
| `minal_on_command` | `(ptr: i32, len: i32) -> i64` | Command hook |
| `minal_on_output` | `(ptr: i32, len: i32) -> i64` | Output hook |
| `minal_on_error` | `(ptr: i32, len: i32) -> i64` | Error hook |
| `minal_ai_complete` | `(ptr: i32, len: i32) -> i64` | AI completion |
| `minal_ai_analyze_error` | `(ptr: i32, len: i32) -> i64` | AI error analysis |

### Return Value Encoding

Functions that return string data use a packed i64:
- **High 32 bits**: pointer to the string data in WASM memory
- **Low 32 bits**: length of the string in bytes
- A return value of `0` means "no data" (the hook is a no-op)

```rust
fn pack_string(s: &str) -> i64 {
    let bytes = s.as_bytes();
    let ptr = minal_alloc(bytes.len() as i32);
    if ptr == 0 { return 0; }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
    }
    ((ptr as i64) << 32) | (bytes.len() as i64)
}
```

## Event Types

### PluginEvent::Command

Sent when a shell command is entered (detected via OSC 133).

```json
{
  "type": "command",
  "command": "cargo build",
  "working_dir": "/home/user/project"
}
```

### PluginEvent::Output

Sent when terminal output is received from the PTY.

```json
{
  "type": "output",
  "data": "Compiling minal v0.1.0\n"
}
```

### PluginEvent::Error

Sent when a command exits with a non-zero status.

```json
{
  "type": "error",
  "command": "cargo test",
  "exit_code": 1,
  "stderr": "error[E0308]: mismatched types..."
}
```

## Hook Response

All hook functions return a JSON `HookResponse`:

```json
{
  "suppress": false,
  "modified_command": null,
  "message": "Optional message to display"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `suppress` | `bool` | If `true`, the event is suppressed |
| `modified_command` | `string?` | Modified command text (command hooks only) |
| `message` | `string?` | Message to display to the user |

## AI Provider Plugins

Plugins can provide custom AI backends by exporting `minal_ai_complete` and
optionally `minal_ai_analyze_error`.

### Manifest

```toml
[plugin]
name = "my-ai"
version = "0.1.0"

[ai_provider]
name = "my-ai"
```

### Configuration

In `minal.toml`:

```toml
[ai]
provider = "plugin"
plugin_provider = "my-ai"
enabled = true

[plugins]
enabled = true
```

### AI Complete

Receives a JSON-encoded `AiContext` and returns a completion string:

```rust
#[no_mangle]
pub extern "C" fn minal_ai_complete(ptr: i32, len: i32) -> i64 {
    let context_json = read_input(ptr, len);
    // Parse context, generate completion...
    let completion = "ls -la";
    pack_string(completion)
}
```

### AI Error Analysis

Receives a JSON-encoded `ErrorContext` and returns a JSON `ErrorAnalysis`:

```rust
#[no_mangle]
pub extern "C" fn minal_ai_analyze_error(ptr: i32, len: i32) -> i64 {
    let error_json = read_input(ptr, len);
    let analysis = r#"{
        "summary": "Command not found",
        "suggestion": "Install the package with: brew install foo",
        "severity": "low",
        "category": "user_error"
    }"#;
    pack_string(analysis)
}
```

## Security Model

Plugins run in a WASI sandbox with the following constraints:

- **Filesystem**: Read-only access to the plugin's own directory only
- **Network**: No network access
- **System calls**: Limited to WASI preview 1 capabilities
- **Memory**: Isolated WASM linear memory
- **Stdio**: Inherited from the host (stdout/stderr visible in terminal)

Plugins cannot:
- Access files outside their directory
- Make network requests
- Execute arbitrary system commands
- Access other plugins' memory

## Plugin Manager API

For programmatic use (e.g., in tests):

```rust
use minal_plugin::PluginManager;
use std::path::Path;

// Create a manager (optionally with an allowlist)
let mut mgr = PluginManager::new(vec![])?;

// Scan a directory for plugins
let loaded = mgr.scan_directory(Path::new("/path/to/plugins"))?;

// Dispatch an event
use minal_plugin::PluginEvent;
let event = PluginEvent::Command {
    command: "ls -la".to_string(),
    working_dir: "/home/user".to_string(),
};
let responses = mgr.dispatch_event(&event)?;

// Extract an AI provider
let provider = mgr.take_ai_provider("my-ai")?;
```

## Examples

See the `examples/plugins/` directory for complete working examples:

- **hello-plugin**: Minimal event hook plugin (on_command, on_error)
- **echo-ai-provider**: Example AI provider plugin

Build an example:

```bash
cd examples/plugins/hello-plugin
cargo build --target wasm32-wasip1 --release
```

## Troubleshooting

### Plugin not loading

- Check that `plugin.toml` exists in the plugin directory
- Verify `wasm_path` in the manifest points to a valid `.wasm` file
- Check Minal logs for detailed error messages
- Ensure `[plugins] enabled = true` in your config

### Hook not firing

- Verify the hook is enabled in `plugin.toml` (e.g., `on_command = true`)
- For output hooks, ensure the plugin subscribes to `on_output = true`
- Check that shell integration (OSC 133) is set up for command/error hooks

### AI provider not working

- Ensure both `[plugins] enabled = true` and `[ai] provider = "plugin"` are set
- Set `plugin_provider = "my-plugin-name"` in the `[ai]` section
- Verify the plugin exports `minal_ai_complete`
- Note: streaming chat (`chat_stream`) is not supported for WASM providers
