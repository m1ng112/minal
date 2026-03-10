//! `minal-core` — Terminal emulation core.
//!
//! Provides the VT parser, grid/cell data structures, cursor management,
//! scrollback buffer, and PTY management.

pub mod ansi;
pub mod cell;
pub mod cursor;
pub mod grid;
pub mod handler;
pub mod term;

mod error;
pub use error::CoreError;
