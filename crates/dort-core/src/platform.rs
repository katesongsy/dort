use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Macos,
    Linux,
}

impl Platform {
    pub fn detect() -> Result<Self> {
        match std::env::consts::OS {
            "macos" => Ok(Self::Macos),
            "linux" => Ok(Self::Linux),
            other => Err(Error::Invalid(format!(
                "unsupported operating system: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Architecture {
    X86_64,
    Aarch64,
}

impl Architecture {
    pub fn detect() -> Result<Self> {
        match std::env::consts::ARCH {
            "x86_64" => Ok(Self::X86_64),
            "aarch64" => Ok(Self::Aarch64),
            arch => Err(Error::Invalid(format!("unsupported architecture: {arch}"))),
        }
    }
}
