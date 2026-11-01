//! Shared, UI-independent models, secure read-only observation and pure previews.
//! Live deployment and recovery execution are deliberately not implemented yet.
#[cfg(not(unix))]
compile_error!("Dort currently supports macOS and Ubuntu only.");

pub mod context;
pub mod discovery;
pub mod error;
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
