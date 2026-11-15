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

    def navigate(self, group, item):
        self.settle()
        self.send(b'\x1b[21~')  # Places is groups-first at every width.
        self.wait(b'PLACE')
        self.settle()
        marker = ' ▸' if (group + ' ▸').encode() in self.screen.text() else ' >'
        self.click_label(group + marker)
        label = item.split(' [')[0]
        if group == 'Overview':
            self.settle()
            label = ('› ' if ('› ' + label).encode() in self.screen.text() else '> ') + label
        self.click_label(label, min_row=3)

    def destination(self, group):
        items = {"Overview": "Overview", "Files": "Managed",
                 "Recovery": "All repositories", "Checks": "Manifest checks [m]"}
        if group == "Git":
            self.navigate("Git", "Choose Git scope [g]")
            self.send(b"\x1b")
            self.settle()
        else:
            self.navigate(group, items[group])

    def more_action(self, label):
        self.settle()  # Let an automatic read refresh and its foreground frame complete.
        self.send(b'\x1bOQ')
        self.wait(b'ACTION')
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

    def choose_snapshot(self, number):
        self.wait(b'Enter Compare')  # The binding page, not its retained loading title.
        self.settle()
        self.send(b' ')
        self.wait(b'Choose inspection')
        self.send(b'\x1b[H' + b'\x1b[B' * number + b'\r')
        self.settle()

    def closure_provider(self, number):
        self.send(b'\x1bOS')  # F4 opens an explicit choice, never a provider cycle.
        self.wait(b'Choose historical provider')
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


def tools_workflows(binary, base, env):
    home = base / 'tools-home'
    home.mkdir(mode=0o700)
    tools_env = dict(env, HOME=str(home), PATH='/no-external-programs', XDG_CONFIG_HOME=str(home / '.config'),
                     XDG_STATE_HOME=str(home / '.local/state'))
    executable = base / 'fake-provider'
    executable.write_text('#!/bin/sh\nprintf ran >> "$HOME/calls"\n'
                          "printf '{\"formulae\":[{\"full_name\":\"ripgrep\",\"installed\":[{\"version\":\"14.1.0\"}]}]}'\n")
    executable.chmod(0o700)
    auto_home, auto_bin = base / 'defaults-home', base / 'defaults-bin'
    auto_home.mkdir(mode=0o700)
    auto_bin.mkdir(mode=0o700)
    (auto_bin / 'brew').write_bytes(executable.read_bytes())
    (auto_bin / 'brew').chmod(0o700)
    auto_env = dict(tools_env, HOME=str(auto_home), PATH=str(auto_bin),
                    XDG_CONFIG_HOME=str(auto_home / '.config'),
                    XDG_STATE_HOME=str(auto_home / '.local/state'))
    with Session([binary, 'tui'], auto_env) as session:
        session.wait(b'loaded.')
        session.send(b'\x1b[D')
        session.wait(b'[x] Homebrew')
        assert not (auto_home / 'calls').exists(), 'Default discovery executed a provider'
        session.send(b'd')
        session.wait(b'Exact inspection scopes')
        assert not (auto_home / 'calls').exists(), 'Scope details executed a provider'
        session.send(b'd')
        session.click_action('Inspect installed tools')
        session.wait(b'Snapshot s1')
        assert (auto_home / 'calls').read_text() == 'ran', 'Collection did not execute exactly once'
        session.send(b'c')
        session.wait(b'[x] Homebrew')
        assert (auto_home / 'calls').read_text() == 'ran', 'Refresh replayed approval'
        session.send(b'q')
        session.finish()
    with Session([binary, '--dry-run', 'tui'], auto_env, (14, 48)) as session:
        session.wait(b'loaded.')
        session.send(b'\x1b[D')
        session.wait(b'[x] Homebrew')
        session.send(b'\r')
        session.settle()
        assert (auto_home / 'calls').read_text() == 'ran', 'Preview-only defaults executed'
        session.send(b'q')
        session.finish()
    print('PASS default discovery, exact scopes, one-click collection and compact preview-only guard')
    with Session([binary, 'tui'], tools_env) as session:
        session.wait(b'loaded.')
        session.send(b'\x1b[D')
        session.wait(b'Inspect installed tools')
        session.send(b'e' + str(executable).encode() + b'\t\x15/no-external-programs\t\r')
        session.wait(b'Run query')
        assert not (home / 'calls').exists(), 'Provider executed during review'
        session.resize((13, 47))
        session.wait(b'terminal too small')
        session.resize((24, 80))
        session.wait(b'Review exact query')
        assert not (home / 'calls').exists(), 'Resize executed inspection'
        session.send(b'\r')
        session.wait(b'Run query')
        session.send(b'\r')
        session.wait(b'1/1 reported')
        session.settle()
        assert b'Search metadata' not in session.screen.text()
        assert session.screen.text().count(b'F1 Help') == 1, 'Inventory has duplicate action hints'
        session.send(b'\x1bOQ')  # F2 is contextual More on the migrated inventory surface.
        session.wait(b'More actions')
        assert (home / 'calls').read_text() == 'ran', 'Opening More ran a provider'
        session.mouse(0, 0, 0)
        session.settle()
        assert b'More actions' in session.screen.text(), 'Outside click dismissed More'
        session.send(b'\x1b')
        session.settle()
        session.wait(b'Filter packages')
        assert (home / 'calls').read_text() == 'ran', 'Dismissing More replayed approval'
        session.send(b'/RIPGREP\r')
        session.wait(b'ripgrep')
        session.send(b'r')
        session.wait(b'Run query')
        session.settle()
        assert (home / 'calls').read_text() == 'ran', 'Retry replayed inspection approval'
        session.send(b'\x1b')
        session.wait(b'Trusted executable')
        session.send(b'\x1b')
        session.wait(b'Filter packages')
        session.send(b'q')
        session.finish()
    assert sorted(p.name for p in home.iterdir()) == ['calls'], 'Inspection created Dort state'
    print('PASS Tools query review, explicit activation, resize, search and fresh retry')
    with Session([binary, '--dry-run', 'tui'], tools_env, (14, 48)) as session:
        session.wait(b'loaded.')
        session.send(b'\x1b[D')
        session.wait(b'Homebrew')
        session.send(b'e' + str(executable).encode() + b'\t\x15/no-external-programs\t\r')
        session.wait(b'execution disabled')
        session.send(b'\r')
        session.settle()
        assert (home / 'calls').read_text() == 'ran'
        session.send(b'\x1b')
        session.wait(b'Enter Review')
        session.settle()
        session.send(b'\x1b')
        session.wait(b'No inspection collected yet.')
        session.send(b'q')
        session.finish()
    print('PASS compact preview-only Tools cannot execute providers')
    with Session([binary, 'tools', 'inspect', 'brew', '--executable', str(executable),
                  '--search-dir', '/no-external-programs'], tools_env) as session:
        session.wait(b'Run query? [y/N]')
        assert (home / 'calls').read_text() == 'ran'
        session.send(b'y\n')
        session.finish(tui=False)
    assert (home / 'calls').read_text() == 'ranran'
    assert sorted(p.name for p in home.iterdir()) == ['calls']
    print('PASS interactive CLI provider inspection uses simple y/N confirmation')


def tool_declaration_workflows(binary, base, env):
    home, repo, bin_dir = (base / name for name in ('declaration-home', 'declaration-repo', 'declaration-bin'))
    for directory in (home, repo, bin_dir):
        directory.mkdir(mode=0o700)
    (repo / '.git').mkdir()
    original = '# retained declaration fixture\nschema=1\n'
    (repo / 'dort.toml').write_text(original)
    git = bin_dir / 'git'
    git.write_text('#!/bin/sh\nfor arg do\ncase "$arg" in\nconfig) exit 1;;\nrev-parse) pwd -P; exit 0;;\nstatus) printf "# branch.head main\\000"; exit 0;;\nesac\ndone\nexit 94\n')
    git.chmod(0o700)
    provider = bin_dir / 'demo'
    provider.write_text('#!/bin/sh\nprintf ran > "$HOME/PROVIDER-RAN"\n')
    provider.chmod(0o700)
    scoped = dict(env, HOME=str(home), PATH=str(bin_dir), XDG_CONFIG_HOME=str(home / '.config'),
                  XDG_STATE_HOME=str(home / '.local/state'))
    subprocess.run([binary, '--json', '--yes', 'init', str(repo)], env=scoped,
                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
    with Session([binary, 'tui'], scoped) as session:
        session.wait(b'loaded.')
        session.send(b'\x1b[D')
        session.wait(b'Homebrew formulae')
        session.send(b'a')
        session.wait(b'Add tool declaration')
        session.send(b'demo\tdemo\t\t ')
        session.wait(b'macOS declaration provider')
        session.send(b'\x1b[H\x1b[B\r\tdemo\t\t ')
        session.wait(b'Ubuntu declaration provider')
        session.send(b'\x1b[H\x1b[B\r\tdemo')
        session.wait(b'Review declaration')
        session.send(b'\x1b')
        session.wait(b'No inspection collected yet.')
        session.send(b'a\r')  # Retained complete draft, not typed confirmation.
        session.wait(b'Already declared: false')
        assert (repo / 'dort.toml').read_text() == original
        session.send(b'\r')
        session.wait(b'Confirm')
        assert (repo / 'dort.toml').read_text() == original
        session.click_action('Confirm')
        session.wait(b'Tool declaration completed.')
        session.send(b'q')
        session.finish()
    assert '[[tools]]' in (repo / 'dort.toml').read_text()
    output = subprocess.run([binary, '--json', 'history'], env=scoped,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
    row = next(r for r in json.loads(output.stdout)['transactions'] if r['journal_schema'] == 5)
    with Session([binary, 'tui'], scoped, (14, 48)) as session:
        session.wait(b'loaded.')
        session.destination('Recovery')
        session.send(b'/' + row['transaction_id'][:16].encode() + b'\rb')
        session.wait(b'Rollback schema 5')
        session.send(b'\r')
        session.wait(b'Confirm')
        session.send(b'\r')
        session.wait(b'Operation result')
        session.send(b'q')
        session.finish()
    assert (repo / 'dort.toml').read_text() == original
    assert not (home / 'PROVIDER-RAN').exists()
    print('PASS retained multi-platform tool declaration, click confirmation and compact schema-5 Recovery')


def tools_reconciliation_workflows(binary, base, env):
    home = base / 'reconciliation-home'
    home.mkdir(mode=0o700)
    lookup = base / 'reconciliation-bin'
    lookup.mkdir(mode=0o700)
    scoped_env = dict(env, HOME=str(home), PATH='/no-external-programs', XDG_CONFIG_HOME=str(home / '.config'),
                      XDG_STATE_HOME=str(home / '.local/state'))
    manifest = base / 'reconciliation.toml'
    manifest.write_text("schema=1\n[[tools]]\nid='node'\nbinary='node'\n[tools.install]\n"
                        "macos={provider='mise',package='node',version='22'}\nubuntu={provider='mise',package='node',version='22'}\n"
                        "[[tools]]\nid='rg'\nbinary='rg'\n[tools.install]\n"
                        "macos={provider='mise',package='rg'}\nubuntu={provider='mise',package='rg'}\n")
    scopes = []
    for name, package, version in [('first', 'node', '22.1.0'), ('second', 'rg', '14.1.0')]:
        executable = base / ('reconciliation-' + name)
        payload = json.dumps({package: [{'version': version, 'installed': True}]})
        executable.write_text('#!/bin/sh\nif [ "$1" = install ]; then printf ran > "$HOME/INSTALL-RAN"; fi\nprintf "' + name + '\\n" >> "$HOME/calls"\nprintf \'%s\' \'' + payload + "'\n")
        executable.chmod(0o700)
        scopes.append({'id': name, 'provider': 'mise', 'executable': str(executable),
                       'search_dirs': ['/no-external-programs']})
    candidate = lookup / 'node'
    candidate.write_text('#!/bin/sh\nprintf PRIVATE > "$HOME/candidate-executed"\n')
    candidate.chmod(0o700)
    with Session([binary, 'tui'], scoped_env) as session:
        session.wait(b'loaded.')
        session.send(b'\x1b[D')
        session.wait(b'Inspect installed tools')
        session.send(b'e\x1b[Z ')
        session.wait(b'Choose inspection provider')
        session.send(b'\x1b[H\x1b[B\x1b[B\r\t' + scopes[0]['executable'].encode() + b'\t\x15/no-external-programs\r')
        session.wait(b'Run query')
        session.send(b'\r')
        session.wait(b'Snapshot s1')
        session.send(b'e\t\x15' + scopes[1]['executable'].encode() + b'\t\t\r')
        session.wait(b'Run query')
        session.send(b'\r')
        session.wait(b'Snapshot s2')
        assert (home / 'calls').read_text() == 'first\nsecond\n'
        session.send(b's')
        session.wait(b'Session snapshots')
        session.send(b'\x1b[A\r')
        session.wait(b'Snapshot s1')
        session.send(b'p' + str(manifest).encode() + b'\t' + str(lookup).encode() + b'\t\t\r')
        session.wait(b'Not observed')
        session.choose_snapshot(1)
        session.send(b'\t')
        session.choose_snapshot(2)
        session.send(b'\r')  # node=s1, rg=s2, then compare.
        session.wait(b'Package proposals/no-ops only')
        session.send(b'\r')
        session.wait(b'analysis_scope')
        session.send(b'\x1b[6~\x1b[H')
        session.wait(b'analysis_scope')
        session.send(b'\x1b')
        session.wait(b'Package proposals/no-ops only')
        session.send(b'\x1b')
        session.wait(b'Load declarations')
        session.send(b'\r')
        session.wait(b'Enter Compare')
        session.click_label('rg (rg)')
        session.choose_snapshot(1)  # rg=s1 explicitly (package absent there).
        session.send(b'\r')
        session.wait(b'Review only')
        session.send(b'\x1b[B\r')
        for _ in range(100):
            session.settle()
            if b'PREVIEW ONLY' in session.screen.text():
                break
            session.send(b'\x1b[6~')
        session.wait(b'PREVIEW ONLY')
        assert not (home / 'INSTALL-RAN').exists()
        session.send(b'\x1b')
        session.wait(b'Review only')
        session.send(b'\x1b')
        session.wait(b'Load declarations')
        session.send(b'\x1b')
        session.wait(b'Filter packages')
        session.send(b's')
        session.wait(b'Session snapshots')
        session.send(b'x')
        session.settle()
        assert b's1 |' not in session.screen.text() and b's2 |' in session.screen.text()
        assert (home / 'calls').read_text() == 'first\nsecond\n'
        assert not (home / 'candidate-executed').exists()
        session.send(b'\x1b')
        session.wait(b'Filter packages')
        session.send(b'q')
        session.finish()
    print('PASS multiple snapshots, explicit bindings, non-executable command previews and forget without queries')
    cli = [binary, 'tools', 'reconcile', '--manifest', str(manifest), '--tool-search-dir', str(lookup),
           '--bind', 'node=first', '--bind', 'rg=second']
    for scope in scopes:
        cli.extend(['--scope', json.dumps(scope)])
    before = (home / 'calls').read_text()
    with Session(cli, scoped_env) as session:
        session.wait(b'Snapshot first:')
        assert (home / 'calls').read_text() == before
        session.send(b'y\n')
        session.wait(b'Snapshot second:')
        assert (home / 'calls').read_text() == before + 'first\n'
        session.send(b'y\n')
        session.finish(tui=False)
    assert (home / 'calls').read_text() == before + 'first\nsecond\n'
    assert not (home / 'candidate-executed').exists()
    assert sorted(p.name for p in home.iterdir()) == ['calls']
    print('PASS CLI reconciliation separately confirms each fixed inspection scope')


def tool_install_workflows(binary, base, env):
    home, repo, bin_dir, lookup = (base / name for name in ('install-home', 'install-repo', 'install-bin', 'install-lookup'))
    for directory in (home, repo, bin_dir, lookup):
        directory.mkdir(mode=0o700)
    (repo / '.git').mkdir()
    (repo / 'dort.toml').write_text("schema=1\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nmacos={provider='mise',package='demo',version='1.2.3'}\nubuntu={provider='mise',package='demo',version='1.2.3'}\n")
    git = bin_dir / 'git'
    git.write_text('#!/bin/sh\nfor arg do\ncase "$arg" in\nconfig) exit 1;;\nrev-parse) pwd -P; exit 0;;\nstatus) printf "# branch.head main\\000"; exit 0;;\nesac\ndone\nexit 94\n')
    git.chmod(0o700)
    mise = bin_dir / 'mise'
    mise.write_text('#!/bin/sh\nif [ "$1" = ls ]; then\nprintf Q >> "$HOME/calls"\nif [ -f "$HOME/installed" ]; then printf \'{"demo":[{"version":"1.2.3","installed":true}]}\'; else printf "{}"; fi\nexit 0\nfi\nprintf I >> "$HOME/calls"\nprintf started > "$HOME/started"\nprintf PRIVATE_INSTALL_OUTPUT; printf PRIVATE_INSTALL_ERROR >&2\nn=0\nwhile [ ! -f "$HOME/release" ] && [ "$n" -lt 2000000 ]; do n=$((n+1)); done\nprintf installed > "$HOME/installed"\n')
    mise.chmod(0o700)
    scoped = dict(env, HOME=str(home), PATH=str(bin_dir), XDG_CONFIG_HOME=str(home / '.config'),
                  XDG_STATE_HOME=str(home / '.local/state'))
    subprocess.run([binary, '--json', '--yes', 'init', str(repo)], env=scoped,
                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
    with Session([binary, 'tui'], scoped) as session:
        session.wait(b'loaded.')
        session.send(b'\x1b[D')
        session.wait(b'[x] mise')
        assert not (home / 'calls').exists(), 'Defaults executed mise'
        session.send(b'\r')
        session.wait(b'0 packages reported')
        session.send(b'p' + str(repo / 'dort.toml').encode() + b'\t' + str(lookup).encode() + b'\t\t\r')
        session.wait(b'Compare')
        session.choose_snapshot(1)
        session.send(b'\r')
        session.wait(b'Package proposals/no-ops only')
        session.more_action('Review exact mise')
        session.wait(b'Enter Install')
        assert (home / 'calls').read_text() == 'Q', 'Review executed a new query or install'
        session.resize((14, 48))
        session.wait(b'Enter Install')
        session.send(b'd')
        session.wait(b'Tools installation details')
        session.send(b'd')
        session.click_action('Install')
        session.wait(b'Installing package')
        session.send(b'q')
        session.wait(b'waiting for worker')
        (home / 'release').write_text('release fixture')
        session.finish()
        assert b'PRIVATE_INSTALL' not in session.data, 'Raw provider output reached terminal'
    assert (home / 'calls').read_text() == 'QQIQ'
    assert (home / 'installed').exists(), 'Quit did not join the approved install'
    print('PASS exact mise review, compact scope, one-click install, typed progress and joined quit')
    attempts = home / '.local/state/dort/tools-attempts'
    damaged = attempts / ('0' * 64)
    damaged.mkdir(mode=0o700)
    before = (home / 'calls').read_text()
    with Session([binary, 'tui'], scoped, (14, 48)) as session:
        session.wait(b'loaded.')
        session.navigate('Tools', 'Installation attempts [i]')
        session.wait(b'UNRESOLVED')
        session.send(b'c')
        session.wait(b'Trusted mise executable')
        session.send(str(mise).encode() + b'\t' + str(bin_dir).encode() + b'\t\r')
        session.wait(b'Enter Inspect')
        assert (home / 'calls').read_text() == before, 'Closure review executed mise'
        session.send(b'\r')
        session.wait(b'Explicitly closed')
        session.send(b'\x1b')
        session.wait(b'Tools /')
        session.send(b'q')
        session.finish()
    assert (home / 'calls').read_text() == before + 'Q'
    print('PASS compact all-repository attempt inspection, missing-scope draft and one-activation closure')


def npm_install_workflows(binary, base, env):
    home, repo, bin_dir = (base / name for name in ('npm-home', 'npm-repo', 'npm-bin'))
    for directory in (home, repo, bin_dir):
        directory.mkdir(mode=0o700)
    (repo / '.git').mkdir()
    manifest = repo / 'dort.toml'
    manifest.write_text("schema=1\n[[tools]]\nid='node'\nbinary='node'\n[tools.install]\nmacos={provider='mise',package='node',version='20.1.0'}\nubuntu={provider='mise',package='node',version='20.1.0'}\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nmacos={provider='npm',package='demo',version='1.2.3',runtime_tool='node'}\nubuntu={provider='npm',package='demo',version='1.2.3',runtime_tool='node'}\n")
    git = bin_dir / 'git'
    git.write_text('#!/bin/sh\nfor arg do\ncase "$arg" in\nconfig) exit 1;;\nrev-parse) pwd -P; exit 0;;\nstatus) printf "# branch.head main\\000"; exit 0;;\nesac\ndone\nexit 94\n')
    mise = bin_dir / 'mise'
    mise.write_text('#!/bin/sh\nprintf M >> "$HOME/calls"\nprintf \'{"node":[{"version":"20.1.0","installed":true}]}\'\n')
    node = bin_dir / 'node'
    node.write_text('#!/bin/sh\nshift\nif [ "$1" = ls ]; then\nprintf N >> "$HOME/calls"\nif [ -f "$HOME/installed" ]; then printf \'{"dependencies":{"demo":{"version":"1.2.3"}}}\'; else printf \'{"dependencies":{}}\'; fi\nexit 0\nfi\nprintf I >> "$HOME/calls"\nprintf started > "$HOME/started"\nprintf PRIVATE_NPM_OUTPUT; printf PRIVATE_NPM_ERROR >&2\nn=0\nwhile [ ! -f "$HOME/release" ] && [ "$n" -lt 10000000 ]; do n=$((n+1)); done\nprintf installed > "$HOME/installed"\n')
    for program in (git, mise, node):
        program.chmod(0o700)
    cli = home / 'npm-cli.js'
    cli.write_text('TRUSTED_CLI_NOT_EXECUTED_DIRECTLY')
    prefix = home / 'global-prefix'
    scoped = dict(env, HOME=str(home), PATH=str(bin_dir), XDG_CONFIG_HOME=str(home / '.config'),
                  XDG_STATE_HOME=str(home / '.local/state'))
    subprocess.run([binary, '--json', '--yes', 'init', str(repo)], env=scoped,
                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
    with Session([binary, 'tui'], scoped, (40, 120)) as session:
        session.wait(b'loaded.')
        session.send(b'\x1b[D')
        session.wait(b'[x] mise')
        session.send(b'\r')
        session.wait(b'node')
        assert (home / 'calls').read_text() == 'M'
        session.send(b'c')
        session.wait(b'Inspect installed tools')
        session.send(b'\x1b[Be')  # Move from mise to npm, then Advanced.
        session.wait(b'Global npm prefix')
        session.click_label('Trusted executable')
        for value in (node, bin_dir, manifest, 'node', cli, prefix):
            session.send(b'\x15' + str(value).encode() + b'\t')
        session.send(b'\r')
        session.wait(b'Run query')
        assert (home / 'calls').read_text() == 'M', 'npm review executed a query'
        session.send(b'\r')
        session.wait(b'0 packages reported')
        session.send(b'p' + str(manifest).encode() + b'\t' + str(bin_dir).encode() + b'\t\t\r')
        session.wait(b'Compare')
        session.choose_snapshot(1)
        session.send(b'\t')
        session.choose_snapshot(1)
        session.send(b'\r')
        session.wait(b'Package proposals/no-ops only')
        session.send(b'\x1b[B')
        session.more_action('Review exact npm')
        session.wait(b'scripts off')
        assert (home / 'calls').read_text() == 'MN', 'Install review executed a query'
        assert not prefix.exists(), 'Install review created prefix'
        session.resize((14, 48))
        session.wait(b'bin/man')
        session.wait(b'No rollback')
        session.send(b'\r')
        session.wait(b'Installing package')
        session.send(b'\x03')
        session.wait(b'waiting for worker')
        (home / 'release').write_text('fixture')
        session.finish()
        assert b'PRIVATE_NPM' not in session.data, 'Raw npm output reached terminal'
    assert (home / 'calls').read_text() == 'MNMNIMN'
    assert (home / 'installed').exists() and prefix.is_dir()
    print('PASS npm Advanced scope, explicit runtime catalog binding, compact native-effects warning and joined Ctrl-C')
    damaged = home / '.local/state/dort/tools-attempts' / ('0' * 64)
    damaged.mkdir(mode=0o700)
    before = (home / 'calls').read_text()
    with Session([binary, 'tui'], scoped, (40, 120)) as session:
        session.wait(b'loaded.')
        session.navigate('Tools', 'Installation attempts [i]')
        session.wait(b'UNRESOLVED')
        session.send(b'c')
        session.wait(b'Trusted mise executable')
        session.closure_provider(1)
        session.wait(b'Trusted Node executable')
        session.click_label('Trusted Node executable')
        for value in (node, bin_dir, cli, prefix):
            session.send(b'\x15' + str(value).encode() + b'\t')
        session.send(b'\r')
        session.wait(b'Enter Inspect')
        assert (home / 'calls').read_text() == before, 'Closure preview ran Node'
        session.resize((14, 48))
        session.wait(b'Enter Inspect')
        session.send(b'\r')
        session.wait(b'Explicitly closed')
        session.send(b'\x1bq')
        session.finish()
    assert (home / 'calls').read_text() == before + 'N'
    print('PASS damaged npm attempt provider/Node/CLI/prefix form and compact one-activation closure')


def brew_install_workflows(binary, base, env):
    home, repo, bin_dir = (base / name for name in ('brew-home', 'brew-repo', 'brew-bin'))
    for directory in (home, repo, bin_dir):
        directory.mkdir(mode=0o700)
    (repo / '.git').mkdir()
    manifest = repo / 'dort.toml'
    manifest.write_text("schema=1\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nmacos={provider='brew',package='demo'}\nubuntu={provider='mise',package='demo',version='1.2.3'}\n")
    prefix = home / 'prefix'
    for name in ('Homebrew/bin', 'Homebrew/Library', 'Cellar', 'bin', 'etc', 'include', 'lib', 'opt', 'sbin', 'share', 'var/homebrew'):
        (prefix / name).mkdir(parents=True, exist_ok=True)
    git = bin_dir / 'git'
    git.write_text('#!/bin/sh\nfor arg do\ncase "$arg" in\nconfig) exit 1;;\nrev-parse) pwd -P; exit 0;;\nstatus) printf "# branch.head main\\000"; exit 0;;\nesac\ndone\nexit 94\n')
    git.chmod(0o700)
    brew = prefix / 'Homebrew/bin/brew'
    brew.write_text('''#!/bin/sh
printf '%s\\n' "$*" >> "$HOME/calls"
case "$*" in
'help install') printf '%s' '--formula --no-ask --skip-post-install HOMEBREW_NO_INSTALL_UPGRADE HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK HOMEBREW_NO_INSTALL_CLEANUP';;
'--prefix') printf '%s\\n' "$HOME/prefix";;
'--repository') printf '%s\\n' "$HOME/prefix/Homebrew";;
'--cellar') printf '%s\\n' "$HOME/prefix/Cellar";;
'info --json=v2 --formula homebrew/core/demo') printf '{"formulae":[{"name":"demo","full_name":"demo","tap":"homebrew/core","disabled":false,"versions":{"stable":"2.7"},"installed":['; if [ -f "$HOME/installed" ]; then printf '{"version":"2.7"}'; fi; printf ']}]}';;
'info --json=v2 --installed --formula') if [ -f "$HOME/installed" ]; then printf '{"formulae":[{"full_name":"demo","installed":[{"version":"2.7"}]}]}'; else printf '{"formulae":[]}'; fi;;
'install --formula --no-ask --skip-post-install homebrew/core/demo')
printf started > "$HOME/started"
printf PRIVATE_BREW_OUTPUT; printf PRIVATE_BREW_ERROR >&2
n=0
while [ ! -f "$HOME/release" ] && [ "$n" -lt 10000000 ]; do n=$((n+1)); done
printf installed > "$HOME/installed";;
*) exit 99;;
esac
''')
    brew.chmod(0o700)
    scoped = dict(env, HOME=str(home), PATH=str(bin_dir), XDG_CONFIG_HOME=str(home / '.config'),
                  XDG_STATE_HOME=str(home / '.local/state'))
    subprocess.run([binary, '--json', '--yes', 'init', str(repo)], env=scoped,
                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
    if sys.platform == 'darwin':
        with Session([binary, 'tui'], scoped, (40, 120)) as session:
            session.wait(b'loaded.')
            session.send(b'\x1b[D')
            session.wait(b'Homebrew')
            session.send(b'e' + str(brew).encode() + b'\t\x15' + str(bin_dir).encode() + b'\t\r')
            session.wait(b'Run query')
            assert not (home / 'calls').exists(), 'Brew review executed a query'
            session.send(b'\r')
            session.wait(b'0 packages reported')
            session.send(b'p' + str(manifest).encode() + b'\t' + str(bin_dir).encode() + b'\t\t\r')
            session.wait(b'Compare')
            session.choose_snapshot(1)
            session.send(b'\r')
            session.wait(b'Package proposals/no-ops only')
            before = (home / 'calls').read_text()
            session.more_action('Review core formula')
            session.wait(b'CAN EXECUTE')
            assert (home / 'calls').read_text() == before
            session.send(b'a')
            session.wait(b'Expected physical Homebrew prefix')
            session.send(b'\r')
            session.wait(b'Install core formula')
            session.resize((14, 48))
            session.wait(b'native links')
            session.wait(b'No rollback')
            session.send(b'\r')
            session.wait(b'Installing package')
            session.send(b'\x03')
            session.wait(b'waiting for worker')
            (home / 'release').write_text('fixture')
            session.finish()
            assert b'PRIVATE_BREW' not in session.data
        assert (home / 'installed').exists()
        print('PASS Homebrew current-version/native-effects review, retained prefix and joined Ctrl-C')
    else:
        print('SKIP Homebrew installation PTY: declarations are macOS-only; explicit macOS fake-host core/TUI tests remain portable')
    damaged = home / '.local/state/dort/tools-attempts' / ('0' * 64)
    damaged.parent.mkdir(mode=0o700, exist_ok=True)
    damaged.mkdir(mode=0o700)
    before = (home / 'calls').read_text() if (home / 'calls').exists() else ''
    with Session([binary, 'tui'], scoped, (40, 120)) as session:
        session.wait(b'loaded.')
        session.navigate('Tools', 'Installation attempts [i]')
        session.wait(b'UNRESOLVED')
        session.send(b'c')
        session.wait(b'Trusted mise executable')
        session.closure_provider(2)
        session.wait(b'Trusted Homebrew executable')
        session.click_label('Trusted Homebrew executable')
        for value in (brew, bin_dir, prefix):
            session.send(b'\x15' + str(value).encode() + b'\t')
        session.send(b'\r')
        session.wait(b'Enter Inspect')
        assert ((home / 'calls').read_text() if (home / 'calls').exists() else '') == before
        session.resize((14, 48))
        session.wait(b'Enter Inspect')
        session.send(b'\r')
        session.wait(b'Explicitly closed')
        session.send(b'\x1bq')
        session.finish()
    assert (home / 'calls').read_text() == before + '--prefix\ninfo --json=v2 --installed --formula\n'
    print('PASS damaged Homebrew attempt retained three-field scope and compact fresh closure')


def tool_outcome_workflows(binary, base, env):
    home, repo, bin_dir, lookup = (base / name for name in ('outcome-home', 'outcome-repo', 'outcome-bin', 'outcome-lookup'))
    for directory in (home, repo, bin_dir, lookup):
        directory.mkdir(mode=0o700)
    (repo / '.git').mkdir()
    manifest = repo / 'dort.toml'
    manifest.write_text("schema=1\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nmacos={provider='mise',package='demo',version='1.2.3'}\nubuntu={provider='mise',package='demo',version='1.2.3'}\n")
    git = bin_dir / 'git'
    git.write_text('#!/bin/sh\nfor arg do\ncase "$arg" in\nconfig) exit 1;;\nrev-parse) pwd -P; exit 0;;\nstatus) printf "# branch.head main\\000"; exit 0;;\nesac\ndone\nexit 94\n')
    git.chmod(0o700)
    mise = bin_dir / 'mise'
    mise.write_text('''#!/bin/sh
if [ "$1" = ls ]; then
printf Q >> "$HOME/calls"
if [ -f "$HOME/installed" ]; then printf '{"demo":[{"version":"1.2.3","installed":true}]}'; else printf '{}'; fi
exit 0
fi
printf I >> "$HOME/calls"
printf PRIVATE_OUTCOME; printf PRIVATE_ERROR >&2
printf installed > "$HOME/installed"
exit 9
''')
    mise.chmod(0o700)
    scoped = dict(env, HOME=str(home), PATH=str(bin_dir), XDG_CONFIG_HOME=str(home / '.config'), XDG_STATE_HOME=str(home / '.local/state'))
    subprocess.run([binary, '--json', '--yes', 'init', str(repo)], env=scoped, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
    with Session([binary, 'tui'], scoped) as session:
        session.wait(b'loaded.')
        session.send(b'\x1b[D')
        session.wait(b'[x] mise')
        session.send(b'\r')
        session.wait(b'0 packages reported')
        session.send(b'p' + str(manifest).encode() + b'\t' + str(lookup).encode() + b'\t\t\r')
        session.wait(b'Compare')
        session.choose_snapshot(1)
        session.send(b'\r')
        session.wait(b'Package proposals/no-ops only')
        session.more_action('Review exact mise')
        session.wait(b'Enter Install')
        session.send(b'\r')
        session.wait(b'Process failed')
        session.wait(b'Package: exact version reported present')
        assert (home / 'calls').read_text() == 'QQIQ'
        attempts = home / '.local/state/dort/tools-attempts'
        before = {p.relative_to(attempts): p.read_bytes() for p in attempts.glob('*/*.json')}
        session.resize((14, 48))
        session.wait(b'Process failed')
        session.wait(b'reported present')
        session.click_action('Fresh review')
        session.wait(b'Fresh review')
        session.wait(b'no replay')
        assert (home / 'calls').read_text() == 'QQIQ', 'Fresh review ran an unapproved query'
        assert before == {p.relative_to(attempts): p.read_bytes() for p in attempts.glob('*/*.json')}
        session.send(b'\r')
        session.wait(b'Snapshot s2')
        assert (home / 'calls').read_text() == 'QQIQQ'
        session.resize((40, 120))
        session.wait(b'Snapshot s2')  # Wait for the grown frame, not stale pre-resize geometry.
        session.send(b'p')
        session.wait(b'Load declarations')
        session.click_action('Load declarations')
        session.wait(b'Compare')
        session.wait(b's1')  # A new catalog must not silently replace the old binding.
        session.click_label('demo (demo)')
        session.choose_snapshot(2)
        session.wait(b's2')
        session.send(b'\r')
        session.wait(b'Package no-op')
        session.send(b'i')  # A matching package is not an upgrade/reinstall request.
        assert (home / 'calls').read_text() == 'QQIQQ'
        session.send(b'\x1bq')
        session.finish()
        assert b'PRIVATE_OUTCOME' not in session.data and b'PRIVATE_ERROR' not in session.data
    assert before == {p.relative_to(attempts): p.read_bytes() for p in attempts.glob('*/*.json')}
    print('PASS failed process with package present, compact fresh-review click, new query approval and explicit new-ID binding without replay')


def apt_install_workflows(binary, base, env):
    home, repo, bin_dir, programs = (base / name for name in ('apt-home', 'apt-repo', 'apt-bin', 'apt-programs'))
    for directory in (home, repo, bin_dir, programs):
        directory.mkdir(mode=0o700)
    (repo / '.git').mkdir()
    manifest = repo / 'dort.toml'
    manifest.write_text("schema=1\n[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\nubuntu={provider='apt',package='demo'}\nmacos={provider='mise',package='demo',version='1.2.3'}\n")
    git = bin_dir / 'git'
    git.write_text('#!/bin/sh\nfor arg do\ncase "$arg" in\nconfig) exit 1;;\nrev-parse) pwd -P; exit 0;;\nstatus) printf "# branch.head main\\000"; exit 0;;\nesac\ndone\nexit 94\n')
    git.chmod(0o700)
    bodies = {
        'dpkg-query': 'printf D >> "$HOME/calls"; if [ -f "$HOME/installed" ]; then printf "demo\\t2.7\\tinstalled\\tok\\n"; fi',
        'apt-cache': 'printf C >> "$HOME/calls"; printf "demo:\\n  Installed: (none)\\n  Candidate: 2.7\\n"',
        'sudo': 'printf S >> "$HOME/calls"; shift 6; "$@"',
        'apt-get': '''printf I >> "$HOME/calls"
printf started > "$HOME/started"
printf PRIVATE_APT_OUTPUT; printf PRIVATE_APT_ERROR >&2
n=0
while [ ! -f "$HOME/release" ] && [ "$n" -lt 10000000 ]; do n=$((n+1)); done
printf installed > "$HOME/installed"''',
    }
    for name, body in bodies.items():
        program = programs / name
        program.write_text('#!/bin/sh\n' + body + '\n')
        program.chmod(0o700)
    scoped = dict(env, HOME=str(home), PATH=str(bin_dir), XDG_CONFIG_HOME=str(home / '.config'),
                  XDG_STATE_HOME=str(home / '.local/state'))
    subprocess.run([binary, '--json', '--yes', 'init', str(repo)], env=scoped,
                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
    if sys.platform == 'linux':
        with Session([binary, 'tui'], scoped, (40, 120)) as session:
            session.wait(b'loaded.')
            session.send(b'\x1b[D')
            session.wait(b'apt / dpkg packages')
            session.click_label('apt / dpkg packages')
            session.send(b'e')
            session.wait(b'Trusted executable')
            session.send(str(programs / 'dpkg-query').encode() + b'\t\x15' + str(bin_dir).encode() + b'\t\r')
            session.wait(b'Run query')
            assert not (home / 'calls').exists(), 'APT review executed a query'
            session.send(b'\r')
            session.wait(b'0 packages reported')
            session.send(b'p' + str(manifest).encode() + b'\t' + str(bin_dir).encode() + b'\t\t\r')
            session.wait(b'Compare')
            session.choose_snapshot(1)
            session.send(b'\r')
            session.wait(b'Package proposals/no-ops only')
            before = (home / 'calls').read_text()
            session.more_action('Review root-risk')
            session.wait(b'ROOT RISK')
            assert (home / 'calls').read_text() == before
            session.send(b'a')
            session.wait(b'Trusted sudo')
            session.send(b'\r')
            session.wait(b'Install with root risk')
            session.resize((14, 48))
            session.wait(b'CAN EXECUTE')
            session.send(b'\r')
            session.wait(b'Installing package')
            session.send(b'\x03')
            session.wait(b'waiting for worker')
            (home / 'release').write_text('fixture')
            session.finish()
            assert b'PRIVATE_APT' not in session.data
        assert (home / 'installed').exists()
        print('PASS APT root-risk review, retained explicit programs and joined Ctrl-C (fake nonprivileged supervisor)')
    else:
        print('SKIP APT installation PTY: declarations are Ubuntu-only; portable core/TUI use explicit Ubuntu fake-host contexts')
    damaged = home / '.local/state/dort/tools-attempts' / ('0' * 64)
    damaged.parent.mkdir(mode=0o700, exist_ok=True)
    damaged.mkdir(mode=0o700)
    before = (home / 'calls').read_text() if (home / 'calls').exists() else ''
    with Session([binary, 'tui'], scoped, (40, 120)) as session:
        session.wait(b'loaded.')
        session.navigate('Tools', 'Installation attempts [i]')
        session.wait(b'UNRESOLVED')
        session.send(b'c')
        session.wait(b'Trusted mise executable')
        session.closure_provider(3)
        session.wait(b'Trusted dpkg-query')
        session.click_label('Trusted dpkg-query')
        for value in (programs / 'dpkg-query', bin_dir):
            session.send(b'\x15' + str(value).encode() + b'\t')
        session.send(b'\r')
        session.wait(b'Enter Inspect')
        assert ((home / 'calls').read_text() if (home / 'calls').exists() else '') == before
        session.resize((14, 48))
        session.wait(b'Enter Inspect')
        session.send(b'\r')
        session.wait(b'Explicitly closed')
        session.send(b'\x1bq')
        session.finish()
    assert (home / 'calls').read_text() == before + 'D'
    print('PASS damaged APT closure uses only retained dpkg/PATH fields, never sudo authentication')


def interaction_surface_workflows(binary, base, env):
    for size in ((14, 48), (24, 80), (40, 140)):
        home = base / ('interaction-home-' + str(size[1]))
        home.mkdir(mode=0o700)
        scoped = dict(env, HOME=str(home), XDG_CONFIG_HOME=str(home / '.config'),
                      XDG_STATE_HOME=str(home / '.local/state'))
        with Session([binary, 'tui'], scoped, size) as session:
            session.wait(b'loaded.')
            session.send(b'\x1b[D')
            session.wait(b'Inspect installed tools')
            session.settle()
            assert b'\\u{b7}' not in session.screen.text(), 'Trusted decoration was metadata-escaped'
            assert '·'.encode() in session.screen.text(), 'UTF-8 chrome lost its actual separator'
            # A foreground-changing event and F2 arrive in one input burst.
            # More must use the newly rendered declaration, not cached Tools actions.
            session.send(b'a\x1bOQ')
            session.wait(b'Dependencies')
            session.wait(b'ACTION')
            assert b'Inspect selected' not in session.screen.text()
            session.send(b'\x1b')
            session.wait(b'Tool ID')
            session.send(b'q?/')
            session.wait(b'Tool ID')
            session.wait(b'q?/')
            session.settle()
            footer = session.screen.text().splitlines()[-1]
            assert b'Review' in footer and b'More' in footer and b'Help' in footer, 'Declaration strip describes the underlying Tools page'
            assert b'Inspect tools' not in footer and b'Snapshots' not in footer
            session.send(b'\x1b[21~')  # F10 opens a page menu even during text entry.
            session.wait(b'Overview')
            session.settle()
            session.send(b'\x1b')
            session.settle()
            session.wait(b'Tool ID')
            session.wait(b'q?/')
            session.send(b'\x1bOP')
            session.wait(b'Explicit provider inspection')
            session.settle()
            footer = session.screen.text().splitlines()[-1]
            assert b'Back' in footer and b'Review' not in footer, 'Help strip describes the background page'
            assert b'Review' not in footer
            session.send(b'\x1b[21~')
            session.wait(b'Dialog open:')
            session.mouse(0, 0, 0)  # Outside click cannot navigate or dismiss Help.
            session.settle()
            assert b'Explicit provider inspection' in session.screen.text() and b'Keys in this context' in session.screen.text(), 'Outside click dismissed the dialog'
            session.send(b'\x1b')
            session.settle()
            session.wait(b'Tool ID')
            session.wait(b'q?/')
            session.destination('Checks')  # A real menu destination is not silently ignored.
            session.wait(b'Checks')
            session.navigate('Tools', 'Inspect installed tools [c]')
            session.wait(b'Homebrew formulae')  # Join metadata discovery before input.
            session.settle()
            session.send(b'a')
            session.wait(b'Tool ID')
            session.wait(b'q?/')  # Actual navigation retains drafts, not approvals.
            session.send(b'\x1b')
            session.wait(b'No inspection collected yet.')
            session.settle()
            session.send(b'q')
            session.finish()
        assert not list(home.iterdir()), 'Page/dialog navigation created state'
        print('PASS page menus during text entry, retained draft, dialog feedback/outside capture and UTF-8 chrome', size)


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

        interaction_surface_workflows(binary, base, env)
        tools_workflows(binary, base, env)
        tool_install_workflows(binary, base, env)
        npm_install_workflows(binary, base, env)
        brew_install_workflows(binary, base, env)
        apt_install_workflows(binary, base, env)
        tool_outcome_workflows(binary, base, env)
        tools_reconciliation_workflows(binary, base, env)
        tool_declaration_workflows(binary, base, env)

        # Text carries navigation/state meaning even with a low-color terminal.
        with Session([binary, "--dry-run", "tui"], dict(env, TERM="vt100", NO_COLOR="1"), (14, 48)) as session:
            session.wait(b"loaded.")
            for label in [b"[Places]", b"Overview", b"DRY RUN", b"Create or attach", b"F2 More", b"F1 Help"]:
                session.wait(label)
            session.send(b"?")
            session.wait(b"Start with a repository")
            session.send(b"\x1b[6~" * 30)
            session.settle()
            assert b"Start with a repository" not in session.screen.text()
            session.send(b"\x1b[H")
            session.wait(b"Start with a repository")
            session.send(b"\x1b")
            session.wait(b"Create or attach")
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
                session.settle(); assert b'Keys in this context' not in session.screen.text()
                session.send(b'\x1b[C\r')  # Overview -> Files group, then expand at every width.
                session.wait(b'Discover')
                session.send(b'\x1b[B')
                session.settle()
                session.send(b'\x1bOP')
                session.wait(b'Choose a destination group')
                session.send(b'\r')  # Help cannot select the retained menu entry
                session.settle()
                session.send(b'\x1b')
                session.settle(); assert b'Keys in this context' not in session.screen.text()
                session.send(b'\r')
                session.wait(b'0 / 0 shown')
                session.wait(b'Files / Discover')
                session.send(b'12345')
                session.settle()
                assert b'Files / Discover' in session.screen.text(), 'Numeric aliases changed destination'
                session.send(b"\x1b[21~\r")  # F10: expand Files, then use its local shortcut.
                session.wait(b"Managed")
                session.send(b"m")
                session.wait(b"Files / Managed")
                session.send(b"\x1b[21~\x1b[C\x1b[C\r")  # Checks group, explicitly expanded.
                session.wait(b"Diagnostics")
                session.send(b"j")
                session.wait(b"Checks / Diagnostics")
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
                session.focused_field('ID')
                session.send(b'\x1b[21~')
                session.wait(b'Files >' if ascii_only else 'Files ▸'.encode())
                session.settle()
                assert not session.screen.cursor_visible, 'Places retained the background field caret'
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
            session.wait(b"Filter files")
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
            session.wait(b'Filter files')
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
            session.wait(b'Filter files')
            session.wait(b'[Places]')
            session.more_action('Ignore selected')
            session.wait(b'Ignore discovery path')
            assert not ignore_store.exists(), 'Ignore preview wrote preferences'
            session.send(b'\x1bOP')
            session.wait(b'Keys in this context')
            session.send(b'\r')
            session.settle()
            assert not ignore_store.exists(), 'Help approved a preview'
            session.send(b'\x1b')
            session.wait(b'Ignore discovery path')
            session.send(b'\r')
            session.wait(b'Confirm reviewed changes')
            session.send(b'ye\x1bOP')
            session.wait(b'Keys in this context')
            session.send(b's\r')
            session.settle()
            assert not ignore_store.exists(), 'Help submitted confirmation input'
            session.send(b'\x1b')
            session.wait(b'Confirm reviewed changes')
            session.send(b's')  # Printable text alone cannot approve.
            session.send(b'\x1bOP')
            session.wait(b'Keys in this context')
            session.resize((13,47))
            session.settle()
            session.resize((14,48))
            session.wait(b'Keys in this context')
            session.send(b'\x1b')  # Help closes to preview; fresh confirmation required
            session.wait(b'Ignore discovery path')
            session.send(b'\r')
            session.wait(b'Confirm reviewed changes')
            session.send(b'\r')
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
            session.send(b'f')
            session.wait(b'Filter files')
            session.navigate('Files', 'Ignored paths [U]')
            session.wait(b'Ignored exact paths')
            assert b'Enter' in session.screen.text().splitlines()[-1]
            session.send(b'\r')
            session.wait(b'Unignore discovery path')
            session.send(b'\r')
            session.wait(b'Confirm')
            session.send(b'\r')
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
            session.focused_field('Path')
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
            session.focused_field('Path')
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
            session.focused_field('Manifest path')
            session.send(os.fsencode(standalone) + b"\r")
            session.wait(b"Declarations validated")
            session.wait(b"Checks / Manifest")
            session.wait(b"Valid")
            session.wait(b"Declarations validated")
            session.send(b"\x1b[6~" * 6)
            session.wait(b"Capability limits")
            session.send(b"\x1b[H")
            session.wait(b"Structural check only")
            session.send(b"j")
            session.wait(b"Checks / Diagnostics")
            session.send(b"m")
            session.wait(b"Checks / Manifest")
            session.send(b"v\x10")  # Open Profiles directly without a footer focus stop.
            session.wait(b"PROFILE")
            session.send(b"\x1b[B \r")  # Toggle dev explicitly, then apply from that row.
            session.focused_field('Manifest path')
            session.send(b"\r")  # Validate directly from the retained path field.
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
            session.send(b"\x1bOR")  # F3 opens explicit scrollable choice metadata.
            session.wait(b"The exact root becomes")
            session.send(b"\x1b[6~\x1b[H")
            session.wait(b"The exact root becomes")
            session.send(b"\x1b")
            session.wait(b"REPOSITORY")
            session.send(b"\x0e")  # Ctrl-N opens Create/attach; no footer focus.
            session.focused_field('Path')
            session.send(b"\x1b")
            session.settle()
            session.send(b"v")
            session.focused_field('Manifest path')
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
            session.focused_field('Manifest path')
            session.send(b"\x1b")
            session.settle()
            session.send(b"q")
            session.finish()
        assert not list(home.iterdir()), "Read-only compact pickers persisted state"
        print("PASS compact typed pickers, details/Home and session-only validation")

        with Session([binary, "tui"], env, size=(14, 48)) as session:
            session.wait(b"loaded.")
            session.send(b";")
            session.wait(b"Find an action")
            session.send(b"q?/")
            session.wait(b"No matching actions")
            session.send(b"\x15rollback\r")
            session.wait(b"No matching actions")  # other-view commands stay in their view
            session.send(b"\x1bOP")  # F1 works even in Actions search
            session.wait(b"Find a task by name.")
            session.send(b"\r")  # Help never dispatches
            session.send(b"\x1b")
            session.wait(b"rollback")  # exact query retained
            session.send(b"\x1b")  # leave search
            session.settle()
            session.send(b"\x1b")  # close
            session.settle()
            session.destination("Recovery")
            session.send(b";rollback\r")  # disabled Enter in its relevant view
            session.wait(b"x Preview rollback")
            session.send(b"\x1bOR")  # Full metadata/reasons are an explicit dialog subview.
            session.wait(b"Action details")
            session.send(b"\x1b[6~")
            session.settle()
            assert b"Action details" in session.screen.text(), "Paging replaced the metadata subview"
            reached = False
            for _ in range(30):
                if b"transaction." in session.screen.text(): # final wrapped reason row
                    reached = True
                    break
                session.send(b"\x1b[6~")
                session.settle()
            assert reached, "Compact Actions prerequisites were not keyboard reachable"
            session.send(b"\x1b[H")
            session.wait(b"Action details")
            session.send(b"\x1b")  # metadata -> choices
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
            session.wait(b"About")
            session.send(b"\x1b")
            session.settle()
            session.send(b"q")
            session.finish()
        assert not list(home.iterdir()), "Browsing Actions or refusing an action created state"
        print("PASS compact search-first contextual Actions and full disabled reasons without dispatch")

        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.navigate("Checks", "Diagnostics [j]")
            session.wait(b"Checks / Diagnostics")
            session.wait(b"Registered repositories: 0")
            session.send(b"r")
            session.wait(b"Diagnostics | Dort")
            session.settle()  # identical refresh result may still be the prior frame
            session.navigate("Session", "About")
            session.wait(b"About")
            session.wait(b"Guarded create/attach")
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
            session.wait(b"All-repository transactions: 0")
            session.wait(b"Global write blockers: 0")
            session.send(b"\x1b[6~")
            session.wait(b"Configuration unavailable")
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
            session.wait(b"Eligible hint")
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
            session.wait(b"Eligible hint")
            session.more_action('Adopt selected')  # The real contextual action, not explanatory prose.
            session.focused_field('TARGET')
            session.click_label("ID")
            session.send(b"\x15mouse-id")
            session.wait(b"mouse-id")
            session.send(b"\x1b")
            session.settle()
            session.send(b"\r")  # Explicit inspection, independent of earlier paging/focus
            session.resize((14, 48))
            session.wait(b"Selected content")
            session.send(b"\x1b")
            session.wait(b"NAME")
            session.send(b"\x1b[C")  # direct destination, never action selection
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
            session.send(b'f')
            session.wait(b'focus-start-marker')
            session.send(b'\r')  # Explicit reader, not a Tab focus stop.
            session.settle()
            session.send(b'j\x06')  # Content row + full rendered page, not file selection.
            session.settle()
            assert b'second-file-marker' not in session.screen.text()
            session.send(b'G')
            session.wait(b'focus-end-marker')
            session.send(b'gg')
            session.wait(b'focus-start-marker')
            session.resize((14, 48))
            session.wait(b'Selected content')
            session.send(b'G')
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
            session.send(b"f")
            session.wait(b"Eligible hint")
            session.send(b"\r")
            session.wait(b"Selected content")
            session.send(b"e")  # direct, separate Adopt selected action
            session.focused_field('TARGET')
            session.send(b"\t\t\t\r")  # fields -> Review, no repository selected
            session.wait(b"Read failed:")
            session.click_action('Retry read')  # Fresh read/preview intent, never execution.
            session.settle()
            session.wait(b"Read failed:")
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
        with Session([binary, "tui"], dict(env, PATH=str(bin_dir))) as session:
            session.wait(b"loaded.")
            session.navigate("Git", "Choose Git scope [g]")
            session.send(os.fsencode(git_only) + b"\r")
            session.wait(b"PATH")
            session.wait(b"q?name")
            session.send(b"/q?/\x1b")  # literal text, no quit/help/actions
            session.wait(b"0 displayed")
            session.send(b"\x1b[3~u")  # clear display filter; unstaged summary
            session.wait(b"Unstaged (worktree vs index)")
            session.wait(b"1 entries")
            session.send(b";View patch content")
            session.wait(b"Find an action")
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
            session.send(b"w")
            session.wait(b"May expose secrets")
            session.settle()
            assert not (base / "patch-calls").exists(), "Warning opened a patch automatically"
            session.send(b"\x1b")
            session.wait(b"1 entries")
            session.send(b"w\r")
            session.wait(b"PRIVATE-PATCH\\x1b[31m")
            session.send(b"\x1b")
            session.wait(b"1 entries")
            session.send(b"r")
            session.wait(b"PATH")
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
            session.send(b"\x1b[3~r")
            session.settle()  # identical summaries can otherwise match the prior frame
            session.wait(b"1 entries")
            session.wait(b"PATH")
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
            session.navigate("Git", "Unstaged [u]")
            session.wait(b"Unstaged (worktree vs index)")
            session.wait(b"100644 -> 100644")  # loaded diff before content action
            session.more_action('View patch')
            session.wait(b"May expose secrets")
            session.mouse(0, 20, 8)  # Only the labeled disclosure control can approve.
            session.send(b"yes")  # Text alone is not an activation.
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
                session.focused_field('Path')
                session.send(os.fsencode(created) + b"\r")
                session.wait(b"Review effects before confirming")
                session.send(b"\x1b")
                session.settle()
                assert not list(onboarding_home.iterdir()), "Preview/back created local state"
                if intent == "create":
                    assert not created.exists(), "Preview created repository"
                session.send(b"\r")  # retained draft -> fresh preview
                session.settle()
                session.send(b"\r")
                session.wait(b"Confirm")
                session.send(b"\r")
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
        target = home / ".editor"
        state = home / ".local/state"
        with Session([binary, "tui"], env) as session:
            session.wait(b"loaded.")
            session.send(b"a")
            session.wait(b"Review effects before confirming")
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
            session.wait(b"Review effects before confirming")
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
            session.wait(b"Review effects before confirming")
            session.send(b"\r")
            session.wait(b"Confirm")
            session.send(b"\r")
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
            session.wait(b"Review effects before confirming")
            session.send(b"\r")
            session.wait(b"Confirm")
            session.send(b"\r")
            session.wait(b"Operation result")
            session.wait(b"Everything is up to date.")
            session.more_action('Refresh observations')
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
                session.send(b"\r")
                session.wait(b"Confirm")
                session.send(b"\r")
                session.wait(b"Possible partial effects")
                session.send(b"\x1b[6~" * 4)
                session.settle()
                assert b"Possible partial effects" in session.screen.text(), "Risk scrolled out of view"
                assert b"Retry execution" not in session.screen.text()
                session.more_action('Refresh observations')  # Must work while the writer lock is held.
                session.wait(b"Earlier write failed:")
                session.destination("Recovery")
                session.wait(b"Global blockers: 0")
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
            session.wait(b"Repository ID:")
            session.wait(b"Transaction ID:")
            session.send(b"b")
            session.wait(b"Rollback schema 2")
            session.wait(b"Inverse paths:")
            session.send(b"\x1b")
            session.wait(b"1/1 shown")
            assert target.is_symlink(), "Preview/back performed rollback"
            session.send(b"b")
            session.wait(b"Rollback schema 2")
            session.send(b"\r")
            session.wait(b"Confirm")
            session.send(b"\r")
            session.wait(b"Operation result")
            assert not target.exists() and not target.is_symlink()
            # Return from the retained receipt before opening navigation menus.
            session.click_action('Back')
            # A new action needs a new actual preview and confirmation.
            session.destination("Overview")
            session.send(b"a")
            session.wait(b"Review effects before confirming")
            session.send(b"\r")
            session.wait(b"Confirm")
            session.send(b"\r")
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
            session.wait(b"Global blockers: 0")
            session.wait(b"1/2 shown")
            session.wait(b"Completed")
            session.send(b"\r")  # selected row inspects; never requests rollback
            session.wait(b"Journal facts")
            session.wait(b"Repository ID:")
            session.send(b"\x1b[6~" * 4)
            session.wait(b"Validated checkpoints:")
            session.send(b"\x1b[H")
            session.wait(b"Repository ID:")
            session.more_action('Review rollback')  # same typed action as b; still preview only
            session.wait(b"Rollback schema 2")
            session.send(b"\r")
            session.settle()
            assert b"Confirm" not in session.data, "Dry-run Recovery opened approval"
            session.send(b"\x1b")
            session.wait(b"Journal facts")
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
            session.send(b"\r")
            session.wait(b"Journal facts")
            session.mouse(65, 90, 10)
            session.settle()
            scrolled = session.screen.text()
            session.send(b"\t\x1b[Z")
            session.settle()
            assert scrolled == session.screen.text(), "Tab changed Recovery detail scroll"
            session.send(b"\x1b[H")
            session.wait(b"Transaction identity")
            session.resize((14, 48))
            session.wait(b"Journal facts")
            session.send(b"\x1b")
            session.wait(b"TX ID")
            session.click_label("Completed", min_row=5)
            session.send(b"\r")
            session.wait(b"Journal facts")
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
            session.wait(b"Enter Continue")
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
            session.focused_field('TARGET')
            session.send(b"~/.q?config\tconfig\tfiles/config\r")  # Fields only; Enter previews.
            session.wait(b"Enter Adopt all")
            session.send(b"\x1b")
            session.settle()
            assert not candidate.is_symlink() and not (repo / "files/config").exists()
            session.send(b"\r")  # retained Review -> fresh preview
            session.settle()
            session.send(b"\r")  # Adopt all: one explicit approval, no typed yes.
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
            session.send(b";delete managed link: enter\r")
            session.focused_field('Exact ID')
            session.send(b"config\r")
            session.wait(b"Enter Continue")
            session.send(b"\r")
            session.wait(b"Delete managed link for ID:")
            assert candidate.is_symlink(), "Confirmation executed without activation"
            session.send(b"\r")
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
            session.send(b'f')
            session.wait(b'Filter files')
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
            assert b'ADOPTION' not in session.screen.text(), 'Batch editor leaked the Discover table underneath'
            assert b'ordinary settings' not in session.screen.text(), 'Batch editor leaked selected-file content'
            session.send(b'\x1bOR')  # F3 opens scrollable full selected-item paths.
            session.wait(b'Selected draft metadata')
            session.wait(b'files/.config/bundle/only-a')
            session.send(b'\x1b[H\x1b')
            session.settle()
            session.wait(b'All editable defaults')
            session.send(b'je')  # Select only-b and open a separate item form.
            session.wait(b'Edit item')
            session.send(b'\t\x15edited-b\r')  # TARGET -> ID; Enter retains edits and returns to the list.
            session.wait(b'Review all')
            session.send(b'e')
            session.wait(b'edited-b')
            session.click_label('Done')  # Return only; never review/adopt an individual item.
            session.wait(b'Review all')
            session.click_label('Review all')
            session.wait(b'Exact marked set')
            session.send(b'\r')  # Adopt all consumes the real combined preview once.
            session.wait(b'Enter Details')
            assert all((targets / name).is_symlink() for name in ('only-a', 'only-b'))
            for name in ('only-a', 'only-b'):
                assert (targets / name).resolve() == batch_repo / 'files/.config/bundle' / name
            batch_heads(batch_home, 1)
            session.destination("Recovery")
            session.send(b'b')
            session.wait(b'Rollback schema 4')
            session.send(b'\r')
            session.wait(b'Confirm')
            session.send(b'\r')
            session.wait(b'Operation result')
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
                session.wait(b'Exact marked set')
                session.send(b'\r')
                session.wait(b'Confirm')
                session.send(b'\r')
                session.wait(b'Operation result')
                assert all((targets / name).is_symlink() for name in ('only-a', 'only-b'))
                assert not (targets / 'only-c').is_symlink()
                batch_heads(batch_home, 1)
                session.send(b'\x1b')
                session.settle()
                session.send(operation.encode())
                session.wait(b'Exact marked set')
                session.send(b'\r')
                session.wait(b'Confirm')
                if operation == 'D':
                    session.wait(b'only-a')
                    session.wait(b'only-b')
                    assert all((targets / name).is_symlink() for name in ('only-a', 'only-b'))
                    session.resize((14, 47))
                    session.wait(b'terminal too small')
                    session.resize((24, 80))
                    session.wait(b'Exact marked set')
                    session.settle()
                    session.send(b'\r')  # Fresh confirmation, not execution.
                    session.wait(b'Confirm')
                session.send(b'\r')
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
            session.wait(b"Enter Continue")
            session.send(b'\r')
            session.wait(b'Confirm')
            session.send(b'\r')
            session.wait(b'Operation result')
            session.send(b'\x1b')
            session.settle()
            session.send(b'ljlj')
            session.click_label('only-a')
            session.send(b' u')  # Mark A, then Keep selected with the advertised key.
            session.wait(b"Enter Continue")
            session.send(b'\r')
            session.wait(b'Confirm')
            session.send(b'\r')
            session.wait(b'File operation completed.')
            session.settle()  # Complete the joined read refresh before leaving its receipt.
            session.send(b'\x1b')
            session.wait(b'0 marked (0 hidden)')
            session.click_label('only-b')
            session.send(b' K')
            session.wait(b'Exact marked set')
            assert b'only-a' not in session.screen.text(), 'Completed Keep leaked into a later batch'
            session.send(b'\r')
            session.wait(b'Confirm')
            session.send(b'\r')
            session.wait(b'Batch file operation completed.')
            session.settle()
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
