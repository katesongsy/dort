//! Read-only npm package inventory.
use super::{
    Description, Failure, Installation, LIMIT, TIMEOUT, absolute, catalog, command, run_bounded,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
pub(super) const PREFIX_LIMIT: usize = 8192;
pub(super) const WARNING: &str = "Finds the npm global prefix, then lists packages there. npm uses Node from the displayed PATH. Configuration can run code and change caches or logs. Queries stop after 10 seconds or 4 MiB. No packages are installed.";
pub(super) fn prefix_arguments() -> Vec<String> {
    [
        "prefix",
        "--global",
        "--offline",
        "--ignore-scripts",
        "--logs-max=0",
        "--update-notifier=false",
        "--audit=false",
        "--fund=false",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
fn prefix(bytes: &[u8]) -> Result<PathBuf, Failure> {
    let text = std::str::from_utf8(bytes).map_err(|_| Failure::Malformed)?;
    let text = text.strip_suffix('\n').unwrap_or(text);
    if text.is_empty() || text.len() > 4096 || text.chars().any(char::is_control) {
        return Err(Failure::Malformed);
    }
    let path = PathBuf::from(text);
    if !absolute(&path) || path.components().count() > 64 {
        return Err(Failure::Malformed);
    }
    Ok(path)
}
pub(super) fn inspect(
    probe: &Description,
    inventory: &mut Description,
) -> Result<Vec<Installation>, Failure> {
    inspect_with_timeout(probe, inventory, TIMEOUT)
}
fn inspect_with_timeout(
    probe: &Description,
    inventory: &mut Description,
    timeout: Duration,
) -> Result<Vec<Installation>, Failure> {
    let start = Instant::now();
    let bytes = run_bounded(command(probe), PREFIX_LIMIT, timeout)?;
    let prefix = prefix(&bytes)?;
    // The provider's path is only an argv value, never opened, canonicalized or executed.
    inventory.arguments.extend([
        "--prefix".into(),
        prefix.to_str().ok_or(Failure::Malformed)?.into(),
    ]);
    inventory.npm_global_prefix = Some(prefix);
    let remaining = timeout
        .checked_sub(start.elapsed())
        .filter(|t| !t.is_zero())
        .ok_or(Failure::Timeout)?;
    run_bounded(command(inventory), LIMIT - PREFIX_LIMIT, remaining)
        .and_then(|bytes| catalog::parse(inventory.query, &bytes))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefix_and_list_share_one_deadline_and_join_their_children() {
        use std::{fs, os::unix::fs::PermissionsExt};
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().canonicalize().unwrap();
        let npm = home.join("npm");
        fs::write(
            &npm,
            r#"#!/bin/sh
if [ "$1" = prefix ]; then
printf P >> "$HOME/calls"
/bin/sleep 1
printf '%s\n' "$HOME/globals"
else
printf L >> "$HOME/calls"
/bin/sleep 3
printf F >> "$HOME/calls"
printf '{"dependencies":{}}'
fi
"#,
        )
        .unwrap();
        fs::set_permissions(&npm, fs::Permissions::from_mode(0o700)).unwrap();
        let p =
            super::super::Preview::npm_inventory(npm, home.clone(), vec![home.clone()]).unwrap();
        let mut inventory = p.description().clone();
        assert!(matches!(
            inspect_with_timeout(
                p.prefix_probe().unwrap(),
                &mut inventory,
                Duration::from_secs(3)
            ),
            Err(Failure::Timeout)
        ));
        assert_eq!(fs::read_to_string(home.join("calls")).unwrap(), "PL");
    }
    #[test]
    fn prefix_is_one_bounded_absolute_path_not_provider_content() {
        assert_eq!(
            prefix(b"/some/prefix\n").unwrap(),
            PathBuf::from("/some/prefix")
        );
        assert_eq!(
            prefix("/a/工具".as_bytes()).unwrap(),
            PathBuf::from("/a/工具")
        );
        for bytes in [
            b"".as_slice(),
            b"relative\n",
            b"/a\n/b\n",
            b"/a\n\n",
            b"/a/../b\n",
            b"/a/./b",
            b"/a\0b",
            b"/a\r\n",
            b"/a\x1bb",
            b"\xff",
        ] {
            assert!(matches!(prefix(bytes), Err(Failure::Malformed)));
        }
        assert!(prefix(format!("/{}", "a".repeat(4096)).as_bytes()).is_err());
        assert!(prefix(format!("/{}", vec!["a"; 65].join("/")).as_bytes()).is_err());
    }
}
