use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Macos,
    Ubuntu,
}

impl Platform {
    pub fn detect() -> Result<Self> {
        match std::env::consts::OS {
            "macos" => Ok(Self::Macos),
            "linux" => {
                let text = std::fs::read_to_string("/etc/os-release").map_err(|e| {
                    crate::error::io_error("read OS identity", "/etc/os-release", e)
                })?;
                Self::from_os_release(&text)
            }
            other => Err(Error::Invalid(format!(
                "unsupported operating system: {other}"
            ))),
        }
    }

    pub fn from_os_release(text: &str) -> Result<Self> {
        let id = text.lines().find_map(|line| {
            line.strip_prefix("ID=")
                .map(|v| v.trim().trim_matches(['"', '\'']))
        });
        match id {
            Some("ubuntu") => Ok(Self::Ubuntu),
            _ => Err(Error::Invalid(
                "only Ubuntu Linux is supported (ID=ubuntu required)".into(),
            )),
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
