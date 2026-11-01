//! Shared, UI-independent environment models and read-only validation.
//! Live-file observation, planning, and deployment are deliberately not implemented yet.
#[cfg(not(unix))]
compile_error!("Dort currently supports macOS and Ubuntu only.");

pub mod context;
pub mod error;
pub mod local;
pub mod manifest;
pub mod model;
pub mod platform;
pub mod profiles;
pub mod state;

pub use error::{Error, Result};
pub use manifest::Manifest;
