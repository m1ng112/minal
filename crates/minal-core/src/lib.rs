//! `minal-core` — Terminal emulation core.
//!
//! Provides the VT parser, grid/cell data structures, cursor management,
//! scrollback buffer, and PTY management.

pub mod ansi;
pub mod cell;
pub mod charset;
pub mod cursor;
pub mod grid;
pub mod handler;
pub mod pty;
pub mod scrollback;
pub mod selection;
pub mod term;

mod error;
pub use error::CoreError;
