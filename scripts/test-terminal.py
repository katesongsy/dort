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
            # Python may coerce a missing/C locale in its own environment; pass
            # the requested fixture environment explicitly to the actual binary.
            "import subprocess,os,sys,json; p=subprocess.run(sys.argv[4:],env=json.loads(sys.argv[3])); "
            "os.write(int(sys.argv[1]),str(p.returncode).encode()); "
            "os.read(int(sys.argv[2]),1)"
        )
        try:
            self.process = subprocess.Popen(
                [sys.executable, "-c", supervisor, str(status_w), str(release_r), json.dumps(env), *arguments],
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

    def mouse(self, button, column, row):
        # SGR coordinates are one-based; button 0 clicks, 64/65 wheel up/down.
        self.send(f"\x1b[<{button};{column + 1};{row + 1}M".encode())
        if button == 0:
            self.send(f"\x1b[<0;{column + 1};{row + 1}m".encode())

    def click_label(self, label, offset=0, min_row=0):
        self.wait(label.encode())
        self.settle()
        for row, cells in enumerate(self.screen.cells):
            text = "".join(cells)
            if row >= min_row and label in text:
                self.mouse(0, text.index(label) + offset, row)
                return
        raise AssertionError("Click target was not visible")

    def navigate(self, group, item):
        if self.screen.columns < 80:
            self.click_label("F10")
            self.click_label(group + " >")
        else:
            self.click_label(group + (" ▾" if (group + " ▾").encode() in self.screen.text() else " v"))
        self.click_label(item, min_row=3)

    def destination(self, group):
        items = {"Overview": "Overview", "Files": "Managed",
                 "Recovery": "All repositories", "Checks": "Manifest checks [m]"}
        if group == "Git":
            self.navigate("Git", "Choose Git scope [g]")
            self.send(b"\x1b")
            self.settle()
        else:
            self.navigate(group, items[group])

    def resize(self, size):
        self.settle()
        self.screen = Screen(size)
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", *size, 0, 0))
        os.killpg(self.process.pid, signal.SIGWINCH)

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
        for mode in (1000, 1002, 1003, 1015, 1006):
            enabled = f"\x1b[?{mode}h".encode()
            disabled = f"\x1b[?{mode}l".encode()
            assert enabled in self.data, "Mouse capture was not enabled"
            assert self.data.rfind(disabled) > self.data.rfind(enabled), "Mouse capture was not restored"
        os.write(self.release_w, b"x")
        self.process.wait(timeout=5)


def has_color_sgr(data):
    # Reset/default colors and bold/reverse/underline are allowed in monochrome.
    return any(value in range(30, 39) or value in range(40, 49)
               or value in range(90, 108)
               for match in re.finditer(rb"\x1b\[([0-9;]*)m", data)
               for value in [int(part or b"0") for part in match[1].split(b";")])


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
            "TERM": "xterm-256color", "LANG": "C.UTF-8",
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
            for label in [b"F10", b"Overview", b"DRY RUN", b"Create/attach",
                          b"Choose repository", b"Files: Discover", b"Checks: manifest"]:
                session.wait(label)
            session.send(b"?")
            session.wait(b"Help: Overview")
            session.send(b"\x1b[6~" * 30)
            session.settle()
            assert b"Help: Overview" not in session.screen.text()
            session.send(b"\x1b[H")
            session.wait(b"Help: Overview")
            session.send(b"\x1b")
            session.wait(b"Create/attach")
            session.send(b"q")
            session.finish()
            assert session.screen.text().isascii() and session.data.isascii()
            assert not has_color_sgr(session.data), "Monochrome emitted explicit colors"
        assert not list(home.iterdir()), "Compact navigation/help created state"
        print("PASS compact low-color navigation and Home help reset without state")

        for menu_size in [(14, 48), (24, 80), (40, 140)]:
            with Session([binary, 'tui'], env, menu_size) as session:
                session.wait(b'loaded.')
                session.send(b'\x1b[21~')  # F10
                session.settle(); assert b'Help:' not in session.screen.text()
                session.send(b'\x1b[C')  # Overview -> Files menu/group
                session.settle()
                if menu_size[1] < 80:
                    session.send(b'\r')
                session.wait(b'Discover [f]')
                session.send(b'\x1b[B')
                session.settle()
                session.send(b'\x1bOP')
                session.wait(b'Help: Navigation menus')
                session.send(b'\r')  # Help cannot select the retained menu entry
                session.settle()
                session.send(b'\x1b')
                session.settle(); assert b'Help:' not in session.screen.text()  # not the retained selected label in Help
                session.send(b'\r')
                session.wait(b'Discovery loaded')
                session.wait(b'Files / Discover')
                session.send(b'12345')
                session.settle()
                assert b'Files / Discover' in session.screen.text(), 'Numeric aliases changed destination'
                session.send(b"\x1b[21~")  # F10: local menu shortcuts work at every width
                if menu_size[1] < 80:
                    session.send(b"\r")
                session.wait(b"Managed [m]")
                session.send(b"m")
                session.wait(b"Files / Managed")
                session.send(b"\x1b[21~\x1b[C\x1b[C")  # Checks group
                if menu_size[1] < 80:
                    session.send(b"\r")
                session.wait(b"Diagnostics [j]")
                session.send(b"j")
                session.wait(b"Checks | Diagnostics")
                session.send(b'q')
                session.finish()
        assert not list(home.iterdir()), 'Menu navigation created local state'
        print('PASS keyboard dropdown navigation, contextual menu Help and consolidated hints at all sizes')

        # Capability detection uses only each child's environment; ASCII and color
        # are independent. These are VT fixture checks, not physical theme review.
        cases = [
            (dict(env, NO_COLOR="1"), False, True, (14, 48)),
            (dict(env, LC_ALL="C", LC_CTYPE="C.UTF-8"), True, False, (24, 80)),
            (dict(env, LC_CTYPE="POSIX"), True, False, (40, 140)),
            (dict(env, LC_ALL="", LC_CTYPE="C.UTF-8", LANG="C", NO_COLOR=""), False, False, (14, 48)),
            ({k: v for k, v in env.items() if k != "LANG"}, True, False, (24, 80)),
        ]
        for child_env, ascii_only, monochrome, size in cases:
            with Session([binary, "tui"], child_env, size) as session:
                session.wait(b"loaded.")
                session.navigate("Files", "Managed")
                session.wait(b"Files / Managed")
                session.send(b"o")
                session.wait(b"TARGET")
                session.send(b"~/.editor")
                session.wait(b"~/.editor")
                session.send(b"\tq")
                session.settle()
                session.wait(b"ID | Focus" if ascii_only else "ID · Focus".encode())
                assert session.screen.text().isascii() == ascii_only, "Unexpected decoration capability"
                assert session.data.isascii() == ascii_only, "Unexpected output encoding"
                assert has_color_sgr(session.data) != monochrome, "Unexpected color capability"
                session.send(b"\x03")
                session.finish()
                if monochrome:
                    assert not has_color_sgr(session.data), "Monochrome emitted explicit colors"
            assert not list(home.iterdir()), "Appearance/form navigation created state"
            print("PASS independent terminal capabilities and mouse/form input", ascii_only, monochrome, size)

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
            session.wait(b"Discovery loaded")
            session.send(b"/nvim\x1b")
            session.settle()
            session.send(b"\r")
            session.settle()  # Enter inspects a directory; adoption is a separate action.
            assert b"TARGET >" not in session.screen.text()
            session.wait(b"Directory / tree group")
            session.send(b"e")
            session.settle()
            assert b"TARGET >" not in session.screen.text()
            session.send(b"q")
            session.finish()
        assert not (discovery_home / ".config" / "dort").exists()
        assert not (discovery_home / ".local").exists()
        assert (discovery_home / ".config" / "nvim" / "init.lua").is_file()
        print("PASS ineligible discovered directory remains inspectable without adoption or state")

        # Only disposable hidden-home fixtures: nested plugin and agent settings,
        # automatic selected content, escaped controls and authentication exclusion.
        (discovery_home / '.pi' / 'agent').mkdir(parents=True)
        (discovery_home / '.pi' / 'agent' / 'settings.json').write_text('synthetic-agent-preview\n')
        (discovery_home / '.pi' / 'agent' / 'auth.json').write_text('private-auth-fixture\n')
        plugins = discovery_home / '.config' / 'nvim' / 'lua' / 'plugins'
        plugins.mkdir(parents=True)
        (plugins / 'editor.lua').write_text('synthetic-plugin-preview\n')
        with Session([binary, 'tui'], discovery_env, (32, 140)) as session:
            session.wait(b'loaded.')
            session.send(b'f')
            session.wait(b'Discovery loaded')
            session.send(b'/settings.json\rljlj')  # Open .pi and agent.
            session.click_label('[ ] settings.json', offset=5)
            session.wait(b'synthetic-agent-preview')
            assert b'private-auth-fixture' not in session.screen.text()
            session.destination("Git")
            session.settle()
            assert b'synthetic-agent-preview' not in session.screen.text()
            session.send(b'f')
            # The old status can survive a destination switch. Wait for the
            # refreshed list itself before typing into its filter.
            session.wait(b'[ ] settings.json')
            session.send(b'/\x15editor.lua\rljljljlj')  # Open the matching directory chain.
            session.click_label('[ ] editor.lua', offset=5)
            session.wait(b'synthetic-plugin-preview')
            session.send(b'q')
            session.finish()
        assert not (discovery_home / '.config' / 'dort').exists()
        assert not (discovery_home / '.local').exists()
        print('PASS bounded hidden-home discovery and automatic ephemeral selected-file preview')

        ignore_home = base / 'ignore-home'
        (ignore_home / '.editor' / 'sub').mkdir(parents=True)
        ignored_file = ignore_home / '.editor' / 'sub' / 'config'
        ignored_file.write_text('ordinary ignore fixture\n')
        ignore_env = dict(env, HOME=str(ignore_home),
                          XDG_CONFIG_HOME=str(ignore_home / '.config'),
                          XDG_STATE_HOME=str(ignore_home / '.local/state'))
        ignore_store = ignore_home / '.config' / 'dort' / 'discovery-ignore.toml'
        with Session([binary, 'tui'], ignore_env, (14, 48)) as session:
            session.wait(b'loaded.')
            session.send(b'f')
            session.wait(b'Discovery loaded')
            session.wait(b'Ignore selected')
            session.wait(b'F10')
            session.click_label('Ignore selected')
            session.wait(b'Ignore discovery path')
            assert not ignore_store.exists(), 'Ignore preview wrote preferences'
            session.send(b'\x1bOP')
            session.wait(b'Help: Review preview')
            session.send(b'yes\r')
            session.settle()
            assert not ignore_store.exists(), 'Help approved a preview'
            session.send(b'\x1b')
            session.wait(b'Ignore discovery path')
            session.send(b'\r')
            session.wait(b'Submit to execute')
            session.send(b'ye\x1bOP')
            session.wait(b'Help: Confirmation')
            session.send(b's\r')
            session.settle()
            assert not ignore_store.exists(), 'Help submitted confirmation input'
            session.send(b'\x1b')
            session.wait(b'Submit to execute')
            session.send(b's')  # retained ye becomes yes, without submitting
            session.send(b'\x1bOP')
            session.wait(b'Help: Confirmation')
            session.resize((13,47))
            session.settle()
            session.resize((14,48))
            session.wait(b'Help: Confirmation')
            session.send(b'\x1b')  # Help closes to preview; fresh confirmation required
            session.wait(b'Ignore discovery path')
            session.send(b'\r')
            session.wait(b'Submit to execute')
            session.send(b'yes\r')
            session.wait(b'Discovery loaded')
            session.send(b'q')
            session.finish()
        assert '.editor' in ignore_store.read_text()
        (ignore_home / '.editor' / 'later').write_text('later ordinary fixture\n')
        output = subprocess.run([binary, '--json', 'files', 'discover'], env=ignore_env,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
        assert not any('/.editor' in c['entry']['path'] for c in json.loads(output.stdout)['candidates'])
        with Session([binary, 'tui'], ignore_env, (14, 48)) as session:
            session.wait(b'loaded.')
            session.send(b'f')
            session.wait(b'Discovery loaded')
            session.navigate('Files', 'Ignored paths [U]')
            session.wait(b'Unignore')
            assert b'Enter' in session.screen.text().splitlines()[-1]
            session.send(b'\r')
            session.wait(b'Unignore discovery path')
            session.send(b'\r')
            session.wait(b'Confirm')
            session.send(b'yes\r')
            session.wait(b'Discovery loaded')
            session.send(b'q')
            session.finish()
        assert '.editor' not in ignore_store.read_text()
        assert ignored_file.read_text() == 'ordinary ignore fixture\n'
        print('PASS persistent confirmed ignore, CLI pruning and compact Unignore without live-file changes')

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
            session.wait(b"Declarations validated")
            session.wait(b"Checks | Standalone manifest")
            session.wait(b"Valid")
            session.wait(b"Declarations validated")
            session.send(b"\x1b[6~" * 6)
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
            session.wait(b"Declarations validated")
            session.send(b"q")
            session.finish()
        assert not (home / ".config").exists() and not (home / ".local").exists()
        print("PASS independent standalone check and nonpersistent profile additions")

        with Session([binary, "tui"], env, size=(14, 48)) as session:
            session.wait(b"loaded.")
            session.send(b"p")
            session.wait(b"REPOSITORY")
            session.wait(b"CHOICE")
            session.wait(b"The exact root becomes")
            session.send(b"\x1b[6~")
            session.settle()
            assert b"REPOSITORY" in session.screen.text(), "Paging replaced repository controls"
            session.send(b"\x1b[H")
            session.wait(b"REPOSITORY")
            session.send(b"\x1b[B\x1b[B")  # no roots: active, Create/attach
            session.wait(b"> [Create/attach")
            session.send(b"\r")
            session.wait("Path · Focus".encode())
            session.send(b"\x1b")
            session.settle()
            session.send(b"v")
            session.wait("Manifest path · Focus".encode())
            session.send(os.fsencode(standalone) + b"\r\t\r")
            session.wait(b"PROFILE")
            session.wait(b"Mandatory")
            session.wait(b"Temporary profiles | Never persisted")
            session.send(b"\x1b[6~")
            session.settle()
            assert b"PROFILE" in session.screen.text(), "Paging replaced profile controls"
            reached = False
            for _ in range(40):
                if b"Effective union" in session.screen.text():
                    reached = True
                    break
                session.send(b"\x1b[6~")
                session.settle()
            assert reached, "Compact picker contributors were not keyboard reachable"
            session.send(b"\x1b[H")
            session.wait(b"PROFILE")
            session.send(b"\x1b[B \x1b[B\r")  # dev toggle, real Apply button
            session.wait(b"Standalone profile additions validated")
            session.send(b"\x1b")
            session.settle()
            session.send(b"q")
            session.finish()
        assert not list(home.iterdir()), "Read-only compact pickers persisted state"
        print("PASS compact typed pickers, details/Home and session-only validation")

        with Session([binary, "tui"], env, size=(14, 48)) as session:
            session.wait(b"loaded.")
            session.send(b";")
            session.wait(b"Search by label")
            session.send(b"q?/")
            session.wait(b"No matching actions")
            session.send(b"\x15rollback\r")
            session.wait(b"No matching actions")  # other-view commands stay in their view
            session.send(b"\x1bOP")  # F1 works even in Actions search
            session.wait(b"Help: Actions")
            session.send(b"\r")  # Help never dispatches
            session.send(b"\x1b")
            session.wait(b"rollback")  # exact query retained
            session.send(b"\x1b")  # leave search
            session.settle()
            session.send(b"\x1b")  # close
            session.settle()
            session.destination("Recovery")
            session.send(b";rollback\r")  # disabled Enter in its relevant view
            session.wait(b"Rollback unavailable")
            session.wait(b"Action details")
            session.send(b"\x1b[6~")
            session.settle()
            assert b"SCOPE" in session.screen.text(), "Paging replaced Actions controls"
            reached = False
            for _ in range(30):
                if b"transaction." in session.screen.text(): # final wrapped reason row
                    reached = True
                    break
                session.send(b"\x1b[6~")
                session.settle()
            assert reached, "Compact Actions prerequisites were not keyboard reachable"
            session.send(b"\x1b[H")
            session.wait(b"SCOPE")
            session.send(b"\x1b")  # input -> results
            session.settle()
            session.send(b"\x1b")  # close, restoring Recovery
            session.settle()
            session.destination("Checks")
            session.send(b";")
            session.wait(b"Check current")  # current-view actions, not flat navigation first
            session.send(b"about\r")
            session.wait(b"No matching actions")  # session commands moved out of Actions
            session.send(b"\x1b")
            session.settle()
            session.send(b"\x1b")
            session.settle()
            session.navigate("Session", "About")
            session.wait(b"| About")
            session.send(b"\x1b")
            session.settle()
            session.send(b"q")
            session.finish()
        assert not list(home.iterdir()), "Browsing Actions or refusing an action created state"
        print("PASS compact search-first contextual Actions and full disabled reasons without dispatch")

        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.navigate("Checks", "Diagnostics [j]")
            session.wait(b"Diagnostics | Dort")
            session.wait(b"Registered repositories: 0")
            session.send(b"r")
            session.wait(b"Read-only Diagnostics loaded")
            session.settle()  # identical refresh result may still be the prior frame
            session.navigate("Session", "About")
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
            session.navigate("Checks", "Diagnostics [j]")
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
        with Session([binary, "tui"], env, (24, 110)) as session:
            session.wait(b"loaded.")
            session.navigate("Files", "Discover [f]")
            session.wait(b"Eligible hint")
            session.click_label(".q?config")  # selection only; content is automatic
            session.send(b"\r")  # retain inspection when shrinking to compact
            session.wait(b"private-body-marker")
            session.wait("Content · Tab focus".encode())
            session.mouse(65, 90, 10)  # detail viewport, not row selection
            session.settle()
            scrolled = session.screen.text()
            session.send(b"\t\x1b[Z")
            session.settle()
            assert session.screen.text() == scrolled, "Focus round trip changed content or viewport"
            session.send(b"\x1b[H")
            session.wait(b"[Eligible hint]")
            session.click_label("[Adopt selected")  # exact visible button, not explanatory prose
            session.wait("TARGET · Focus".encode())
            session.click_label("ID")
            session.send(b"\x15mouse-id")
            session.wait(b"mouse-id")
            session.send(b"\x1b")
            session.settle()
            session.send(b"\r")  # Explicit inspection, independent of earlier paging/focus
            session.resize((14, 48))
            session.wait(b"Content")
            session.send(b"\x1b")
            session.wait(b"NAME")
            session.send(b"\x1b[C")  # direct destination, never action selection
            session.wait(b"All repositories")
            session.navigate("Files", "Managed")
            session.wait(b"Files / Managed")  # destination chooses managed view
            session.resize((21, 110))
            session.wait(b"Selected item")
            session.resize((20, 110))
            session.wait(b"Files / Managed")
            assert b"Selected item" not in session.screen.text()
            assert b"private-body-marker" not in session.screen.text(), "Discover content survived leaving its scope"
            session.send(b"q")
            session.finish()
        assert not (home / ".config").exists() and not (home / ".local").exists()
        assert candidate.read_text() == "private-body-marker\n"
        print("PASS direct Files keys/mouse, retained viewport and width/height layouts without writes")

        focus_home = base / 'content-focus-home'
        focus_home.mkdir(mode=0o700)
        (focus_home / '.focus-a').write_text('focus-start-marker\n' + 'ordinary wrapped setting ' * 1200 + '\nfocus-end-marker\n')
        (focus_home / '.focus-b').write_text('second-file-marker\n')
        focus_env = dict(env, HOME=str(focus_home), XDG_CONFIG_HOME=str(focus_home / '.config'),
                         XDG_STATE_HOME=str(focus_home / '.local/state'))
        with Session([binary, 'tui'], focus_env, (40, 140)) as session:
            session.wait(b'loaded.')
            session.send(b'f')
            session.wait(b'focus-start-marker')
            session.send(b'\t')
            session.settle()
            session.send(b'j\x06')  # Content row + full rendered page, not file selection.
            session.settle()
            assert b'second-file-marker' not in session.screen.text()
            session.send(b'G')
            session.wait(b'focus-end-marker')
            session.send(b'gg')
            session.wait(b'focus-start-marker')
            session.resize((14, 48))
            session.wait(b'Content')
            session.send(b'G')
            session.wait(b'focus-end-marker')
            session.send(b'\x1b')
            session.wait(b'NAME')
            session.send(b'j\t')  # List selection, then compact content focus.
            session.wait(b'second-file-marker')
            session.send(b'\x1bq')
            session.finish()
        assert not (focus_home / '.config').exists() and not (focus_home / '.local').exists()
        print('PASS Discover content focus, wrapped Vim pages/end, compact resize and separate file cursor without writes')

        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.send(b"f")
            session.wait(b"Eligible hint")
            session.send(b"\r")
            session.wait(b"Content")
            session.send(b"e")  # direct, separate Adopt selected action
            session.wait("TARGET · Focus".encode())
            session.send(b"\t\t\t\r")  # fields -> Review, no repository selected
            session.wait(b"Read failed:")
            session.click_label("[Retry read")  # Fresh read/preview intent, never execution.
            session.settle()
            session.wait(b"Read failed:")
            session.click_label("[Back")
            session.settle()
            session.wait(b".q?config")
            assert b"private-body-marker" not in session.screen.text(), "Discover content entered retained adoption/error UI"
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
            session.navigate("Git", "Choose Git scope [g]")
            session.send(os.fsencode(git_only) + b"\r")
            session.wait(b"Git read completed")
            session.wait(b"q?name")
            session.send(b"/q?/\x1b")  # literal text, no quit/help/actions
            session.wait(b"0 displayed")
            session.send(b"\x1b[3~u")  # clear display filter; unstaged summary
            session.wait(b"Unstaged (worktree vs index)")
            session.wait(b"1 entries")
            session.send(b";View patch content")
            session.wait(b"Search by label")
            session.wait(b"Action details")
            session.send(b"\x1b[6~")
            session.settle()
            assert b"SCOPE" in session.screen.text(), "Paging replaced Actions controls"
            session.send(b"\x1b[H")
            session.wait(b"SCOPE")
            session.send(b"\x1b")
            session.settle()
            session.send(b"\x1b")
            session.wait(b"1 entries")
            assert not (base / "patch-calls").exists(), "Summary/Actions browsing prefetched a patch"
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

        patch_calls = (base / "patch-calls").read_text()
        with Session([binary, "tui"], dict(env, PATH=str(bin_dir)), (14, 48)) as session:
            session.wait(b"loaded.")
            session.navigate("Git", "Choose Git scope [g]")
            session.send(os.fsencode(git_only) + b"\r")
            session.wait(b"Git read completed")
            for column in [b"PATH", b"KIND", b"XY"]:
                session.wait(column)
            session.wait(b"> q?name")
            session.send(b"\r")
            session.wait(b"Git details")
            session.wait(b"Path:")
            session.send(b"\x1b[6~" * 2)
            session.wait(b"Git-only root:")
            session.send(b"\x1b[H")
            session.wait(b"Path:")
            session.send(b"\x1b")
            session.wait(b"PATH")
            session.send(b"u")
            for column in [b"PATH", b"CHANGE", b"MODES"]:
                session.wait(column)
            session.wait(b"100644 -> 100644")
            session.send(b"/q?/\r")
            session.wait(b"0 displayed")
            session.wait(b"No matches")
            session.send(b"\x1b[3~r")
            session.settle()  # identical summaries can otherwise match the prior frame
            session.wait(b"1 entries")
            session.wait(b"Git read completed")
            session.send(b"\r")
            session.wait(b"Modes: 100644 -> 100644")
            assert b"PRIVATE-PATCH" not in session.data, "Metadata rendered patch content"
            session.send(b"q")
            session.finish()
        assert (base / "patch-calls").read_text() == patch_calls, "Metadata inspection fetched a patch"
        assert not list(home.iterdir()) and not list(git_only.iterdir())
        print("PASS compact typed Git tables, full-scope details and Home without patch reads or state")

        with Session([binary, "tui"], dict(env, PATH=str(bin_dir)), (24, 110)) as session:
            session.wait(b"loaded.")
            session.navigate("Git", "Choose Git scope [g]")
            session.send(os.fsencode(git_only) + b"\r")
            session.wait(b"Git read completed")
            session.click_label("q?name")
            session.send(b"\r\x1b[6~")
            session.wait(b"Git details")
            session.mouse(65, 90, 10)
            session.settle()
            scrolled = session.screen.text()
            session.send(b"\t\x1b[Z")
            session.settle()
            assert scrolled == session.screen.text(), "Tab changed Git metadata scroll"
            session.send(b"\x1b[H")
            session.wait(b'Path: "q?name"')
            session.resize((21, 110))
            session.wait(b"Git details | Esc list")
            session.send(b"\x1b")
            session.wait(b"PATH")
            session.resize((22, 110))
            session.wait(b"PATH")
            session.wait(b"Git details")
            session.navigate("Git", "Unstaged [u]")
            session.wait(b"Unstaged (worktree vs index)")
            session.wait(b"100644 -> 100644")  # loaded diff before content action
            session.click_label("View patch")
            session.wait(b"May expose secrets")
            session.mouse(0, 20, 8)  # ordinary clicks cannot submit the phrase
            session.send(b"yes\r")
            session.settle()
            assert b"May expose secrets" in session.screen.text()
            assert b"PRIVATE-PATCH" not in session.data
            session.send(b"\x1b")
            session.settle()
            session.send(b"q")
            session.finish()
        assert (base / "patch-calls").read_text() == patch_calls, "Wide metadata controls disclosed content"
        assert not list(home.iterdir()) and not list(git_only.iterdir())
        print("PASS direct Git metadata keys/mouse and resize isolated from patch confirmation")

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
                session.wait("Review ·".encode())
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
            session.wait("Review ·".encode())
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
            session.wait("Review ·".encode())
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
            session.wait("Review ·".encode())
            session.send(b"\r")
            session.wait(b"Confirm")
            session.send(b"yes\r")
            deadline = time.monotonic() + TIMEOUT
            while not target.is_symlink() and time.monotonic() < deadline:
                session.pump()
            assert target.is_symlink(), "Approved TUI apply did not publish the link"
            session.wait(b"Operation result")
            session.wait(b"File operation completed.")
            session.send(b"q")
            session.finish()
        assert target.resolve() == repo / "source"
        assert (repo / "source").read_text() == "ordinary settings\n"
        print("PASS approved apply, persistent result and clean exit")

        retained = {str(path.relative_to(state)): hashlib.sha256(path.read_bytes()).digest()
                    for path in state.rglob("*.json")}
        with Session([binary, "tui"], env, (14, 48)) as session:
            session.wait(b"loaded.")
            session.send(b"a")
            session.wait("Review ·".encode())
            session.send(b"\r")
            session.wait(b"Confirm")
            session.send(b"yes\r")
            session.wait(b"Operation result")
            session.wait(b"Everything is up to date.")
            session.click_label("[Refresh")
            session.settle()
            session.wait(b"Everything is up to date.")
            session.click_label("[Back")
            session.settle()
            assert b"Operation result" not in session.screen.text()
            session.send(b"q")
            session.finish()
        assert retained == {str(path.relative_to(state)): hashlib.sha256(path.read_bytes()).digest()
                            for path in state.rglob("*.json")}, "No-op/receipt refresh changed history"
        assert target.is_symlink() and (repo / "source").read_text() == "ordinary settings\n"
        print("PASS compact no-op receipt, read-only refresh and clicked Back without a journal")

        with (state / "dort/lock").open("r+b") as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with Session([binary, "tui"], env, (14, 48)) as session:
                session.wait(b"loaded.")
                session.send(b"a")
                session.wait("Review ·".encode())
                session.send(b"\r")
                session.wait(b"Confirm")
                session.send(b"yes\r")
                session.wait(b"Possible partial effects")
                session.send(b"\x1b[6~" * 4)
                session.settle()
                assert b"Possible partial effects" in session.screen.text(), "Risk scrolled out of view"
                assert b"Retry execution" not in session.screen.text()
                session.click_label("[Refresh")  # Must work while the writer lock is held.
                session.wait(b"Earlier write failed:")
                session.destination("Recovery")
                session.wait(b"Blockers: 0")
                session.wait(b"Earlier write failed:")
                session.send(b"q")
                session.finish()
        assert retained == {str(path.relative_to(state)): hashlib.sha256(path.read_bytes()).digest()
                            for path in state.rglob("*.json")}, "Failed-write feedback replayed execution"
        assert target.is_symlink() and (repo / "source").read_text() == "ordinary settings\n"
        print("PASS compact failed-write risk, safe refresh and Recovery while the writer lock is held")

        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.destination("Recovery")
            session.send(b"/q?/\x1b")
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
            # Return from the retained receipt before opening navigation menus.
            session.click_label("[Back")
            # A new action needs a new actual preview and confirmation.
            session.destination("Overview")
            session.send(b"a")
            session.wait("Review ·".encode())
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
            session.destination("Recovery")
            session.send(b"/Completed\r")
            session.wait(b"Blockers: 0")
            session.wait(b"Showing 1")
            session.wait(b"Completed")
            session.send(b"\r")  # selected row inspects; never requests rollback
            session.wait(b"Transaction details")
            session.wait(b"Repository ID:")
            session.send(b"\x1b[6~" * 4)
            session.wait(b"Validated checkpoints:")
            session.send(b"\x1b[H")
            session.wait(b"Repository ID:")
            session.click_label("Review rollback")  # same typed action as b; still preview only
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
        print("PASS compact typed Recovery inspection, clicked rollback preview and dry-run without writes")

        with Session([binary, "--dry-run", "tui"], env, (24, 110)) as session:
            session.wait(b"loaded.")
            session.navigate("Recovery", "All repositories")
            session.send(b"/Completed\r")
            session.wait(b"Showing 1")
            session.click_label("Completed", min_row=6)  # phase cell, below the retained filter
            session.send(b"\r")
            session.wait(b"Transaction details")
            session.mouse(65, 90, 10)
            session.settle()
            scrolled = session.screen.text()
            session.send(b"\t\x1b[Z")
            session.settle()
            assert scrolled == session.screen.text(), "Tab changed Recovery detail scroll"
            session.send(b"\x1b[H")
            session.wait(b"Transaction identity")
            session.resize((14, 48))
            session.wait(b"Transaction details")
            session.send(b"\x1b")
            session.wait(b"TX ID")
            session.click_label("Completed", min_row=6)
            session.send(b"\r")
            session.wait(b"Transaction details")
            session.send(b"\x1b")
            session.wait(b"TX ID")
            session.send(b"q")
            session.finish()
        assert checkpoints == {str(path.relative_to(state)): hashlib.sha256(path.read_bytes()).digest()
                               for path in state.rglob("*.json")}, "Wide Recovery browsing changed history"
        assert target.is_symlink() and (repo / "source").read_text() == "ordinary settings\n"
        print("PASS direct Recovery mouse/keys, Home and compact inspect/back without writes")

        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.destination("Files")
            session.send(b"/q?/\x1b")
            session.settle()
            session.wait(b"No matches")
            session.send(b"a")
            session.wait("Review ·".encode())
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
            session.wait("Review ·".encode())
            session.send(b"\x1b")
            session.settle()
            assert not candidate.is_symlink() and not (repo / "files/config").exists()
            session.send(b"\r")  # retained Review -> fresh preview
            session.settle()
            session.send(b"\r")  # Adopt all: one explicit approval, no typed yes.
            session.wait(b"Discover refreshed.")
            session.send(b"q")
            session.finish()
        assert candidate.is_symlink() and candidate.resolve() == repo / "files/config"
        assert (repo / "files/config").read_text() == "ordinary adoption settings\n"
        print("PASS manual adoption retained draft, preview/back and explicit approval")

        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.destination("Files")
            session.send(b"/notconfig\x1b")
            session.settle()
            session.send(b";delete managed link: enter\r")
            session.wait("Exact ID · Focus".encode())
            session.send(b"config\r\r")
            session.wait("Review ·".encode())
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

        # Each batch scenario has its own canonical disposable home and registry.
        def batch_fixture(name, managed):
            batch_home, batch_repo = base / (name + '-home'), base / (name + '-repo')
            batch_home.mkdir(mode=0o700)
            (batch_repo / '.git').mkdir(parents=True)
            targets = batch_home / '.config/bundle'
            targets.mkdir(parents=True, mode=0o700)
            manifest = 'schema=1\n'
            for item in ['only-a', 'only-b'] + (['only-c'] if managed else []):
                (targets / item).write_text('ordinary settings\n')
                if managed:
                    (batch_repo / item).write_text('ordinary source settings\n')
                    manifest += (f"[[files]]\nid='{item}'\nsource='{item}'\n"
                                 f"target='~/.config/bundle/{item}'\nmode='link'\n")
            (batch_repo / 'dort.toml').write_text(manifest)
            configuration = batch_home / '.config/dort'
            configuration.mkdir(mode=0o700)
            repo_id = hashlib.sha256(os.fsencode(batch_repo)).hexdigest()
            registration = configuration / 'local.toml'
            registration.write_text(f'schema=1\nactive_repository="{repo_id}"\n'
                                    f'[repositories."{repo_id}"]\nroot={json.dumps(str(batch_repo))}\n')
            registration.chmod(0o600)
            batch_env = dict(env, HOME=str(batch_home), XDG_CONFIG_HOME=str(batch_home / '.config'),
                             XDG_STATE_HOME=str(batch_home / '.local/state'))
            return batch_home, batch_repo, targets, batch_env

        def batch_heads(batch_home, expected):
            folders = list((batch_home / '.local/state/dort/repositories').glob('*/transactions/*'))
            assert len(folders) == expected, 'Batch did not use one transaction per approval'
            for folder in folders:
                head = json.loads(json.loads(sorted(folder.glob('*.json'))[-1].read_text())['payload'])
                assert head['schema'] == 4 and head['phase'] in ('completed', 'rolled_back')

        batch_home, batch_repo, targets, batch_env = batch_fixture('batch-adopt', False)
        with Session([binary, 'tui'], batch_env) as session:
            session.wait(b'loaded.')
            session.send(b'f')
            session.wait(b'Discovery loaded')
            session.send(b'/bundle\rlljlj')  # Open .config and bundle, then select a leaf.
            session.click_label('[ ] only-a')
            session.click_label('[ ] only-b')
            session.wait(b'2 marked')
            session.send(b'h')  # Folding does not remove either mark.
            session.wait(b'2 hidden')
            session.send(b'lE')
            session.wait(b'All editable defaults')
            session.wait(b'Review all')
            session.settle()
            assert b'Files / Discover' not in session.screen.text(), 'Batch editor leaked Discover underneath'
            session.send(b'\x1b[6~')
            session.wait(b'files/.config/bundle/only-a')
            session.send(b'\x1b[H')
            session.send(b'je')  # Select only-b and open a separate item form.
            session.wait(b'Edit item')
            session.send(b'\r\x15edited-b\r\r\r')  # TARGET -> ID -> SOURCE -> Done -> batch.
            session.wait(b'Review all')
            session.send(b'e\r')
            session.wait(b'edited-b')
            session.click_label('Done')  # Return only; never review/adopt an individual item.
            session.wait(b'Review all')
            session.click_label('Review all')
            session.wait(b'Combined batch preview')
            session.send(b'\r')  # Adopt all consumes the real combined preview once.
            session.wait(b'Discover refreshed.')
            assert all((targets / name).is_symlink() for name in ('only-a', 'only-b'))
            for name in ('only-a', 'only-b'):
                assert (targets / name).resolve() == batch_repo / 'files/.config/bundle' / name
            batch_heads(batch_home, 1)
            session.destination("Recovery")
            session.send(b'b')
            session.wait(b'Rollback schema 4')
            session.send(b'\r')
            session.wait(b'Confirm')
            session.send(b'yes\r')
            session.wait(b'Observations refreshed.')
            session.send(b'q')
            session.finish()
        for name in ('only-a', 'only-b'):
            assert not (targets / name).is_symlink()
            assert (targets / name).read_text() == 'ordinary settings\n'
            assert not (batch_repo / 'files/.config/bundle' / name).exists()
        batch_heads(batch_home, 1)
        print('PASS tree marks/folding, editable combined adoption and schema-4 whole rollback')

        for operation in ('K', 'D'):
            batch_home, batch_repo, targets, batch_env = batch_fixture('batch-' + operation, True)
            with Session([binary, 'tui'], batch_env) as session:
                session.wait(b'loaded.')
                session.destination("Files")
                session.send(b'ljlj')  # Folders start closed.
                session.click_label('[ ] only-a')
                session.click_label('[ ] only-b')
                session.send(b'/only-a\rA')  # Hidden only-b remains explicitly included.
                session.wait(b'Combined batch preview')
                session.send(b'\r')
                session.wait(b'Confirm')
                session.send(b'yes\r')
                session.wait(b'Observations refreshed.')
                assert all((targets / name).is_symlink() for name in ('only-a', 'only-b'))
                assert not (targets / 'only-c').is_symlink()
                batch_heads(batch_home, 1)
                session.send(b'\x1b')
                session.settle()
                session.send(operation.encode())
                session.wait(b'Combined batch preview')
                session.send(b'\r')
                if operation == 'D':
                    session.wait(b'1/2')
                    session.send(b'yes\r')
                    session.settle()
                    assert all((targets / name).is_symlink() for name in ('only-a', 'only-b'))
                    session.send(b'\x7f\x7f\x7fonly-a\r')
                    session.wait(b'2/2')
                    session.resize((14, 47))
                    session.wait(b'terminal too small')
                    session.resize((24, 80))
                    session.wait(b'Combined batch preview')
                    session.settle()  # Drain resize events before fresh confirmation input.
                    session.send(b'\r')
                    session.wait(b'1/2')  # The entire acknowledgement sequence was cleared.
                    session.send(b'only-a\ronly-b\r')
                    session.wait(b'literal yes')
                else:
                    session.wait(b'Confirm')
                session.send(b'yes\r')
                session.wait(b'Batch file operation completed.')
                session.send(b'q')
                session.finish()
            for name in ('only-a', 'only-b'):
                assert not (targets / name).is_symlink()
                if operation == 'K':
                    assert (targets / name).read_text() == 'ordinary source settings\n'
                else:
                    assert not (targets / name).exists()
                assert (batch_repo / name).read_text() == 'ordinary source settings\n'
            assert (targets / 'only-c').read_text() == 'ordinary settings\n'
            remaining = (batch_repo / 'dort.toml').read_text()
            assert 'only-a' not in remaining and 'only-b' not in remaining and 'only-c' in remaining
            batch_heads(batch_home, 2)
            print('PASS selected Apply and combined', operation, 'with hidden marks and guarded approval')

        batch_home, batch_repo, targets, batch_env = batch_fixture('sequential-keep', True)
        with Session([binary, 'tui'], batch_env) as session:
            session.wait(b'loaded.')
            session.destination('Files')
            session.send(b'a')
            session.wait('Review ·'.encode())
            session.send(b'\r')
            session.wait(b'Confirm')
            session.send(b'yes\r')
            session.wait(b'Observations refreshed.')
            session.send(b'\x1b')
            session.settle()
            session.send(b'ljlj')
            session.click_label('only-a')
            session.send(b' u')  # Mark A, then Keep selected with the advertised key.
            session.wait('Review ·'.encode())
            session.send(b'\r')
            session.wait(b'Confirm')
            session.send(b'yes\r')
            session.wait(b'Selected item disappeared')  # successful refresh removed the old mapping
            session.send(b'\x1b')
            session.wait(b'0 marked (0 hidden)')
            session.click_label('only-b')
            session.send(b' K')
            session.wait(b'Combined batch preview')
            assert b'only-a' not in session.screen.text(), 'Completed Keep leaked into a later batch'
            session.send(b'\r')
            session.wait(b'Confirm')
            session.send(b'yes\r')
            session.wait(b'Selected item disappeared')
            session.send(b'\x1b')
            session.wait(b'0 marked (0 hidden)')
            session.send(b'q')
            session.finish()
        for name in ('only-a', 'only-b'):
            assert not (targets / name).is_symlink()
            assert (targets / name).read_text() == 'ordinary source settings\n'
            assert (batch_repo / name).is_file()
        assert (targets / 'only-c').is_symlink()
        remaining = (batch_repo / 'dort.toml').read_text()
        assert 'only-a' not in remaining and 'only-b' not in remaining and 'only-c' in remaining
        assert len(list((batch_home / '.local/state/dort/repositories').glob('*/transactions/*'))) == 3
        print('PASS selected u Keep and sequential marked Keep without stale hidden marks')

        for mode, code in [("normal", 0), ("error", 0), ("panic", 101)]:
            with Session(
                [probe, "--exact", "tui::tests::terminal_cleanup_probe", "--nocapture"],
                dict(env, DORT_TEST_TERMINAL_CLEANUP=mode),
            ) as session:
                session.finish(code)
            print("PASS terminal cleanup", mode)
    print("All PTY cases passed (disposable HOME, no installed Git requirement)")


if __name__ == "__main__":
    run()
