#![allow(dead_code)]
use dort_core::{
    context::HostContext,
    platform::{Architecture, Platform},
};
use std::path::PathBuf;
use tempfile::TempDir;

pub struct TestHome {
    pub sandbox: TempDir,
    pub host: HostContext,
}

impl TestHome {
    pub fn new() -> Self {
        let sandbox = tempfile::tempdir().unwrap();
        // macOS /var is a symlink; the fixture explicitly uses the real temporary root.
        let root = sandbox.path().canonicalize().unwrap();
        let home = root.join("home");
        std::fs::create_dir(&home).unwrap();
        let host =
            HostContext::new(home, None, None, Platform::Ubuntu, Architecture::X86_64).unwrap();
        Self { sandbox, host }
    }

    pub fn repository(&self, name: &str) -> PathBuf {
        let root = self.sandbox.path().canonicalize().unwrap().join(name);
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("dort.toml"), "schema = 1\n").unwrap();
        root
    }
}
