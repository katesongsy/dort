"""Disposable Tools PTY journeys using the public declaration/discovery workflows."""
import subprocess


def program(path, body):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text('#!/bin/sh\n' + body + '\n')
    path.chmod(0o700)


def fixture(binary, base, env, name, provider='brew', declarations=True):
    root = base / name
    root.mkdir(mode=0o700)
    home, repo, bin_dir = (root / part for part in ('h', 'r', 'b'))
    for directory in (home, repo, bin_dir):
        directory.mkdir(mode=0o700)
    scoped = dict(env, HOME=str(home), PATH=str(bin_dir),
                  XDG_CONFIG_HOME=str(home / '.config'), XDG_STATE_HOME=str(home / '.local/state'))
    (repo / '.git').mkdir()
    (repo / 'dort.toml').write_text('schema=1\n')
    program(bin_dir / 'git', '''for arg do
case "$arg" in
config) exit 1;;
rev-parse) pwd -P; exit 0;;
status) printf '# branch.head main\\000'; exit 0;;
esac
done
exit 94''')
    subprocess.run([binary, '--json', '--yes', 'init', str(repo)], env=scoped,
                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
    if declarations:
        runtime = ("[[tools]]\nid='runtime'\nbinary='node'\n[tools.install]\n"
                   "provider='brew'\npackage='node'\n") if provider == 'npm' else ''
        spec = {'npm': "provider='npm'\npackage='demo'\nversion='1.2.3'\nruntime_tool='runtime'",
                'brew': "provider='brew'\npackage='demo'"}[provider]
        (repo / 'dort.toml').write_text('schema=1\n' + runtime +
            "[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\n" + spec + '\n')
    prefix = home / 'prefix'
    for part in ('Homebrew/bin', 'Homebrew/Library', 'Cellar', 'bin', 'etc', 'include', 'lib', 'opt', 'sbin', 'share', 'var/homebrew'):
        (prefix / part).mkdir(parents=True, exist_ok=True)
    brew = prefix / 'Homebrew/bin/brew'
    program(brew, '''case "$*" in
'help install') printf '%s' '--formula --no-ask --skip-post-install HOMEBREW_NO_INSTALL_UPGRADE HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK HOMEBREW_NO_INSTALL_CLEANUP';;
'--prefix') printf '%s\\n' "$HOME/prefix";;
'--repository') printf '%s\\n' "$HOME/prefix/Homebrew";;
'--cellar') printf '%s\\n' "$HOME/prefix/Cellar";;
'info --json=v2 --installed --formula')
 printf Q >> "$HOME/calls"
 printf '{"formulae":['
 sep=''
 for package in demo other node; do
  marker="$HOME/$package"; [ "$package" != demo ] || marker="$HOME/installed"
  if [ -f "$marker" ]; then
   version=2.7; [ "$package" != node ] || version=24.0.0
   printf '%s{"name":"%s","full_name":"%s","tap":"homebrew/core","installed":[{"version":"%s"}]}' "$sep" "$package" "$package" "$version"
   sep=,
  fi
 done
 printf ']}';;
'info --json=v2 --formula '*)
 for arg do :; done
 package=${arg##*/}
 marker="$HOME/$package"; [ "$package" != demo ] || marker="$HOME/installed"
 printf '{"formulae":[{"name":"%s","full_name":"%s","tap":"homebrew/core","disabled":false,"versions":{"stable":"2.7"},"installed":[' "$package" "$package"
 if [ -f "$marker" ]; then printf '{"version":"2.7"}'; fi
 printf ']}]}';;
'install --formula --no-ask --skip-post-install '*)
 printf I >> "$HOME/calls"
 if [ -f "$HOME/hold" ]; then while [ ! -f "$HOME/release" ]; do :; done; fi
 for arg do :; done
 case "$arg" in */other) printf installed > "$HOME/other";; *) printf installed > "$HOME/installed";; esac
 printf PRIVATE_PROVIDER_OUTPUT
 if [ -f "$HOME/fail" ]; then exit 9; fi;;
*) exit 99;;
esac''')
    (bin_dir / 'brew').symlink_to(brew)
    if provider == 'npm':
        runtime_bin = home / 'rt/bin'
        (home / 'node').write_text('installed')
        program(runtime_bin / 'node', '''shift
if [ "$1" = prefix ]; then
 printf P >> "$HOME/calls"; printf '%s\\n' "$HOME/globals"; exit 0
fi
if [ "$1" = ls ]; then
 printf Q >> "$HOME/calls"
 if [ -f "$HOME/installed" ]; then printf '{"dependencies":{"demo":{"version":"1.2.3"}}}'; else printf '{"dependencies":{}}'; fi
 exit 0
fi
printf I >> "$HOME/calls"; printf installed > "$HOME/installed"; printf PRIVATE_PROVIDER_OUTPUT''')
        cli = home / 'rt/lib/node_modules/npm/bin/npm-cli.js'
        cli.parent.mkdir(parents=True)
        cli.write_text('PRIVATE_CLI_BODY')
        program(runtime_bin / 'npm', 'exec "$HOME/rt/bin/node" "$HOME/rt/lib/node_modules/npm/bin/npm-cli.js" "$@"')
        for name in ('git', 'brew'):
            (runtime_bin / name).symlink_to(bin_dir / name)
        scoped['PATH'] = str(runtime_bin)
    return home, repo, bin_dir, scoped


def calls(home):
    path = home / 'calls'
    return path.read_text() if path.exists() else ''


def check(session, home):
    before = calls(home)
    session.send(b'C')
    session.wait(b'Review installed checks')
    session.wait(b'y Run checks')
    session.send(b'\r\r')
    session.settle()
    assert calls(home) == before, 'Check preparation executed a package manager'
    session.send(b'y')
    session.wait(b'Missing')
    session.settle()


def normal_installs(Session, binary, base, env):
    providers = ['brew', 'npm']
    for provider in providers:
        home, repo, bin_dir, scoped = fixture(binary, base, env, 'i-' + provider, provider)
        with Session([binary, 'tui'], scoped, (24, 80)) as session:
            session.wait(b'loaded.')
            session.send(b't')
            session.wait(b'Not checked')
            if provider == 'npm':
                session.send(b'\x1b[B')  # The runtime declaration precedes demo in this fixture.
            session.send(b'\r')
            session.wait(b'Tool details')
            assert calls(home) == '', 'Opening a tool executed a check'
            session.send(b'\r\x1bOR')  # Neither Enter nor obsolete F3 executes a hidden action.
            session.settle()
            assert calls(home) == ''
            if provider == 'npm':
                session.send(b's')
                session.wait(b'Global package prefix')
                session.send(b'\t\t\t\t\x15' + str(home / 'globals').encode() + b'\r')
                session.wait(b'Tool details')
                assert calls(home) == '', 'Saving settings executed Node/npm'
            session.send(b'\x1b')
            check(session, home)
            before = calls(home)
            session.send(b'\r')
            session.wait(b'Tool details')
            assert calls(home) == before, 'List Enter changed from Details to Install'
            session.send(b'i')
            session.wait(b'Review missing installs')
            session.wait(b'Runs package-manager')
            session.settle()
            assert calls(home) == before, 'Installation preparation executed a package manager'
            assert b'root hooks' not in session.screen.text()
            session.resize((14, 48))
            session.wait(b'y Install')
            session.wait(b'F1 Help')
            assert b'F1 Help' in session.screen.text().splitlines()[-1]
            session.resize((24, 80))
            session.wait(b'y Install missing')
            session.send(b'\r\r')
            session.settle()
            assert calls(home) == before, 'Enter approved installation'
            session.send(b'yy')
            session.wait(b'Install results')
            session.settle()
            session.send(b'\r')
            session.wait(b'Tool details')
            session.send(b'\x1b')
            session.wait(b'Installed')
            session.send(b'3')
            session.wait(b'Installation history')
            before = calls(home)
            session.send(b'\r')
            session.wait(b'Installation details')
            assert calls(home) == before, 'History details executed a package manager'
            session.send(b'q')
            session.finish()
            assert b'PRIVATE_PROVIDER_OUTPUT' not in session.data and b'PRIVATE_CLI_BODY' not in session.data
        assert (home / 'installed').exists()
        print('PASS simple declared check/install, stable Enter, compact exact approval and history', provider)


def batch_guards(Session, binary, base, env):
    for mode in ('failure', 'joined', 'dry-run', 'attention'):
        home, repo, bin_dir, scoped = fixture(binary, base, env, 'b-' + mode)
        # Two independent declarations share one reviewed scope and one serial approval.
        path = repo / 'dort.toml'
        text = path.read_text()
        path.write_text(text + text[text.index('[[tools]]'):].replace("id='demo'", "id='other'").replace("binary='demo'", "binary='other'").replace("package='demo'", "package='other'"))
        if mode == 'attention':
            for binary_name in ('demo', 'other'):
                program(bin_dir / binary_name, 'exit 0')
        if mode == 'failure':
            (home / 'fail').write_text('fixture')
        if mode == 'joined':
            (home / 'hold').write_text('fixture')
        args = [binary, '--dry-run', 'tui'] if mode == 'dry-run' else [binary, 'tui']
        with Session(args, scoped) as session:
            session.wait(b'loaded.')
            session.send(b'tC')
            session.wait(b'Review installed checks')
            assert calls(home) == ''
            if mode == 'dry-run':
                session.send(b'\r\ry')
                session.settle()
                assert calls(home) == ''
                session.resize((13, 47))
                session.wait(b'terminal too small')
                session.resize((14, 48))
                session.wait(b'Not checked')
                session.send(b'q')
                session.finish()
                print('PASS declared dry-run and shrink remain process-free')
                continue
            session.send(b'y')
            session.wait(b'Missing')
            session.settle()
            assert calls(home) == 'Q'
            session.send(b'i')
            if mode == 'attention':
                session.wait(b'Installation needs attention')
                session.wait(b'A command exists')
                assert b'Runs package-manager' not in session.screen.text() and b'root hooks' not in session.screen.text()
                session.send(b'e')
                session.wait(b'Edit tool')
                session.send(b'\x1bq')
                session.finish()
                assert calls(home) == 'Q'
                print('PASS missing tools show actionable reasons and Edit without execution warnings')
                continue
            session.wait(b'Review missing installs')
            session.send(b'y')
            if mode == 'joined':
                session.wait(b'Installing package')
                session.send(b'\x03')
                session.wait(b'waiting for the worker')
                (home / 'release').write_text('fixture')
                session.finish()
            else:
                session.wait(b'Install results')
                session.send(b'\x1b[6~' * 12)
                session.wait(b'Not started')
                session.send(b'q')
                session.finish()
            assert b'PRIVATE_PROVIDER_OUTPUT' not in session.data
        assert calls(home).count('I') == (2 if mode == 'joined' else 1), 'Approved batch was replayed or did not stop on failure'
        print('PASS serial combined approval and joined worker', mode)


def discovery_declarations(Session, binary, base, env):
    home, repo, bin_dir, scoped = fixture(binary, base, env, 'discovery', declarations=False)
    program(bin_dir / 'brew', '''printf Q >> "$HOME/calls"; printf '{"formulae":[{"name":"observed","full_name":"observed","tap":"homebrew/core","installed":[{"version":"1.2.3"}]}]}' ''')
    with Session([binary, 'tui'], scoped, (14, 48)) as session:
        session.wait(b'loaded.')
        session.send(b't2')
        session.wait(b'Inspect installed tools')
        assert calls(home) == ''
        session.send(b'y')
        session.wait(b'observed')
        session.settle()
        session.send(b'\r')
        session.wait(b'Package')
        before = calls(home)
        session.send(b'\r')
        session.settle()
        assert calls(home) == before == 'Q'
        session.send(b'a')
        session.wait(b'Tool ID')
        session.send(b'\r')
        session.wait(b'Add tool declaration')
        session.ready_review()
        assert 'observed' not in (repo / 'dort.toml').read_text()
        session.send(b'y')
        session.wait(b'Installed packages')
        session.wait(b'PACKAGE / MANAGER')
        session.wait(b'observed')
        manifest = (repo / 'dort.toml').read_text()
        assert 'id = "observed"' in manifest and 'binary = "observed"' in manifest
        assert calls(home) == 'Q', 'Adding a declaration replayed inspection'
        session.navigate('Tools', 'Declared tools [1]')
        session.wait(b'observed')
        session.send(b'e')
        session.wait(b'Edit tool')
        session.send(b'\t\x15changed-command\r')
        session.wait(b'Edit tool declaration')
        session.ready_review()
        session.send(b'y')
        session.wait(b'Tool declaration completed.')
        session.settle()
        session.send(b'\r')
        session.wait(b'observed')
        session.send(b'd')
        session.wait(b'Remove tool declaration')
        session.ready_review()
        session.send(b'y')
        session.wait(b'Tool declaration completed.')
        session.settle()
        session.send(b'\r')
        session.wait(b'No declared tools')
        session.send(b'q')
        session.finish()
    assert calls(home) == 'Q' and 'observed' not in (repo / 'dort.toml').read_text()
    print('PASS current installed inspection, package details, prefilled declaration, edit/remove and separate approvals')
    with Session([binary, '--dry-run', 'tui'], scoped) as session:
        session.wait(b'loaded.')
        session.send(b't2')
        session.wait(b'Inspect installed tools')
        session.send(b'\r\ry')
        session.settle()
        assert calls(home) == 'Q'
        session.send(b'q')
        session.finish()
    print('PASS installed discovery dry-run cannot execute providers')


def damaged_history(Session, binary, base, env):
    home, repo, bin_dir, scoped = fixture(binary, base, env, 'closure')
    for part in ('.local', '.local/state', '.local/state/dort', '.local/state/dort/tools-attempts'):
        (home / part).mkdir(mode=0o700, exist_ok=True)
    damaged = home / '.local/state/dort/tools-attempts' / ('0' * 64)
    damaged.mkdir(mode=0o700)
    with Session([binary, 'tui'], scoped, (14, 48)) as session:
        session.wait(b'loaded.')
        session.send(b't3')
        session.wait(b'Unknown tool')
        session.send(b'\r')
        session.wait(b'Installation details')
        session.send(b'x')
        session.wait(b'Package manager program')
        session.send(b'\t' + str((bin_dir / 'brew').resolve()).encode() + b'\t' + str(bin_dir).encode() + b'\t' + str(home / 'prefix').encode() + b'\r')
        session.wait(b'Close installation record')
        session.ready_review()
        assert calls(home) == ''
        session.send(b'y')
        session.wait(b'Explicitly closed')
        session.send(b'q')
        session.finish()
    assert calls(home) == 'Q', 'Record closure replayed an installation'
    print('PASS damaged installation record asks only necessary scope and confirms fresh-query closure once')


def npm_without_repository(Session, binary, base, env):
    home, repo, bin_dir, scoped = fixture(binary, base, env, 'npm-path', 'npm', declarations=False)
    (home / '.config/dort/local.toml').unlink()
    (repo / 'dort.toml').write_text('PRIVATE_INVALID_MANIFEST')
    (home / 'rt/bin/brew').unlink()
    (home / 'installed').write_text('installed')
    with Session([binary, 'tui'], scoped, (24, 80)) as session:
        session.wait(b'loaded.')
        session.send(b't2')
        session.wait(b'Inspect installed tools')
        session.wait(b'prefix --global')
        assert calls(home) == '', 'Preparing PATH inventory executed npm'
        session.send(b'y')
        session.wait(b'demo')
        session.settle()
        session.send(b'\r')
        session.wait(b'Global prefix')
        session.wait(b'npm-path/h/globals')
        assert str(home / 'globals').encode() in b''.join(line.strip() for line in session.screen.text().splitlines())
        session.send(b'q')
        session.finish()
    assert calls(home) == 'PQ', 'PATH inventory did not query the configured prefix exactly once'
    assert b'PRIVATE_INVALID_MANIFEST' not in session.data and b'PRIVATE_CLI_BODY' not in session.data
    print('PASS npm PATH inventory without a repository, with reviewed prefix discovery and actual global scope')


def blocked_operation_error(Session, binary, base, env):
    import hashlib
    home, repo, bin_dir, scoped = fixture(binary, base, env, 'blocked-operation')
    repository_id = hashlib.sha256(str(repo).encode()).hexdigest()
    transaction_id = 'a' * 64
    transactions = home / '.local/state/dort/repositories' / repository_id / 'transactions'
    transactions.mkdir(parents=True, mode=0o700, exist_ok=True)
    original_transactions = {entry.name for entry in transactions.iterdir()}
    damaged = transactions / transaction_id
    damaged.mkdir(mode=0o700)
    record = damaged / '000000.json'
    payload = 'PRIVATE_MALFORMED_RECOVERY_RECORD'
    record.write_text(payload)
    record.chmod(0o600)
    original = (repo / 'dort.toml').read_bytes()
    with Session([binary, 'tui'], scoped, (14, 48)) as session:
        session.wait(b'loaded.')
        session.send(b'td')
        session.wait(b'Remove tool declaration')
        session.ready_review()
        session.send(b'y')
        session.wait(b'Changes blocked')
        session.wait(b'An earlier operation needs attention')
        assert b'Possible partial effects' not in session.screen.text()
        assert b'Write failed' not in session.screen.text()
        session.send(b'\x1b[6~' * 10)
        session.settle()
        assert b'An earlier operation needs attention' in session.screen.text()
        session.send(b'\r')
        session.wait(b'Recovery')
        session.wait(b'Damaged')
        session.send(b'q')
        session.finish()
    assert (repo / 'dort.toml').read_bytes() == original
    assert record.read_text() == payload and calls(home) == ''
    assert {entry.name for entry in transactions.iterdir()} == original_transactions | {transaction_id}
    assert payload.encode() not in session.data
    print('PASS blocked earlier operation has clear scope/recovery guidance without a false new-write warning or mutation')


def tools_workflows(Session, binary, base, env):
    normal_installs(Session, binary, base, env)
    batch_guards(Session, binary, base, env)
    discovery_declarations(Session, binary, base, env)
    npm_without_repository(Session, binary, base, env)
    damaged_history(Session, binary, base, env)
    blocked_operation_error(Session, binary, base, env)


def interaction_surface_workflows(Session, binary, base, env):
    for size in ((14, 48), (24, 80), (40, 140)):
        home, repo, bin_dir, scoped = fixture(binary, base, env, 'controls-' + str(size[1]))
        with Session([binary, 'tui'], scoped, size) as session:
            session.wait(b'loaded.')
            session.send(b't')
            for label in (b'1 Declared', b'2 Installed', b'3 History'):
                session.wait(label)
            session.send(b'3')
            session.wait(b'Installation history')
            session.wait(b'3 History')
            session.send(b'1')
            session.wait(b'Declared tools')
            session.send(b'2')
            session.wait(b'Inspect installed tools')
            session.send(b'\x1b')
            session.wait(b'2 Installed')
            session.send(b'/3')
            session.settle()
            assert b'Installed packages' in session.screen.text(), 'A filter digit changed destination'
            session.send(b'\r1a')
            session.wait(b'Tool ID')
            session.send(b'\tq?/')
            session.settle()
            session.send(b'\x1bOP')
            session.wait(b'Enter opens the selected')
            session.settle()
            rows = session.screen.text().splitlines()
            footer = next(row for row in rows if b'Esc Back' in row)
            assert footer != rows[-1] and b'Review' not in footer
            session.mouse(0, 0, 0)
            session.wait(b'q?/')
            session.destination('Checks')
            session.wait(b'Checks')
            session.navigate('Tools', 'Declared tools [1]')
            session.send(b'a')
            session.wait(b'q?/')
            session.send(b'\x1bq')
            session.finish()
        assert calls(home) == '', 'Navigation or field editing ran a provider'
        print('PASS numbered Tools tabs, literal filter digits, retained draft, relevant footer and foreground Help', size)
