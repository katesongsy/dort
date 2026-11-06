//! UI-independent planning, experimental journalled bootstrap/file lifecycle and guarded recovery.
//! External package work, remote Git workflows and generated/secret deployment remain deferred.
#[cfg(not(unix))]
compile_error!("Dort currently supports macOS and Ubuntu only.");

pub mod adoption;
pub mod batch;
mod batch_execution;
mod batch_journal;
pub mod bootstrap;
pub mod context;
pub mod discovery;
pub mod discovery_ignore;
pub mod error;
pub mod git;
pub mod journal;
pub mod lifecycle;
mod lifecycle_journal;
pub mod local;
pub mod manifest;
pub mod model;
pub mod observation;
pub mod planner;
pub mod platform;
pub mod profiles;
pub mod recovery;
pub mod state;

pub use error::{Error, Result};
pub use manifest::Manifest;
