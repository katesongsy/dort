//! UI-independent observation/planning and experimental recoverable one-file adoption.
//! Public CLI writes, general deployment and external package work remain deferred.
#[cfg(not(unix))]
compile_error!("Dort currently supports macOS and Ubuntu only.");

pub mod adoption;
pub mod context;
pub mod discovery;
pub mod error;
pub mod journal;
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
