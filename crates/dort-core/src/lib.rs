//! UI-independent planning and experimental journalled adoption/link/unmanage services.
//! Public CLI writes and external package work remain deferred.
#[cfg(not(unix))]
compile_error!("Dort currently supports macOS and Ubuntu only.");

pub mod adoption;
pub mod context;
pub mod discovery;
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
pub mod state;

pub use error::{Error, Result};
pub use manifest::Manifest;
