#!/usr/bin/env python3
"""Disposable POSIX PTY checks; Python stdlib only, no Git or real HOME access."""

import codecs
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import re
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


class Screen:
    """Small bounded VT view for Ratatui's cursor-addressed differential draws.

    Raw byte markers alone miss labels when unchanged cells are not re-emitted.
    Only the cursor/erase operations used by these fixed-size fixtures are needed;
    styling and terminal modes do not affect the text assertions.
    """

    def __init__(self, size):
        self.rows, self.columns = size
        self.cells = [[" "] * self.columns for _ in range(self.rows)]
        self.row = self.column = 0
        self.pending = ""
        self.decoder = codecs.getincrementaldecoder("utf-8")(errors="replace")

    def feed(self, data):
        self.pending += self.decoder.decode(data)
        while self.pending:
            if self.pending.startswith("\x1b"):
                match = re.match(r"\x1b\[([0-?]*)([ -/]*)([@-~])", self.pending)
                if not match:
                    # A CSI may be split across PTY reads.
                    if self.pending == "\x1b" or self.pending.startswith("\x1b["):
                        break
                    self.pending = self.pending[2:]
                    continue
                arguments, _, command = match.groups()
                self.pending = self.pending[match.end():]
                if arguments.startswith("?"):
                    continue
                values = [int(value or 0) for value in arguments.split(";")]
                amount = values[0] or 1
                if command in "Hf":
                    self.row = min(self.rows - 1, amount - 1)
                    self.column = min(self.columns - 1, (values[1] or 1) - 1 if len(values) > 1 else 0)
                elif command == "A":
                    self.row = max(0, self.row - amount)
                elif command == "B":
                    self.row = min(self.rows - 1, self.row + amount)
                elif command == "C":
                    self.column = min(self.columns - 1, self.column + amount)
                elif command == "D":
                    self.column = max(0, self.column - amount)
                elif command == "G":
                    self.column = min(self.columns - 1, amount - 1)
                elif command == "J" and values[0] in (2, 3):
                    self.cells = [[" "] * self.columns for _ in range(self.rows)]
                elif command == "K":
                    start = 0 if values[0] in (1, 2) else self.column
                    end = self.column + 1 if values[0] == 1 else self.columns
                    self.cells[self.row][start:end] = [" "] * (end - start)
                continue
            character, self.pending = self.pending[0], self.pending[1:]
            if character == "\r":
                self.column = 0
            elif character == "\n":
                self.row = min(self.rows - 1, self.row + 1)
            elif character >= " ":
                self.cells[self.row][self.column] = character
                self.column = min(self.columns - 1, self.column + 1)
        assert len(self.pending) < 256, "Unbounded terminal escape sequence"

    def text(self):
        return "\n".join("".join(row) for row in self.cells).encode()


class Session:
    def __init__(self, arguments, env, size=(24, 80)):
        self.master, self.slave = pty.openpty()
        self.before = termios.tcgetattr(self.slave)
        self.data = b""
        self.screen = Screen(size)
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
                    data = os.read(fd, 65536)
                    self.data += data
                    self.screen.feed(data)
                except OSError:
                    pass
                assert len(self.data) <= 1024 * 1024, "PTY output exceeded test bound"

    def wait(self, marker):
        deadline = time.monotonic() + TIMEOUT
        while marker not in self.screen.text() and time.monotonic() < deadline:
            self.pump()
            if self.code is not None:
                break
        assert marker in self.screen.text(), "Expected terminal state was not rendered"

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
    screen = Screen((2, 20))
    screen.feed(b"Git executes\x1b[")
    screen.feed(b"1;11Hable\x1b[2;1Hlabel")
    assert b"Git executable" in screen.text()
    assert b"label" in screen.text()
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

        # Text carries navigation/state meaning even with a low-color terminal.
        with Session([binary, "--dry-run", "tui"], dict(env, TERM="vt100", NO_COLOR="1"), (14, 48)) as session:
            session.wait(b"loaded.")
            for label in [b"1 Overview", b"2 Files", b"3 Recovery", b"4 Checks", b"5 Git",
                          b"DRY RUN", b"Create/attach", b"Choose repository", b"Discover files", b"Check a manifest"]:
                session.wait(label)
            session.send(b"?")
            session.wait(b"Space: Actions")
            session.send(b"\x1b[6~" * 30)
            session.settle()
            assert b"Space: Actions" not in session.screen.text()
            session.send(b"\x1b[H")
            session.wait(b"Space: Actions")
            session.send(b"\x1b")
            session.wait(b"Create/attach")
            session.send(b"q")
            session.finish()
        assert not list(home.iterdir()), "Compact navigation/help created state"
        print("PASS compact low-color navigation and Home help reset without state")

        # A directory remains inspectable but cannot overwrite an adoption draft.
        discovery_home = base / "discovery-home"
        discovery_home.mkdir(mode=0o700)
        (discovery_home / ".config" / "nvim").mkdir(parents=True, mode=0o700)
        (discovery_home / ".config" / "nvim" / "init.lua").write_text("ordinary settings\n")
        discovery_env = dict(env, HOME=str(discovery_home),
                             XDG_CONFIG_HOME=str(discovery_home / ".config"),
                             XDG_STATE_HOME=str(discovery_home / ".local/state"))
        with Session([binary, "tui"], discovery_env, (24, 80)) as session:
            session.wait(b"loaded.")
            session.send(b"f")
            session.wait(b"Not eligible")
            session.send(b"/nvim\x1b")
            session.settle()
            session.send(b"\r")
            session.wait(b"[Not eligible]")
            assert b"TARGET >" not in session.screen.text()
            session.send(b"i")
            session.wait(b"Directories, links and special files")
            session.send(b"q")
            session.finish()
        assert not (discovery_home / ".config" / "dort").exists()
        assert not (discovery_home / ".local").exists()
        assert (discovery_home / ".config" / "nvim" / "init.lua").is_file()
        print("PASS ineligible discovered directory remains inspectable without adoption or state")

        # Text fields must consume shortcut letters, and a missing prerequisite
        # must return to the retained draft without creating state.
        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.send(b"n")
            session.wait("Path · Focus".encode())
            session.send(b"q?/\r\r")
            session.wait(b"Git executable not found")
            session.send(b"\x1b")
            session.settle()
            session.send(b"\x03")
            session.finish()
        assert not list(home.iterdir()), "A rejected onboarding read created state"
        print("PASS retained text draft and missing Git refusal")

        unregistered = base / "unregistered"
        unregistered.mkdir(mode=0o700)
        (unregistered / "dort.toml").write_text("schema=1\n")
        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.send(b"p\r")
            session.wait("Path · Focus".encode())
            session.send(os.fsencode(unregistered) + b"\r\r")
            session.wait(b"configuration unchanged.")
            session.send(b"q")
            session.finish()
        assert not list(home.iterdir()), "Session selection persisted configuration"
        print("PASS explicit session-only repository selection")

        standalone = base / "standalone-q?.toml"
        standalone.write_text("schema=1\n[profiles]\navailable=['dev']\n")
        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.send(b"v")
            session.wait("Manifest path · Focus".encode())
            session.send(os.fsencode(standalone) + b"\r\r")
            session.wait(b"Structural check passed")
            session.wait(b"Checks | Standalone manifest")
            session.wait(b"Valid")
            session.wait(b"live files and installed tools were not inspected")
            session.send(b"\x1b[6~" * 3)
            session.wait(b"Capability limits")
            session.send(b"\x1b[H")
            session.wait(b"Structural check only")
            session.send(b"j")
            session.wait(b"Checks | Diagnostics")
            session.send(b"m")
            session.wait(b"Checks | Standalone manifest")
            session.send(b"v\t\r")  # retained Check button -> Profiles
            session.wait(b"Profiles | standalone manifest")
            session.send(b"\x1b[B\r\x1b[B\r")  # dev toggle, Apply
            session.wait(b"Standalone profile additions validated")
            session.send(b"\x1b[Z\r")  # previous field: Check
            session.wait(b"Structural check passed")
            session.send(b"q")
            session.finish()
        assert not (home / ".config").exists() and not (home / ".local").exists()
        print("PASS independent standalone check and nonpersistent profile additions")

        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.send(b"4j")
            session.wait(b"Diagnostics | Dort")
            session.wait(b"Registered repositories: 0")
            session.send(b"r")
            session.wait(b"Read-only Diagnostics loaded")
            session.send(b" /about\r")
            session.wait(b"| About")
            session.wait(b"Experimental guarded local workflows")
            session.send(b"\x1b")
            session.wait(b"Diagnostics | Dort")
            session.send(b"q")
            session.finish()
        assert not list(home.iterdir()), "Diagnostics/About created local state"
        print("PASS Diagnostics, About/version and scope-preserving return without state")

        # Unavailable registry is distinct from a successfully observed empty history.
        config = home / ".config" / "dort"
        config.mkdir(parents=True, mode=0o700)
        (config / "local.toml").write_text("selected_repository = 'private-config-marker'\ninvalid toml")
        with Session([binary, "tui"], env, (14, 48)) as session:
            session.wait(b"loaded.")
            session.send(b"4j")
            session.wait(b"Registry Unknown")
            session.wait(b"History 0")
            session.wait(b"Blocks: 0")
            session.send(b"\x1b[6~")
            session.wait(b"Configuration unavailable")
            assert b"private-config-marker" not in session.data
            session.send(b"\x1b[H")
            session.wait(b"Diagnostics | Dort")
            session.send(b"q")
            session.finish()
        assert not (home / ".local").exists(), "Unavailable Diagnostics created state"
        (config / "local.toml").unlink()
        config.rmdir()
        config.parent.rmdir()
        print("PASS compact typed Diagnostics unknown registry versus empty history, no source excerpts")

        candidate = home / ".q?config"
        candidate.write_text("private-body-marker\n")
        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.send(b"f")
            session.wait(b"Eligible hint")
            session.send(b"\r")
            session.wait(b"Selected item")
            session.send(b"\t\x1b[C\r")  # visible Adopt selected action
            session.wait("TARGET · Focus".encode())
            session.send(b"\t\t\t\r")  # fields -> Review, no repository selected
            session.wait(b"Read failed:")
            session.send(b"\x1b")
            session.settle()
            session.wait(b".q?config")
            assert b"private-body-marker" not in session.data
            session.send(b"\x03")
            session.finish()
        assert not (home / ".config").exists() and not (home / ".local").exists()
        candidate.unlink()
        print("PASS repository-independent discovery and retained adoption refusal")

        # A deterministic fake executable implements only the fixed bootstrap calls.
        # /bin/mkdir writes exclusively inside the private disposable stage.
        bin_dir = base / "bin"
        bin_dir.mkdir(mode=0o700)
        git = bin_dir / "git"
        git.write_text(
            "#!/bin/sh\nfor arg do\ncase \"$arg\" in\n"
            "init) /bin/mkdir .git || exit 90; "
            "printf 'ref: refs/heads/main\\n' > .git/HEAD; exit 0;;\n"
            "config) exit 1;;\nrev-parse) pwd -P; exit 0;;\n"
            "status) printf '# branch.head main\\000? q?name\\000'; exit 0;;\n"
            "diff) case \" $* \" in *\" --patch \"*) "
            f"printf x >> '{base / 'patch-calls'}'; "
            "printf 'PRIVATE-PATCH\\033[31m\\n';; "
            "*) printf ':100644 100644 0000000000000000000000000000000000000000 "
            "0000000000000000000000000000000000000000 M\\000q?name\\000';; "
            "esac; exit 0;;\n"
            "esac\ndone\nexit 94\n"
        )
        git.chmod(0o700)
        git_only = base / "git-only"
        git_only.mkdir(mode=0o700)
        with Session([binary, "tui"], dict(env, PATH=str(bin_dir))) as session:
            session.wait(b"loaded.")
            session.send(b"5g" + os.fsencode(git_only) + b"\r")
            session.wait(b"Git read completed")
            session.wait(b"q?name")
            session.send(b"/q?/\x1b")  # literal text, no quit/help/actions
            session.wait(b"0 displayed")
            session.send(b"\x1b[3~u")  # clear display filter; unstaged summary
            session.wait(b"Unstaged (worktree vs index)")
            session.wait(b"1 entries")
            assert not (base / "patch-calls").exists(), "Summary prefetched a patch"
            assert b"PRIVATE-PATCH" not in session.data
            session.send(b"wyes\r")
            session.wait(b"May expose secrets")
            session.settle()
            assert not (base / "patch-calls").exists(), "yes bypassed disclosure phrase"
            session.send(b"\x1b")
            session.wait(b"1 entries")
            session.send(b"wview patch\r")
            session.wait(b"PRIVATE-PATCH\\x1b[31m")
            session.send(b"\x1b")
            session.wait(b"1 entries")
            session.send(b"r")
            session.wait(b"Git read completed")
            session.settle()
            assert b"PRIVATE-PATCH" not in session.screen.text()
            assert (base / "patch-calls").read_text() == "x", "Refresh re-fetched patch"
            session.send(b"s")
            session.wait(b"Staged (index vs HEAD")
            session.send(b"q")
            session.finish()
        assert not list(home.iterdir()) and not list(git_only.iterdir())
        print("PASS independent Git root, metadata search/summaries and one-view patch disclosure")

        created = base / "created"
        for intent in ("create", "attach"):
            onboarding_home = base / f"home-{intent}"
            onboarding_home.mkdir(mode=0o700)
            onboarding_env = dict(
                env, HOME=str(onboarding_home), PATH=str(bin_dir),
                XDG_CONFIG_HOME=str(onboarding_home / ".config"),
                XDG_STATE_HOME=str(onboarding_home / ".local/state"),
            )
            with Session([binary, "tui"], onboarding_env) as session:
                session.wait(b"loaded.")
                session.send(b"n")
                session.wait("Path · Focus".encode())
                session.send(os.fsencode(created) + b"\r\r")
                session.wait(b"Preview (metadata only)")
                session.send(b"\x1b")
                session.settle()
                assert not list(onboarding_home.iterdir()), "Preview/back created local state"
                if intent == "create":
                    assert not created.exists(), "Preview created repository"
                session.send(b"\r")  # retained draft -> fresh preview
                session.settle()
                session.send(b"\r")
                session.wait(b"Confirm")
                session.send(b"yes\r")
                session.wait(b"Operation completed.")
                session.send(b"q")
                session.finish()
            assert (created / ".git/HEAD").is_file()
            assert (created / "dort.toml").read_text() == "schema = 1\n"
            config_text = (onboarding_home / ".config/dort/local.toml").read_text()
            assert str(created) in config_text and "active_repository" in config_text
            print("PASS guided", intent, "with preview/back, explicit approval and clean exit")

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

        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.send(b"3/q?/\x1b")
            session.wait(b"No matches")
            session.send(b"\x1b[3~")  # clear display filter, no ID transcription
            session.wait(b"Showing 1")
            session.send(b"\r")
            session.wait(b"Repository ID:")
            session.wait(b"Transaction ID:")
            session.send(b"b")
            session.wait(b"Rollback schema 2")
            session.wait(b"Inverse paths:")
            session.send(b"\x1b")
            session.wait(b"Showing 1")
            assert target.is_symlink(), "Preview/back performed rollback"
            session.send(b"b")
            session.wait(b"Rollback schema 2")
            session.send(b"\r")
            session.wait(b"Confirm")
            session.send(b"yes\r")
            session.wait(b"Observations refreshed.")
            assert not target.exists() and not target.is_symlink()
            # A new action needs a new actual preview and confirmation.
            session.send(b"1a")
            session.wait(b"Preview (metadata only)")
            session.send(b"\r")
            session.wait(b"Confirm")
            session.send(b"yes\r")
            session.wait(b"Operation completed.")
            session.send(b"q")
            session.finish()
        assert target.is_symlink() and (repo / "source").read_text() == "ordinary settings\n"
        print("PASS searchable all-repository history, full IDs and guarded rollback without transcription")

        checkpoints = {str(path.relative_to(state)): hashlib.sha256(path.read_bytes()).digest()
                       for path in state.rglob("*.json")}
        with Session([binary, "--dry-run", "tui"], env, (14, 48)) as session:
            session.wait(b"loaded.")
            session.send(b"3/Completed\r")
            session.wait(b"Blockers: 0")
            session.wait(b"Showing 1")
            session.wait(b"Completed")
            session.send(b"\r")  # selected row inspects; never requests rollback
            session.wait(b"Transaction details")
            session.wait(b"Repository ID:")
            session.send(b"\x1b[6~" * 2)
            session.wait(b"Validated checkpoints:")
            session.send(b"\x1b[H")
            session.wait(b"Repository ID:")
            session.send(b"\t\x1b[C\r")  # explicit contextual Rollback page button
            session.wait(b"Rollback schema 2")
            session.send(b"\r")
            session.settle()
            assert b"Confirm" not in session.data, "Dry-run Recovery opened approval"
            session.send(b"\x1b")
            session.wait(b"Transaction details")
            session.send(b"q")
            session.finish()
        assert checkpoints == {str(path.relative_to(state)): hashlib.sha256(path.read_bytes()).digest()
                               for path in state.rglob("*.json")}, "Recovery inspection/preview changed history"
        assert target.is_symlink() and (repo / "source").read_text() == "ordinary settings\n"
        print("PASS compact typed Recovery inspection, focused rollback preview and dry-run without writes")

        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.send(b"2/q?/\x1b")
            session.settle()
            session.wait(b"No matches")
            session.send(b"a")
            session.wait(b"Preview (metadata only)")
            session.wait(b"display filters never narrow")
            session.send(b"\x1b")
            session.settle()
            session.send(b"q")
            session.finish()
        assert target.is_symlink()
        print("PASS display-only file search and full-scope apply preview")

        candidate = home / ".q?config"
        candidate.write_text("ordinary adoption settings\n")
        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.send(b"o")
            session.wait("TARGET · Focus".encode())
            session.send(b"~/.q?config\rconfig\rfiles/config\r")
            session.send(b"\r")  # Review button, not an ordinary field
            session.wait(b"Preview (metadata only)")
            session.send(b"\x1b")
            session.settle()
            assert not candidate.is_symlink() and not (repo / "files/config").exists()
            session.send(b"\r")  # retained Review -> fresh preview
            session.settle()
            session.send(b"\r")
            session.wait(b"Confirm")
            session.send(b"yes\r")
            session.wait(b"Operation completed.")
            session.send(b"q")
            session.finish()
        assert candidate.is_symlink() and candidate.resolve() == repo / "files/config"
        assert (repo / "files/config").read_text() == "ordinary adoption settings\n"
        print("PASS manual adoption retained draft, preview/back and explicit approval")

        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.send(b"2/notconfig\x1b")
            session.settle()
            session.send(b" /delete managed link: enter\r")
            session.wait("Exact ID · Focus".encode())
            session.send(b"config\r\r")
            session.wait(b"Preview (metadata only)")
            session.send(b"\r")
            session.wait(b"Separate deletion acknowledgement")
            session.send(b"yes\r")
            session.settle()
            assert candidate.is_symlink(), "Literal yes bypassed exact-ID acknowledgement"
            session.send(b"\x7f\x7f\x7fconfig\ryes\r")
            session.wait(b"Operation completed.")
            session.send(b"q")
            session.finish()
        assert not candidate.exists() and not candidate.is_symlink()
        assert (repo / "files/config").read_text() == "ordinary adoption settings\n"
        assert target.is_symlink()
        print("PASS exact-ID link deletion outside display filter and separate yes")

        for mode, code in [("normal", 0), ("error", 0), ("panic", 101)]:
            with Session(
                [probe, "--exact", "tui::tests::terminal_cleanup_probe", "--nocapture"],
                dict(env, DORT_TEST_TERMINAL_CLEANUP=mode),
            ) as session:
                session.finish(code)
            print("PASS terminal cleanup", mode)
    print("25 PTY cases passed (disposable HOME, no installed Git requirement)")


if __name__ == "__main__":
    run()
