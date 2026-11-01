use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{operation}: {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("manifest validation failed:\n{}", .0.join("\n"))]
    Validation(Vec<String>),
    #[error("{0}")]
    Invalid(String),
    #[error("another Dort writer holds the application lock")]
    Locked,
    #[error("no active repository; use --repo or --manifest")]
    NoRepository,
}

pub(crate) fn io_error(
    operation: &'static str,
    path: impl Into<PathBuf>,
    source: impl Into<std::io::Error>,
) -> Error {
    Error::Io {
        operation,
        path: path.into(),
        source: source.into(),
    }
}
