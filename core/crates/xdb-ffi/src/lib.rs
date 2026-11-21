//! Standalone XDB bindings. The combined rfb-ffi build shares this API source.
#![cfg(feature = "xdb")]

uniffi::setup_scaffolding!();

// Keep the API identical while emitting scaffolding in the xdb_ffi namespace.
// This crate is built from the workspace, which also contains the shared source.
#[path = "../../rfb-ffi/src/ios.rs"]
mod ios;
