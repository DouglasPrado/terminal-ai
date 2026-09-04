//! Terminal AI's memory kernel client: everything that talks to `ai-memory`.
//!
//! Depends on `terminal-ai-domain` and nothing else internal — no `tauri`, no `persistence`. That
//! is the seam Constitution VII asks for: the deferred daemon can reuse this crate unchanged, and
//! records *about* the kernel (wiring bindings, the migration log) are written by the composition
//! root, exactly as the usage poller's snapshots already are.
#![forbid(unsafe_code)]

pub mod cli;
pub mod http;
pub mod kernel;
pub mod migration;
pub mod probe;
pub mod runtime;
pub mod scope;
pub mod supervisor;
pub mod token;
pub mod wiring;
