//! Fixed installer protocol. Both pipes are drained fairly and discarded.
#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
use super::{ProcessOutcome, Scope, arguments};
use std::{
    io::Read,
    os::unix::process::CommandExt,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(super) fn run(scope: &Scope, timeout: Duration) -> ProcessOutcome {
    let mut command = Command::new(&scope.executable);
    command
        .env_clear()
        .env("HOME", &scope.home)
        .env("XDG_CONFIG_HOME", scope.home.join(".config"))
        .env("XDG_CACHE_HOME", scope.home.join(".cache"))
        .env("XDG_DATA_HOME", scope.home.join(".local/share"))
        .env("XDG_STATE_HOME", scope.home.join(".local/state"))
        .env(
            "PATH",
            std::env::join_paths(&scope.search_dirs).expect("validated search scope"),
        )
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("NO_COLOR", "1")
        .current_dir("/")
        .args(arguments(scope))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    if scope.brew.is_some() {
        super::brew::environment(&mut command);
    } else if scope.npm.is_none() {
        command
            .env("MISE_NO_CONFIG", "1")
            .env("MISE_NO_ENV", "1")
            .env("MISE_NO_HOOKS", "1")
            .env("MISE_YES", "1")
            .env("MISE_AUTO_INSTALL", "0");
    }
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(_) => return ProcessOutcome::Unavailable,
    };
    let result = (|| {
        let mut stdout = child.stdout.take().ok_or(ProcessOutcome::Pipe)?;
        let mut stderr = child.stderr.take().ok_or(ProcessOutcome::Pipe)?;
        use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
        for pipe in [&stdout as &dyn std::os::fd::AsFd, &stderr] {
            let flags = fcntl_getfl(pipe).map_err(|_| ProcessOutcome::Pipe)?;
            fcntl_setfl(pipe, flags | OFlags::NONBLOCK).map_err(|_| ProcessOutcome::Pipe)?;
        }
        let start = Instant::now();
        let (mut out_done, mut err_done) = (false, false);
        loop {
            if start.elapsed() >= timeout {
                return Err(ProcessOutcome::Timeout);
            }
            for (pipe, done) in [
                (&mut stdout as &mut dyn Read, &mut out_done),
                (&mut stderr as &mut dyn Read, &mut err_done),
            ] {
                if *done {
                    continue;
                }
                let mut buffer = [0u8; 8192];
                match pipe.read(&mut buffer) {
                    Ok(0) => *done = true,
                    Ok(_) => (),
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                        ) => {}
                    Err(_) => return Err(ProcessOutcome::Pipe),
                }
            }
            if let Some(status) = child.try_wait().map_err(|_| ProcessOutcome::Wait)?
                && out_done
                && err_done
            {
                return Ok(if status.success() {
                    ProcessOutcome::Success
                } else {
                    ProcessOutcome::Failed
                });
            }
            thread::sleep(Duration::from_millis(1));
        }
    })();
    // Also end descendants after normal exit. No owned background jobs survive.
    if let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32) {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    let _ = child.wait();
    result.unwrap_or_else(|e| e)
}
