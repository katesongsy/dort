//! UI-independent planning, file lifecycle and guarded recovery for Dort.
//! Package installation supports Homebrew and npm. Generated files support read-only previews.
#[cfg(not(unix))]
compile_error!("Dort supports macOS, Linux and WSL.");

pub mod adoption;
pub mod batch;
mod batch_execution;
mod batch_journal;
pub mod bootstrap;
pub mod context;
pub mod discovery;
pub mod discovery_ignore;
pub mod error;
pub mod generated;
pub mod git;
pub mod journal;
pub mod lifecycle;
mod lifecycle_journal;
pub mod local;
pub mod manifest;
pub mod manifest_edit;
pub mod model;
pub mod observation;
pub mod planner;
pub mod platform;
pub mod profiles;
pub mod recovery;
pub mod state;
pub mod template;
pub mod template_files;
mod tool_journal;
pub mod tools;

pub use error::{Error, Result};
pub use manifest::Manifest;

pub mod environment;
