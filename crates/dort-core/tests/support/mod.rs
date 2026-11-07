#![allow(dead_code)]
use dort_core::{
    context::HostContext,
    platform::{Architecture, Platform},
};
use std::path::PathBuf;
use tempfile::TempDir;

/// A separate-device fixture: Linux keeps its ordinary /dev/shm probe; macOS
/// accepts only the private root marked by the explicitly opted-in image launcher.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn cross_device_dir() -> Option<TempDir> {
    #[cfg(target_os = "linux")]
    {
        let result = tempfile::tempdir_in("/dev/shm");
        if result.is_err() {
            eprintln!("SKIP cross-device case: /dev/shm is unavailable");
        }
        result.ok()
    }
    #[cfg(target_os = "macos")]
    {
        use std::{io::Read, os::unix::fs::MetadataExt};
        let Some(root) = std::env::var_os("DORT_TEST_CROSS_DEVICE_ROOT") else {
            eprintln!("SKIP cross-device case: opt-in macOS image fixture is unavailable");
            return None;
        };
        let root = PathBuf::from(root);
        assert!(root.is_absolute());
        assert_eq!(root.canonicalize().unwrap(), root);
        let metadata = std::fs::symlink_metadata(&root).unwrap();
        assert!(metadata.is_dir());
        assert_eq!(metadata.uid(), rustix::process::geteuid().as_raw());
        assert_eq!(metadata.mode() & 0o777, 0o700);
        let marker = root.join(".dort-cross-device-fixture");
        let metadata = std::fs::symlink_metadata(&marker).unwrap();
        assert!(metadata.is_file());
        assert_eq!(metadata.uid(), rustix::process::geteuid().as_raw());
        assert_eq!(metadata.mode() & 0o777, 0o600);
        assert_eq!(metadata.nlink(), 1);
        let file = std::fs::File::from(
            rustix::fs::open(
                &marker,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::CLOEXEC
                    | rustix::fs::OFlags::NOFOLLOW,
                rustix::fs::Mode::empty(),
            )
            .unwrap(),
        );
        let opened = file.metadata().unwrap();
        assert_eq!(
            (opened.dev(), opened.ino()),
            (metadata.dev(), metadata.ino())
        );
        let mut token = String::new();
        file.take(128).read_to_string(&mut token).unwrap();
        assert_eq!(token, "dort-disposable-cross-device-v1\n");
        Some(tempfile::tempdir_in(root).unwrap())
    }
}

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
