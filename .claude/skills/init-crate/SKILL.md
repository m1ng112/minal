---
name: init-crate
description: "Initialize a new workspace member crate. Use when adding a new crate to the Minal workspace."
disable-model-invocation: true
argument-hint: "[crate-name]"
---

Initialize a new workspace member crate named `$ARGUMENTS`.

## Steps

1. Create `crates/$ARGUMENTS/` directory
2. Create `crates/$ARGUMENTS/Cargo.toml` using the template below
3. Create `crates/$ARGUMENTS/src/lib.rs` with a module doc comment and error type
4. Add `"crates/$ARGUMENTS"` to the root `Cargo.toml` `[workspace].members` array
5. Run `cargo check -p $ARGUMENTS` to verify

## Cargo.toml Template

```toml
[package]
name = "$ARGUMENTS"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"

[dependencies]
thiserror = "2"
tracing = "0.1"
```

## src/lib.rs Template

```rust
//! $ARGUMENTS - [brief description]

use thiserror::Error;

/// Errors for the $ARGUMENTS crate.
#[derive(Debug, Error)]
pub enum Error {
    // Add error variants here
}
```

## Conventions

- Crate names use `minal-` prefix
- Edition 2024, rust-version 1.85
- Errors use `thiserror` custom types
- Logging uses `tracing`
- Public APIs have doc comments
