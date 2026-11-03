#!/usr/bin/env python3
"""Disposable POSIX PTY checks; Python stdlib only, no Git or real HOME access."""

import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time

ROOT = Path(__file__).resolve().parent.parent
TIMEOUT = 20


def artifact(arguments, kind):
    result = subprocess.run(
        ["cargo", *arguments, "--locked", "--message-format=json"],
        cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False,
    )
    if result.returncode:
        raise AssertionError("Cargo prerequisite failed; run the Rust checks for diagnostics")
    for line in result.stdout.splitlines():
        item = json.loads(line)
        if (item.get("reason") == "compiler-artifact"
                and item["target"]["name"] == "dort"
                and item["profile"]["test"] == (kind == "test")
                and item.get("executable")):
            return item["executable"]
    raise AssertionError("Cargo did not report the required executable")


class Session:
    def __init__(self, arguments, env, size=(24, 80)):
        self.master, self.slave = pty.openpty()
        self.before = termios.tcgetattr(self.slave)
        self.data = b""
        self.code = None
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", *size, 0, 0))
        self.status_r, status_w = os.pipe()
        release_r, self.release_w = os.pipe()

        def setup():
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)

        # Keep the controlling terminal alive after the program exits. In particular,
        # macOS otherwise revokes the slave before its restored modes can be checked.
        supervisor = (
            "import subprocess,os,sys; p=subprocess.run(sys.argv[3:]); "
            "os.write(int(sys.argv[1]),str(p.returncode).encode()); "
            "os.read(int(sys.argv[2]),1)"
        )
        try:
            self.process = subprocess.Popen(
                [sys.executable, "-c", supervisor, str(status_w), str(release_r), *arguments],
                stdin=self.slave, stdout=self.slave, stderr=self.slave, env=env,
                preexec_fn=setup, cwd=ROOT, pass_fds=(status_w, release_r),
            )
        except BaseException:
            for fd in (self.master, self.slave, self.status_r, self.release_w):
                os.close(fd)
            raise
        finally:
            os.close(status_w)
            os.close(release_r)

    def __enter__(self):
        return self

    def __exit__(self, *_):
        # Assertion failures/timeouts must not leave a raw terminal or fixture child.
        if self.process.poll() is None:
            os.killpg(self.process.pid, signal.SIGKILL)
        self.process.wait(timeout=5)
        for fd in (self.status_r, self.release_w, self.master, self.slave):
            os.close(fd)

    def pump(self):
        readers = [self.master] + ([self.status_r] if self.code is None else [])
        for fd in select.select(readers, [], [], 0.05)[0]:
            if fd == self.status_r:
                status = os.read(fd, 64)
                assert status, "PTY supervisor exited without reporting status"
                self.code = int(status)
            else:
                try:
                    self.data += os.read(fd, 65536)
                except OSError:
                    pass
                assert len(self.data) <= 1024 * 1024, "PTY output exceeded test bound"

    def wait(self, marker):
        deadline = time.monotonic() + TIMEOUT
        while marker not in self.data and time.monotonic() < deadline:
            self.pump()
            if self.code is not None:
                break
        assert marker in self.data, "Expected terminal state was not rendered"

    def settle(self):
        deadline = time.monotonic() + 0.2
        while time.monotonic() < deadline:
            self.pump()

    def send(self, value):
        os.write(self.master, value)

    def finish(self, expected=0):
        deadline = time.monotonic() + TIMEOUT
        while self.code is None and time.monotonic() < deadline:
            self.pump()
        assert self.code is not None, "PTY program timed out"
        for _ in range(20):
            if not select.select([self.master], [], [], 0)[0]:
                break
            self.pump()
        assert self.code == expected, "Unexpected PTY program exit status"
        before = list(self.before)
        after = termios.tcgetattr(self.slave)
        # PENDIN is a kernel pending-input retype flag, not an application mode.
        before[3] &= ~getattr(termios, "PENDIN", 0)
        after[3] &= ~getattr(termios, "PENDIN", 0)
        assert after == before, "Terminal attributes were not restored"
        assert b"\x1b[?1049l" in self.data, "Alternate screen was not left"
        os.write(self.release_w, b"x")
        self.process.wait(timeout=5)


def run():
    binary = artifact(["build", "-p", "dort"], "binary")
    probe = artifact(["test", "-p", "dort", "--bin", "dort", "--no-run"], "test")
    with tempfile.TemporaryDirectory(prefix="dort-terminal-") as temporary:
        base = Path(temporary).resolve()
        home = base / "home"
        home.mkdir(mode=0o700)
        env = {
            "HOME": str(home), "XDG_CONFIG_HOME": str(home / ".config"),
            "XDG_STATE_HOME": str(home / ".local/state"), "PATH": "/no-external-programs",
            "TERM": "xterm-256color",
        }
        for args, key, size in [([], b"q", (24, 80)), (["tui"], b"\x03", (24, 80)),
                                (["--dry-run", "tui"], b"q", (6, 40))]:
            with Session([binary, *args], env, size) as session:
                session.wait(b"Dort")
                session.send(key)
                session.finish()
            assert not list(home.iterdir()), "Read-only TUI startup created state"
            print("PASS entry/exit", args, size)

        # An explicit fixture registration, equivalent to the Rust TestHome helper.
        # A dummy Git marker suffices for file services; no Git executable is needed.
        repo = base / "repo"
        (repo / ".git").mkdir(parents=True)
        (repo / "source").write_text("ordinary settings\n")
        (repo / "dort.toml").write_text(
            "schema=1\n[[files]]\nid='editor'\nsource='source'\n"
            "target='~/.editor'\nmode='link'\n"
        )
        config = home / ".config/dort"
        config.mkdir(parents=True, mode=0o700)
        identity = hashlib.sha256(os.fsencode(repo)).hexdigest()
        local = config / "local.toml"
        local.write_text(
            f'schema=1\nactive_repository="{identity}"\n'
            f'[repositories."{identity}"]\nroot={json.dumps(str(repo))}\n'
        )
        local.chmod(0o600)
        target = home / ".editor"
        state = home / ".local/state"
        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.send(b"a")
            session.wait(b"Preview (metadata only)")
            session.send(b"\r")
            session.wait(b"Confirm")
            session.send(b"yes\x1b")
            session.settle()
            session.send(b"\x1b")
            session.settle()
            session.send(b"q")
            session.finish()
        assert not target.is_symlink() and not target.exists() and not state.exists()
        print("PASS confirmation cancellation without state or live effects")

        with Session([binary, "--dry-run", "tui"], env) as session:
            session.wait(b"loaded.")
            session.send(b"a")
            session.wait(b"Preview (metadata only)")
            session.send(b"\r")
            session.settle()
            assert b"Confirm" not in session.data, "Dry-run opened confirmation"
            session.send(b"q")
            session.finish()
        assert not target.exists() and not target.is_symlink() and not state.exists()
        print("PASS dry-run cannot approve")

        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.send(b"a")
            session.wait(b"Preview (metadata only)")
            session.send(b"\r")
            session.wait(b"Confirm")
            session.send(b"yes\r")
            deadline = time.monotonic() + TIMEOUT
            while not target.is_symlink() and time.monotonic() < deadline:
                session.pump()
            assert target.is_symlink(), "Approved TUI apply did not publish the link"
            session.send(b"q")
            session.finish()
        assert target.resolve() == repo / "source"
        assert (repo / "source").read_text() == "ordinary settings\n"
        print("PASS approved apply and clean exit")

        for mode, code in [("normal", 0), ("error", 0), ("panic", 101)]:
            with Session(
                [probe, "--exact", "tui::tests::terminal_cleanup_probe", "--nocapture"],
                dict(env, DORT_TEST_TERMINAL_CLEANUP=mode),
            ) as session:
                session.finish(code)
            print("PASS terminal cleanup", mode)
    print("9 PTY cases passed (disposable HOME, no installed Git requirement)")


if __name__ == "__main__":
    run()
