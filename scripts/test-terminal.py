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

from terminal_tools import tools_workflows, interaction_surface_workflows

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
        self.cursor_visible = False
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
                    if arguments == '?25' and command in 'hl':
                        self.cursor_visible = command == 'h'
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
        assert marker in self.screen.text(), f"Expected {marker!r} was not rendered: {self.screen.text()!r}"

    def settle(self):
        deadline = time.monotonic() + 0.2
        while time.monotonic() < deadline:
            self.pump()

    def press(self, key, marker):
        """Press a key until `marker` renders.

        The app refuses keys while owned work completes and paints an Action
        unavailable notice, which the next accepted key clears. A fixed settle
        is not enough on a slow runner, so resend while the notice is up.
        """
        deadline = time.monotonic() + TIMEOUT
        sent = False
        while time.monotonic() < deadline:
            if marker in self.screen.text():
                return
            if sent and b'Action unavailable' not in self.screen.text():
                break
            self.send(key)
            sent = True
            window = time.monotonic() + 0.5
            while time.monotonic() < window:
                self.pump()
                if marker in self.screen.text():
                    return
        self.wait(marker)

    def send(self, value):
        if value == b'\x1b':
            self.escape()
            return
        os.write(self.master, value)

    def escape(self):
        """Send a lone Esc and wait for the child to act on it.

        Esc is a prefix byte. A key that lands in the same read window arrives
        as Alt-key and is dropped, which cost 20s timeouts on slower runners.
        """
        frames = len(self.data)
        os.write(self.master, b'\x1b')
        deadline = time.monotonic() + 2.0
        while time.monotonic() < deadline:
            self.pump()
            if self.code is not None:
                return
            if len(self.data) > frames:
                self.settle()
                return

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

    def edit_fields(self, label):
        self.wait(label.encode())
        self.settle()
        self.send(b'\t')  # Forms start outside editing.
        self.focused_field(label)

    def focused_field(self, label):
        self.wait(label.encode())
        self.settle()
        assert self.screen.cursor_visible, 'Text field lost its foreground caret'
        assert self.screen.row > 0
        previous = ''.join(self.screen.cells[self.screen.row - 1])
        assert label in previous, 'Caret does not belong to the expected field: ' + label

    def click_action(self, label):
        # The single foreground strip owns actions; body prose is never a hit.
        self.click_label(label, min_row=len(self.screen.cells) - 1)

    def ready_review(self, action=b'y Confirm'):
        self.settle()
        if b'End Read' in self.screen.text().splitlines()[-1]:
            self.send(b'\x1b[F')
        self.wait(action)
        assert action in self.screen.text().splitlines()[-1]

    def navigate(self, group, item):
        self.settle()
        if self.screen.cursor_visible:
            self.send(b'\x1b')  # Leave field editing; the draft stays.
            self.settle()
        label = item.split(' [')[0]
        if group == 'Session':
            self.more_action('About Dort' if label == 'About' else label)
            return
        sections = {'Overview': b'o', 'Files': b'f', 'Tools': b't',
                    'Recovery': b'r', 'Checks': b'c', 'Git': b'g'}
        pages = {
            'Files': {'Managed': b'1', 'Discover': b'2', 'Ignored paths': b'3'},
            'Checks': {'Manifest checks': b'1', 'Diagnostics': b'2'},
            'Git': {'Status': b'1', 'Unstaged': b'2', 'Staged': b'3'},
            'Tools': {'Declared tools': b'1', 'Installed packages': b'2',
                      'Installation history': b'3'},
        }
        self.send(sections[group])  # Section keys open the section's first page.
        self.settle()
        if group == 'Git' and label == 'Choose Git scope':
            self.send(b'P')  # Advertised Git scope key; opens its own root draft.
            self.settle()
            return
        page = pages.get(group, {}).get(label)
        if page:
            self.send(page)
        self.settle()

    def destination(self, group):
        items = {"Overview": "Overview", "Files": "Managed", "Tools": "Declared tools",
                 "Recovery": "All repositories", "Checks": "Manifest checks [m]"}
        if group == "Git":
            self.navigate("Git", "Choose Git scope [g]")
            self.send(b"\x1b")
            self.settle()
        else:
            self.navigate(group, items[group])

    def more_action(self, label):
        self.settle()  # Let an automatic read refresh and its foreground frame complete.
        footer = self.screen.text().decode().splitlines()[-1]
        if label in footer:
            self.click_action(label)
            return
        self.send(b'\x1bOQ')  # F2 always opens More.
        self.wait(b'More actions')
        self.send(b'\x1b[H')
        for _ in range(40):
            self.settle()
            selected = [line for line in self.screen.text().decode().splitlines()
                        if '› ' + label in line or '> ' + label in line]
            if selected:
                self.send(b'\r')
                return
            self.send(b'\x1b[B')
        raise AssertionError('More action was not keyboard reachable: ' + label)

    def closure_provider(self, number):
        self.send(b'\x1bOS')  # F4 opens an explicit choice, never a provider cycle.
        self.wait(b'Choose package manager')
        self.send(b'\x1b[H' + b'\x1b[B' * number + b'\r')
        self.settle()

    def resize(self, size):
        self.settle()
        visible = self.screen.cursor_visible
        self.screen = Screen(size)
        self.screen.cursor_visible = visible
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", *size, 0, 0))
        os.killpg(self.process.pid, signal.SIGWINCH)

    def finish(self, expected=0, tui=True):
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
        if tui:
            assert b"\x1b[?1049l" in self.data, "Alternate screen was not left"
        for mode in ((1000, 1002, 1003, 1015, 1006) if tui else ()):
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


def guarded_effects(binary):
    with tempfile.TemporaryDirectory(prefix='dort-effects-') as temporary:
        base = Path(temporary).resolve()
        for mode in ('cancel', 'mouse', 'key', 'dry-run', 'blocked'):
            home, repo = base / mode / 'h', base / mode / 'r'
            home.mkdir(parents=True, mode=0o700)
            (repo / '.git').mkdir(parents=True)
            manifest = 'schema=1\n'
            sources = [f'source-{index:02}' for index in range(12)] + ['tail']
            for source in sources:
                manifest += (f"[[files]]\nid='{source}'\nsource='{source}'\n"
                             f"target='~/.{source}'\nmode='link'\n")
                if not (mode == 'blocked' and source == 'tail'):
                    (repo / source).write_text('disposable settings\n')
            (repo / 'dort.toml').write_text(manifest)
            identity = hashlib.sha256(os.fsencode(repo)).hexdigest()
            config = home / '.config/dort'
            config.mkdir(parents=True, mode=0o700)
            local = config / 'local.toml'
            local.write_text(f'schema=1\nactive_repository="{identity}"\n'
                             f'[repositories."{identity}"]\nroot={json.dumps(str(repo))}\n')
            local.chmod(0o600)
            state = home / '.local/state/dort/repositories'
            env = {'HOME': str(home), 'XDG_CONFIG_HOME': str(home / '.config'),
                   'XDG_STATE_HOME': str(home / '.local/state'), 'PATH': '/no-external-programs',
                   'TERM': 'xterm-256color', 'LANG': 'C.UTF-8'}
            args = [binary, '--dry-run', 'tui'] if mode == 'dry-run' else [binary, 'tui']
            tail = b'tail" }'
            with Session(args, env, (14, 48)) as session:
                session.wait(b'loaded.')
                session.press(b'a', b'Review effects before confirming')
                session.send(b'yy')
                session.settle()
                footer = session.screen.text().splitlines()[-1]
                assert b'y Confirm' not in footer, footer
                if mode in ('dry-run', 'blocked'):
                    assert b'End Read' not in footer, footer
                else:
                    assert b'End Read' in footer, footer
                assert tail not in session.screen.text()
                assert not state.exists()
                session.send(b'y')
                session.settle()
                assert not state.exists()
                if mode in ('dry-run', 'blocked'):
                    session.send(b'\x1b[Fy')
                    session.settle()
                    assert b'y Confirm' not in session.screen.text().splitlines()[-1]
                else:
                    session.wait(b'End Read')
                    session.click_action('Read')
                    session.wait(tail)
                    session.wait(b'y Confirm')
                    session.wait(b'F1 Help')
                    footer = session.screen.text().splitlines()[-1]
                    assert all(key in footer for key in (b'y Confirm', b'Esc Back', b'F1 Help')), footer
                    row = 13
                    column = footer.index(b'y Confirm')
                    session.send(b'\x1b[Hy')
                    session.wait(b'End Read')
                    session.mouse(0, column, row)
                    session.settle()
                    assert not state.exists()
                    session.ready_review()
                    if mode == 'mouse':
                        session.click_action('Confirm')
                    elif mode == 'key':
                        session.send(b'yy')
                    else:
                        session.send(b'\x1b')
                        session.send(b'y')
                    if mode in ('mouse', 'key'):
                        session.wait(b'File operation completed.')
                        session.send(b'yy')
                session.send(b'q')
                session.finish()
            published = mode in ('mouse', 'key')
            if published:
                assert all((home / ('.' + source)).resolve() == repo / source for source in sources)
            else:
                assert all(not (home / ('.' + source)).exists() for source in sources)
            count = len(list(state.glob('*/transactions/*')))
            assert count == int(published), (mode, count)
            print('PASS guarded effects', mode, flush=True)


def run():
    screen = Screen((2, 20))
    screen.feed(b"Git executes\x1b[")
    screen.feed(b"1;11Hable\x1b[2;1Hlabel")
    assert b"Git executable" in screen.text()
    assert b"label" in screen.text()
    binary = artifact(["build", "-p", "dort"], "binary")
    probe = artifact(["test", "-p", "dort", "--bin", "dort", "--no-run"], "test")
    guarded_effects(binary)
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

        tools_workflows(Session, binary, base, env)
        interaction_surface_workflows(Session, binary, base, env)

        # Text carries navigation/state meaning even with a low-color terminal.
        with Session([binary, "--dry-run", "tui"], dict(env, TERM="vt100", NO_COLOR="1"), (14, 48)) as session:
            session.wait(b"loaded.")
            for label in [b"Overview", b"DRY RUN", b"Create/attach", b"l Clone", b"F2 More", b"F1 Help"]:
                session.wait(label)
            session.send(b"?")
            session.wait(b"Start with a repository")
            session.send(b"\x1b[6~" * 30)
            session.wait(b"Use a repository you already have")
            assert b"Start with a repository" not in session.screen.text()
            session.send(b"\x1b[H")
            session.wait(b"Start with a repository")
            session.send(b"\x1b")
            session.wait(b"Create/attach")
            session.send(b"q")
            session.finish()
            assert session.screen.text().isascii() and session.data.isascii()
            assert not has_color_sgr(session.data), "Monochrome emitted explicit colors"
        assert not list(home.iterdir()), "Compact navigation/help created state"
        print("PASS compact low-color navigation and Home help reset without state")

        for nav_size in [(14, 48), (24, 80), (40, 140)]:
            with Session([binary, 'tui'], env, nav_size) as session:
                session.wait(b'loaded.')
                session.send(b'f')
                session.wait(b'Files / Managed')
                session.send(b'2')
                session.wait(b'Files / Discover')
                session.send(b'1')
                session.wait(b'Files / Managed')
                session.send(b'9')  # No ninth local destination exists in Files.
                session.settle()
                assert b'Files / Managed' in session.screen.text(), 'An unassigned local number changed destination'
                session.send(b't')
                session.wait(b'Tools / Declared tools')
                session.send(b'c')
                session.wait(b'Checks / Manifest')
                session.send(b'2')
                session.wait(b'Checks / Diagnostics')
                session.send(b'o')
                session.wait(b'Dort / Overview')
                session.send(b'q')
                session.finish()
        assert not list(home.iterdir()), 'Section navigation created local state'
        print('PASS section keys and numbered local destinations at all sizes')

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
                session.send(b"m")
                session.wait(b"TARGET")
                session.send(b"\t~/.editor")
                session.wait(b"~/.editor")
                session.send(b"\tq")
                session.settle()
                session.focused_field('ID')
                session.send(b'\x1bOP')  # Help opens without disturbing the retained draft.
                session.wait(b'Adopt a file')
                session.wait(b'Close Help')
                session.settle()
                assert not session.screen.cursor_visible, 'Help retained the background field caret'
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
            session.send(b"F")
            session.wait(b"Filter files")
            session.send(b"/nvim\x1b")
            session.settle()
            session.send(b"\r")
            session.settle()  # Enter inspects a directory; adoption is a separate action.
            assert b"TARGET >" not in session.screen.text()
            session.wait("Folder · header only".encode())
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
            session.send(b'F')
            session.wait(b'Filter files')
            session.send(b'/settings.json\rljlj')  # Open .pi and agent.
            session.click_label('[ ] settings.json', offset=5)
            session.wait(b'synthetic-agent-preview')
            assert b'private-auth-fixture' not in session.screen.text()
            session.destination("Git")
            session.settle()
            assert b'synthetic-agent-preview' not in session.screen.text()
            session.send(b'F')
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
            session.send(b'F')
            session.wait(b'Filter files')
            session.more_action('Ignore')
            session.wait(b'Ignore discovery path')
            assert not ignore_store.exists(), 'Ignore preview wrote preferences'
            session.send(b'\x1bOP')
            session.wait(b'Review proposed changes')
            session.send(b'\r')
            session.settle()
            assert not ignore_store.exists(), 'Help approved a preview'
            session.send(b'\x1b')
            session.wait(b'Ignore discovery path')
            session.send(b'\r\r\x1bOP')
            session.wait(b'Review proposed changes')
            session.send(b'y\r')
            session.settle()
            assert not ignore_store.exists(), 'Help submitted confirmation input'
            session.send(b'\x1b')
            session.wait(b'Ignore discovery path')
            session.send(b'Y')  # Only lowercase y can approve.
            session.send(b'\x1bOP')
            session.wait(b'Review proposed changes')
            session.resize((13,47))
            session.settle()
            session.resize((14,48))
            session.wait(b'Review proposed changes')
            session.send(b'\x1b')  # Help closes to preview; fresh confirmation required
            session.wait(b'Ignore discovery path')
            session.ready_review()
            session.send(b'y')
            session.wait(b'Filter files')
            session.send(b'q')
            session.finish()
        assert '.editor' in ignore_store.read_text()
        (ignore_home / '.editor' / 'later').write_text('later ordinary fixture\n')
        output = subprocess.run([binary, '--json', 'files', 'discover'], env=ignore_env,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
        assert not any('/.editor' in c['entry']['path'] for c in json.loads(output.stdout)['candidates'])
        with Session([binary, 'tui'], ignore_env, (14, 48)) as session:
            session.wait(b'loaded.')
            session.send(b'F')
            session.wait(b'Filter files')
            session.navigate('Files', 'Ignored paths [3]')
            session.wait(b'Ignored paths [Active]')
            assert b'Enter' in session.screen.text().splitlines()[-1]
            session.send(b'\r')
            session.wait(b'Unignore discovery path')
            session.ready_review()
            session.send(b'y')
            session.wait(b'Filter files')
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
            session.edit_fields('Path')
            session.send(b"q?/\r")
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
            session.edit_fields('Path')
            session.send(os.fsencode(unregistered) + b"\r")
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
            session.edit_fields('Manifest path')
            session.send(os.fsencode(standalone) + b"\r")
            session.wait(b"Declarations are valid")
            session.wait(b"Checks / Manifest")
            session.wait(b"Valid")
            session.wait(b"Declarations are valid")
            session.send(b"\x1b[6~" * 6)
            session.wait(b"Limits")
            session.send(b"\x1b[H")
            session.wait(b"Manifest check")
            session.send(b"j")
            session.wait(b"Checks / Diagnostics")
            session.send(b"m")
            session.wait(b"Checks / Manifest")
            session.send(b"v\x10")  # Open Profiles directly without a footer focus stop.
            session.wait(b"PROFILE")
            session.send(b"\x1b[B \r")  # Toggle dev explicitly, then apply from that row.
            session.edit_fields('Manifest path')
            session.send(b"\r")  # Validate directly from the retained path field.
            session.wait(b"Declarations are valid")
            session.send(b"q")
            session.finish()
        assert not (home / ".config").exists() and not (home / ".local").exists()
        print("PASS independent standalone check and nonpersistent profile additions")

        with Session([binary, "tui"], env, size=(14, 48)) as session:
            session.wait(b"loaded.")
            session.send(b"p")
            session.wait(b"REPOSITORY")
            session.wait(b"CHOICE")
            session.send(b"\x1bOR")  # F3 opens explicit scrollable choice metadata.
            session.wait(b"The exact root becomes")
            session.send(b"\x1b[6~\x1b[H")
            session.wait(b"The exact root becomes")
            session.send(b"\x1b")
            session.wait(b"REPOSITORY")
            session.send(b"\x0e")  # Ctrl-N opens Create/attach; no footer focus.
            session.edit_fields('Path')
            session.send(b"\x1b")  # Stop editing.
            session.settle()
            session.send(b"\x1b")  # Leave the form.
            session.settle()
            session.send(b"v")
            session.edit_fields('Manifest path')
            session.send(os.fsencode(standalone) + b"\x10")
            session.wait(b"PROFILE")
            session.wait(b"Mandatory")
            session.send(b"\x1bOR")
            session.wait(b"Never persisted")
            session.send(b"\x1b[6~")
            session.settle()
            reached = False
            for _ in range(40):
                if b"Effective union" in session.screen.text():
                    reached = True
                    break
                session.send(b"\x1b[6~")
                session.settle()
            assert reached, "Compact picker contributors were not keyboard reachable"
            session.send(b"\x1b[H")
            session.wait(b"Never persisted")
            session.send(b"\x1b")
            session.wait(b"PROFILE")
            session.send(b"\x1b[B \r")  # Toggle dev, then apply without a footer focus stop.
            session.edit_fields('Manifest path')
            session.send(b"\x1b")
            session.settle()
            session.send(b"q")
            session.finish()
        assert not list(home.iterdir()), "Read-only compact pickers persisted state"
        print("PASS compact typed pickers, details/Home and session-only validation")

        with Session([binary, "tui"], env, size=(14, 48)) as session:
            session.wait(b"loaded.")
            session.more_action("About Dort")
            session.wait(b"About")
            session.wait(b"Local repository create/attach")
            session.send(b"\x1b")
            session.settle()
            session.send(b"q")
            session.finish()
        assert not list(home.iterdir()), "Session actions created state"
        print("PASS session actions from More without dispatch")

        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.navigate("Checks", "Diagnostics [j]")
            session.wait(b"Checks / Diagnostics")
            session.wait(b"Registered repositories: 0")
            session.send(b"R")
            session.wait(b"Diagnostics | Dort")
            session.settle()  # identical refresh result may still be the prior frame
            session.navigate("Session", "About")
            session.wait(b"About")
            session.wait(b"Local repository create/attach")
            session.send(b"\x1b")
            session.wait(b"Checks / Diagnostics")
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
            session.wait(b"Registered repositories: Unknown")
            session.wait(b"Operations: 0")
            session.wait(b"Blocked writes: 0")
            session.send(b"\x1b[6~")
            session.wait(b"Config unavailable")
            assert b"private-config-marker" not in session.data
            session.send(b"\x1b[H")
            session.wait(b"Checks / Diagnostics")
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
            session.wait(b"Eligible")
            session.click_label(".q?config")  # selection only; content is automatic
            session.send(b"\r")  # retain inspection when shrinking to compact
            session.wait(b"private-body-marker")
            session.wait(b"Selected content")
            session.mouse(65, 90, 10)  # detail viewport, not row selection
            session.settle()
            scrolled = session.screen.text()
            session.send(b"\t\x1b[Z")
            session.settle()
            assert session.screen.text() == scrolled, "Focus round trip changed content or viewport"
            session.send(b"\x1b[H")
            session.wait(b"Eligible")
            session.more_action('Adopt')
            session.edit_fields('TARGET')
            session.click_label("ID")
            session.send(b"\x15mouse-id")
            session.wait(b"mouse-id")
            session.send(b"\x1b")  # Stop editing.
            session.settle()
            session.send(b"\x1b")  # Return to the selected file.
            session.settle()
            session.send(b"\r")  # Explicit inspection, independent of earlier paging/focus
            session.resize((14, 48))
            session.wait(b"Selected content")
            session.send(b"\x1b")
            session.wait(b"NAME")
            session.send(b"\x1b[C")  # Arrows do not switch destinations.
            session.settle()
            assert b"Files / Discover" in session.screen.text()
            session.navigate("Recovery", "All repositories")
            session.wait(b"All repositories")
            session.navigate("Files", "Managed")
            session.wait(b"Files / Managed")  # destination chooses managed view
            session.resize((21, 110))
            session.wait(b"0 / 0 shown")
            assert b"Selected item" not in session.screen.text(), "Empty scope invented a selected item"
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
            session.send(b'F')
            session.wait(b'focus-start-marker')
            session.send(b'\r')  # Explicit reader, not a Tab focus stop.
            session.settle()
            session.send(b'j\x06')  # Content row + full rendered page, not file selection.
            session.settle()
            assert b'second-file-marker' not in session.screen.text()
            session.send(b'\x1b[F')
            session.wait(b'focus-end-marker')
            session.send(b'\x1b[H')
            session.wait(b'focus-start-marker')
            session.resize((14, 48))
            session.wait(b'Selected content')
            session.send(b'\x1b[F')
            session.wait(b'focus-end-marker')
            session.send(b'\x1b')
            session.wait(b'NAME')
            session.send(b'j\r')  # List selection, then explicit compact reader.
            session.wait(b'second-file-marker')
            session.send(b'\x1bq')
            session.finish()
        assert not (focus_home / '.config').exists() and not (focus_home / '.local').exists()
        print('PASS Discover content focus, wrapped Vim pages/end, compact resize and separate file cursor without writes')

        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.send(b"F")
            session.wait(b"Eligible")
            session.send(b"\r")
            session.wait(b"Selected content")
            session.send(b"a")  # Direct adoption action.
            session.edit_fields('TARGET')
            session.send(b"\t\t\t\r")  # fields -> Review, no repository selected
            session.wait(b"Read failed")
            session.click_action('Retry read')  # Fresh read/preview intent, never execution.
            session.settle()
            session.wait(b"Read failed")
            session.click_action('Back')
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
        with Session([binary, "tui"], dict(env, PATH=str(bin_dir)), (30, 140)) as session:
            session.wait(b"loaded.")
            session.navigate("Git", "Choose Git scope [g]")
            session.edit_fields('Exact Git root')
            session.send(os.fsencode(git_only) + b"\r")
            session.wait(b"q?name")
            assert not (base / "patch-calls").exists(), "Untracked status requested a patch"
            session.send(b"/q?/\x1b")  # literal text, no quit/help/actions
            session.wait(b"0 displayed")
            session.send(b"\x1b[3~u")
            session.wait(b"Git / Unstaged")
            session.wait(b"PRIVATE-PATCH\\x1b[31m")
            rows = session.screen.text().splitlines()
            preview_row = next(row for row in rows if b"PRIVATE-PATCH" in row)
            assert preview_row.index(b"PRIVATE-PATCH") >= 50, "Patch was not in the right pane"
            session.send(b"/not-present\r")
            session.wait(b"No matches")
            assert b"PRIVATE-PATCH" not in session.screen.text()
            session.send(b"\x1b[3~R")
            session.wait(b"PRIVATE-PATCH\\x1b[31m")
            session.send(b"s")
            session.wait(b"Git / Staged")
            session.send(b"q")
            session.finish()
        assert not list(home.iterdir()) and not list(git_only.iterdir())
        print("PASS independent Git root, filter and automatic escaped patch preview")
        with Session([binary, "tui"], dict(env, PATH=str(bin_dir)), (14, 48)) as session:
            session.wait(b"loaded.")
            session.navigate("Git", "Choose Git scope [g]")
            session.edit_fields('Exact Git root')
            session.send(os.fsencode(git_only) + b"\r")
            session.wait(b"PATH")
            for column in [b"PATH", b"KIND", b"XY"]:
                session.wait(column)
            session.wait("› q?name".encode())
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
            session.send(b"\x1b[3~R")
            session.settle()  # identical summaries can otherwise match the prior frame
            session.wait(b"1 entries")
            session.wait(b"PATH")
            session.send(b"\x1bOR")
            session.wait(b"Git details")
            session.send(b"\x1b[6~" * 4)
            session.wait(b"Modes: 100644 -> 100644")
            assert b"PRIVATE-PATCH" not in session.screen.text(), "Scope details showed patch content"
            session.send(b"q")
            session.finish()
        assert not list(home.iterdir()) and not list(git_only.iterdir())
        print("PASS compact typed Git tables and full-scope details")

        with Session([binary, "tui"], dict(env, PATH=str(bin_dir)), (24, 110)) as session:
            session.wait(b"loaded.")
            session.navigate("Git", "Choose Git scope [g]")
            session.edit_fields('Exact Git root')
            session.send(os.fsencode(git_only) + b"\r")
            session.wait(b"PATH")
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
            session.wait(b"Git /")
            session.send(b"\x1b")
            session.wait(b"PATH")
            session.resize((22, 110))
            session.wait(b"PATH")
            session.send(b'\r')  # Reader availability does not depend on the split threshold.
            session.wait(b"Git details")
            session.send(b'\x1b')
            session.wait(b"PATH")
            session.resize((30, 140))
            session.navigate("Git", "Unstaged [u]")
            session.wait(b"Git / Unstaged")
            session.wait(b"100644 -> 100644")  # loaded diff before content action
            session.wait(b"PRIVATE-PATCH\\x1b[31m")
            session.resize((14, 48))
            session.wait(b"100644 -> 100644")
            assert b"PRIVATE-PATCH" not in session.screen.text()
            session.send(b"\r")
            session.wait(b"PRIVATE-PATCH\\x1b[31m")
            session.send(b"\x1b")
            session.wait(b"100644 -> 100644")
            session.send(b"q")
            session.finish()
        assert not list(home.iterdir()) and not list(git_only.iterdir())
        print("PASS Git metadata keys/mouse and compact patch preview after resize")

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
                session.edit_fields('Path')
                session.send(os.fsencode(created) + b"\r")
                session.wait(b"Review effects before confirming")
                session.send(b"\x1b")
                session.settle()
                assert not list(onboarding_home.iterdir()), "Preview/back created local state"
                if intent == "create":
                    assert not created.exists(), "Preview created repository"
                session.send(b"\r")  # retained draft -> fresh preview
                session.settle()
                session.ready_review()
                session.send(b"y")
                session.wait(b"Operation result")
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
        manifest_before = (repo / "dort.toml").read_bytes()
        with Session([binary, "tui"], dict(env, TERM="vt100")) as session:
            session.wait(b"loaded.")
            session.click_action("Profiles")
            session.wait(b"Overview / Profiles")
            session.click_action("New profile")
            session.wait(b"Profile name")
            session.send(b"\twork")  # Tab focuses the name field.
            session.wait(b"work")
            session.send(b"\t ")  # Tab to the first default, then toggle it.
            session.wait(b"[x] On")
            session.send(b"\x1bOP")  # VT100 PF1.
            session.wait(b"Enter a new name such as work")
            session.wait(b"Press Space on a row to toggle it")
            session.send(b"\x1bOP")
            session.wait(b"Esc Stop editing")
            session.wait(b"Default on Linux")
            screen = session.screen.text()
            assert b"work" in screen and b"[x] On" in screen
            assert b"Existing name" not in screen
            session.send(b"\x1b[11~")  # CSI PF1.
            session.wait(b"Enter a new name such as work")
            session.send(b"\x1b[11~")
            session.wait(b"Esc Stop editing")
            session.wait(b"Default on Linux")
            session.send(b"\x1b")
            session.send(b"\x1b")
            session.wait(b"F2 More")
            session.send(b"q")
            session.finish()
        assert (repo / "dort.toml").read_bytes() == manifest_before
        print("PASS PF1 shows profile field help and preserves the draft")

        target = home / ".editor"
        state = home / ".local/state"
        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.send(b"a")
            session.wait(b"Review effects before confirming")
            session.ready_review()
            session.send(b"\r\r\x1by")
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
            session.wait(b"Review effects before confirming")
            session.send(b"\r\ry")
            session.settle()
            assert b"y Confirm" not in session.screen.text(), "Dry-run opened confirmation"
            session.send(b"q")
            session.finish()
        assert not target.exists() and not target.is_symlink() and not state.exists()
        print("PASS dry-run cannot approve")

        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.send(b"a")
            session.wait(b"Review effects before confirming")
            session.ready_review()
            session.send(b"\r\r")
            session.settle()
            session.click_label('Read-only plan')
            session.settle()
            assert not target.exists() and not state.exists(), 'Enter or a body click approved apply'
            session.click_action('Confirm changes')
            deadline = time.monotonic() + TIMEOUT
            while not target.is_symlink() and time.monotonic() < deadline:
                session.pump()
            assert target.is_symlink(), "Approved TUI apply did not publish the link"
            session.wait(b"Operation result")
            session.wait(b"File operation completed.")
            session.send(b"yyq")
            session.finish()
        assert target.resolve() == repo / "source"
        assert (repo / "source").read_text() == "ordinary settings\n"
        print("PASS approved apply, persistent result and clean exit")

        retained = {str(path.relative_to(state)): hashlib.sha256(path.read_bytes()).digest()
                    for path in state.rglob("*.json")}
        with Session([binary, "tui"], env, (14, 48)) as session:
            session.wait(b"loaded.")
            session.send(b"a")
            session.wait(b"Review effects before confirming")
            session.ready_review()
            session.send(b"y")
            session.wait(b"Operation result")
            session.wait(b"Everything is up to date.")
            session.more_action('Refresh data')
            session.settle()
            session.wait(b"Everything is up to date.")
            session.click_action('Back')
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
                session.wait(b"Review effects before confirming")
                session.ready_review()
                session.send(b"y")
                session.wait(b"Possible partial effects")
                session.send(b"\x1b[6~" * 4)
                session.settle()
                assert b"Possible partial effects" in session.screen.text(), "Risk scrolled out of view"
                assert b"Retry execution" not in session.screen.text()
                session.more_action('Refresh data')  # Must work while the writer lock is held.
                session.wait(b"Earlier write failed:")
                session.destination("Recovery")
                session.wait(b"Blocked writes: 0")
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
            session.wait(b"1/1 shown")
            session.send(b"\r")
            session.wait(b"Recovery options")
            session.send(b"\x1b[6~" * 16)
            session.wait(b"Repository ID:")
            session.wait(b"Operation ID:")
            session.send(b"b")
            session.wait(b"Roll back operation")
            session.wait(b"Paths:")
            session.send(b"\x1b")
            session.wait(b"1/1 shown")
            assert target.is_symlink(), "Preview/back performed rollback"
            session.send(b"b")
            session.wait(b"Roll back operation")
            session.ready_review()
            session.send(b"y")
            session.wait(b"Operation result")
            assert not target.exists() and not target.is_symlink()
            # Return from the retained receipt before navigating.
            session.click_action('Back')
            # A new action needs a new actual preview and confirmation.
            session.destination("Overview")
            session.send(b"a")
            session.wait(b"Review effects before confirming")
            session.ready_review()
            session.send(b"y")
            session.wait(b"Operation result")
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
            session.wait(b"Blocked writes: 0")
            session.wait(b"1/2 shown")
            session.wait(b"Completed")
            session.send(b"\r")  # selected row inspects; never requests rollback
            session.wait(b"Operation details")
            session.wait(b"Recovery options")
            assert b"Repository ID:" not in session.screen.text()
            session.send(b"\x1b[6~" * 16)
            session.wait(b"Saved steps:")
            session.send(b"\x1b[H")
            session.wait(b"Recovery options")
            session.more_action('Rollback')  # Still opens a preview.
            session.wait(b"Roll back operation")
            session.send(b"\r\ry")
            session.settle()
            assert b"y Confirm" not in session.screen.text(), "Dry-run Recovery opened approval"
            session.send(b"\x1b")
            session.wait(b"Operation details")
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
            session.wait(b"1/2 shown")
            session.click_label("Completed", min_row=5)  # phase cell, below the retained filter
            session.wait(b"Operations [Active]")
            session.wait(b"Operation details")
            session.send(b"\t")
            session.wait(b"Operation details [Active]")
            assert session.screen.text().count(b"[Active]") == 1
            session.mouse(65, 90, 10)
            session.settle()
            scrolled = session.screen.text()
            session.send(b"\x1b[Z")
            session.wait(b"Operations [Active]")
            session.send(b"\t")
            session.wait(b"Operation details [Active]")
            assert scrolled == session.screen.text(), "Tab changed Recovery detail scroll or row"
            for width in (100, 99, 90):
                session.resize((24, width))
                session.wait(b"Operation details [Active]")
                session.wait(b"OP ID")
            session.resize((14, 48))
            session.wait(b"Operation details [Active]")
            assert b"OP ID" not in session.screen.text()
            session.send(b"\x1b")
            session.wait(b"Operations [Active]")
            session.wait(b"1/2 shown")
            session.click_label("Completed", min_row=5)
            session.send(b"\r")
            session.wait(b"Operation details [Active]")
            session.send(b"\x1b")
            session.wait(b"Operations [Active]")
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
            session.wait(b"Scope: every mapping selected by this request.")
            session.ready_review()
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
            session.send(b"m")
            session.edit_fields('TARGET')
            session.send(b"~/.q?config\tconfig\tfiles/config\r")  # Fields only; Enter previews.
            session.ready_review(b"y Adopt all")
            session.send(b"\x1b")
            session.settle()
            assert not candidate.is_symlink() and not (repo / "files/config").exists()
            session.send(b"\r")  # retained Review -> fresh preview
            session.ready_review(b"y Adopt all")
            session.send(b"y")  # Adopt all: one explicit approval, no typed yes.
            session.wait(b"Enter Details")
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
            session.more_action('Delete managed link: enter exact mapping ID')
            session.edit_fields('Exact ID')
            session.send(b"config\r")
            session.ready_review()
            session.wait(b"config")
            assert candidate.is_symlink(), "Preview executed without activation"
            session.send(b"y")
            session.wait(b"Operation result")
            session.send(b"q")
            session.finish()
        assert not candidate.exists() and not candidate.is_symlink()
        assert (repo / "files/config").read_text() == "ordinary adoption settings\n"
        assert target.is_symlink()
        print("PASS selected-ID link deletion outside display filter with simple confirmation")

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
            session.send(b'F')
            session.wait(b'Filter files')
            session.send(b'/bundle\rlljlj')  # Open .config and bundle, then select a leaf.
            session.click_label('[ ] only-a')
            session.click_label('[ ] only-b')
            session.wait(b'2 marked')
            session.send(b'h')  # Folding does not remove either mark.
            session.wait(b'2 hidden')
            session.send(b'lA')
            session.wait(b'All editable defaults')
            session.wait(b'Review all')
            session.settle()
            assert b'ADOPTION' not in session.screen.text(), 'Batch editor leaked the Discover table underneath'
            assert b'ordinary settings' not in session.screen.text(), 'Batch editor leaked selected-file content'
            assert b'a Adopt' not in session.screen.text(), 'Batch editor offered single-file adoption'
            assert b'i Ignore' not in session.screen.text(), 'Batch editor offered to ignore a discovered path'
            session.send(b'\x1bOR')  # F3 opens scrollable full selected-item paths.
            session.wait(b'Selected draft')
            session.wait(b'files/.config/bundle/only-a')
            session.send(b'\x1b[H\x1b')
            session.settle()
            session.wait(b'All editable defaults')
            session.send(b'je')  # Select only-b and open a separate item form.
            session.wait(b'Edit item')
            session.send(b'\t\t\x15edited-b\r')  # Enter fields, TARGET -> ID; Enter retains edits.
            session.wait(b'Review all')
            session.send(b'e')
            session.wait(b'edited-b')
            session.click_label('Done')  # Return only; never review/adopt an individual item.
            session.wait(b'Review all')
            session.click_label('Review all')
            session.wait(b'One approval / one transaction')
            session.ready_review(b'y Adopt all')
            session.send(b'y')  # Adopt all consumes the real combined preview once.
            session.wait(b'Enter Details')
            assert all((targets / name).is_symlink() for name in ('only-a', 'only-b'))
            for name in ('only-a', 'only-b'):
                assert (targets / name).resolve() == batch_repo / 'files/.config/bundle' / name
            batch_heads(batch_home, 1)
            session.destination("Recovery")
            session.send(b'b')
            session.wait(b'Roll back operation')
            session.ready_review()
            session.send(b'y')
            session.wait(b'Operation result')
            session.send(b'q')
            session.finish()
        for name in ('only-a', 'only-b'):
            assert not (targets / name).is_symlink()
            assert (targets / name).read_text() == 'ordinary settings\n'
            assert not (batch_repo / 'files/.config/bundle' / name).exists()
        batch_heads(batch_home, 1)
        print('PASS tree marks/folding, editable combined adoption and schema-4 whole rollback')

        for operation in ('U', 'D'):
            batch_home, batch_repo, targets, batch_env = batch_fixture('batch-' + operation, True)
            with Session([binary, 'tui'], batch_env) as session:
                session.wait(b'loaded.')
                session.destination("Files")
                session.send(b'ljlj')  # Folders start closed.
                session.click_label('[ ] only-a')
                session.click_label('[ ] only-b')
                session.send(b'/only-a\rA')  # Hidden only-b remains explicitly included.
                session.wait(b'One approval / one transaction')
                session.ready_review()
                session.send(b'y')
                session.wait(b'Operation result')
                assert all((targets / name).is_symlink() for name in ('only-a', 'only-b'))
                assert not (targets / 'only-c').is_symlink()
                batch_heads(batch_home, 1)
                session.send(b'\x1b')
                session.settle()
                session.send(operation.encode())
                session.wait(b'One approval / one transaction')
                if operation == 'D':
                    session.wait(b'only-a')
                    session.send(b'\x1b[F')
                    session.wait(b'only-b')
                    assert all((targets / name).is_symlink() for name in ('only-a', 'only-b'))
                    session.resize((14, 47))
                    session.wait(b'terminal too small')
                    session.resize((24, 80))
                    session.wait(b'y Confirm changes')
                    session.settle()
                    assert all((targets / name).is_symlink() for name in ('only-a', 'only-b')), 'Resize executed changes'
                session.ready_review()
                session.send(b'y')
                session.wait(b'Batch file operation completed.')
                session.send(b'q')
                session.finish()
            for name in ('only-a', 'only-b'):
                assert not (targets / name).is_symlink()
                if operation == 'U':
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
            session.ready_review()
            session.send(b'y')
            session.wait(b'Operation result')
            session.press(b'\x1b', b'Dort / Files / Managed')
            session.press(b'l', b'bundle')
            session.press(b'j', '›   [>] bundle'.encode())
            session.press(b'l', b'only-a')
            session.press(b'j', '›     [ ] only-a'.encode())
            session.send(b' u')  # Mark A, then unmanage it.
            session.ready_review()
            session.send(b'y')
            session.wait(b'File operation completed.')
            session.press(b'\x1b', b'0 marked (0 hidden)')
            session.press(b'\x1b[H', '› [v] .config'.encode())
            session.press(b'j', '›   [v] bundle'.encode())
            session.press(b'j', '›     [ ] only-b'.encode())
            session.send(b' U')
            session.wait(b'One approval / one transaction')
            assert b'only-a' not in session.screen.text(), 'Completed Keep leaked into a later batch'
            session.ready_review()
            session.send(b'y')
            session.wait(b'Batch file operation completed.')
            session.press(b'\x1b', b'0 marked (0 hidden)')
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
        print('PASS selected and marked Unmanage without stale hidden marks')

        for mode, code in [("normal", 0), ("error", 0), ("panic", 101)]:
            with Session(
                [probe, "--exact", "tui::tests::terminal_cleanup_probe", "--nocapture"],
                dict(env, DORT_TEST_TERMINAL_CLEANUP=mode),
            ) as session:
                session.finish(code)
            print("PASS terminal cleanup", mode)
    print("All PTY cases passed (disposable HOME, no installed Git requirement)")


if __name__ == "__main__":
    if sys.argv[1:] == ["--guarded-effects"]:
        guarded_effects(artifact(["build", "-p", "dort"], "binary"))
    else:
        run()
