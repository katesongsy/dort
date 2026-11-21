#!/usr/bin/env python3
"""Focused or full Rust tests. Full builds once and runs each test in its own process."""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
FULL_BUILD = ['cargo', 'test', '--workspace', '--no-run', '--locked', '--message-format=json']
FULL_DOC = ['cargo', 'test', '--workspace', '--doc', '--locked', '--', '--test-threads=1']
FOCUSED = ('quick', 'npm', 'tools', 'ui')


def cargo(*args):
    return ['cargo', 'test', *args, '--locked', '--', '--test-threads=1']


def plan(suite):
    if suite == 'quick':
        return [('Workspace library and application units', cargo('--workspace', '--lib', '--bins'))]
    if suite == 'ui':
        return [('Application and TUI units', cargo('-p', 'dort', '--bin', 'dort'))]
    if suite == 'npm':
        return [
            ('npm core units', cargo('-p', 'dort-core', '--lib', 'npm')),
            ('npm inspection, planning and managed installation', cargo('-p', 'dort-core',
                '--test', 'npm_inventory', '--test', 'tool_inspection', '--test', 'tool_planning', '--test', 'npm_installation')),
            ('npm CLI', cargo('-p', 'dort', '--test', 'npm_inventory_cli', '--test', 'npm_install_cli')),
            ('npm application scope guards', cargo('-p', 'dort', '--bin', 'dort', 'npm')),
            ('Installed discovery TUI', cargo('-p', 'dort', '--bin', 'dort', 'installed_tools_tests')),
        ]
    if suite == 'tools':
        commands = [
            ('Tools core units', cargo('-p', 'dort-core', '--lib', 'tools::')),
            ('Tools inspection, planning and brew/npm installation', cargo('-p', 'dort-core',
                '--test', 'tool_inspection', '--test', 'npm_inventory', '--test', 'tool_planning',
                '--test', 'tool_versions', '--test', 'tool_binaries', '--test', 'tool_install_previews',
                '--test', 'tool_installation', '--test', 'npm_installation', '--test', 'brew_installation',
                '--test', 'tool_declaration', '--test', 'tool_declaration_changes')),
            ('Tools CLI', cargo('-p', 'dort', '--test', 'declared_tools_cli', '--test', 'tool_declaration_cli',
                '--test', 'tool_install_cli', '--test', 'npm_install_cli', '--test', 'npm_inventory_cli',
                '--test', 'brew_install_cli')),
            ('Application services', cargo('-p', 'dort', '--bin', 'dort', 'services::')),
        ]
        for name in ('tools_tests', 'tools_ux_tests', 'tools_install_tests', 'installed_tools_tests',
                     'declared_workflow_tests', 'tool_declaration_tests', 'declaration_changes_tests'):
            commands.append((name, cargo('-p', 'dort', '--bin', 'dort', 'tui::' + name + '::')))
        return commands
    raise ValueError('Unknown focused suite: ' + suite)


def executables(build):
    """Every test binary a cargo test invocation builds."""
    result = subprocess.run(build, cwd=ROOT, capture_output=True, text=True)
    if result.returncode:
        print(result.stderr, flush=True)
        return None
    found = set()
    for line in result.stdout.splitlines():
        try:
            message = json.loads(line)
        except ValueError:
            continue
        if message.get('reason') != 'compiler-artifact':
            continue
        executable = message.get('executable')
        if executable and message.get('profile', {}).get('test'):
            found.add(executable)
    return sorted(found)


VALUE_FLAGS = {'-p', '--package', '--test', '--bin', '--example', '--bench',
               '--exclude', '--features', '-F', '--target', '--jobs', '-j'}


def selection(command):
    """The build command and the name filters behind one cargo test command.

    Cargo takes the test-name filter as a bare positional before `--`, so a
    filter is any positional left once the flags and their values are removed.
    """
    at = command.index('--')
    head = command[:at]
    filters, build = [], list(head[:2])  # 'cargo', 'test'
    index = 2
    while index < len(head):
        part = head[index]
        if part in VALUE_FLAGS:
            build.extend(head[index:index + 2])
            index += 2
        elif part.startswith('-'):
            build.append(part)
            index += 1
        else:
            filters.append(part)
            index += 1
    return build + ['--no-run', '--message-format=json'], filters


def private_tmpdirs(count):
    """A private TMPDIR per test, all created before any test starts.

    A private TMPDIR per test keeps unrelated sibling creation out of shared
    temp ancestors. A short private root keeps scope metadata under its fixed
    bounds; system temp roots can be much longer than an application HOME.
    """
    short = Path('/tmp') if Path('/tmp').is_dir() and os.access('/tmp', os.W_OK) else None
    tmp_root = Path(tempfile.mkdtemp(prefix='dort-', dir=short))
    tmpdirs = []
    for index in range(count):
        path = tmp_root / str(index)
        path.mkdir()
        tmpdirs.append(str(path))
    return tmp_root, tmpdirs


def execute(executable, name, tmpdir, timeout):
    start = time.monotonic()
    environment = dict(os.environ, TMPDIR=tmpdir)
    try:
        result = subprocess.run([executable, '--exact', name, '--test-threads=1'],
                                cwd=ROOT, capture_output=True, text=True, timeout=timeout,
                                env=environment)
        output = result.stdout + result.stderr
        ran = re.search(r'test result: .*? 1 passed;', output) is not None
        code = result.returncode if ran else 'not run'
        return executable, name, code, output, time.monotonic() - start
    except subprocess.TimeoutExpired as error:
        output = (error.stdout or b'') + (error.stderr or b'')
        if isinstance(output, bytes):
            output = output.decode(errors='replace')
        return executable, name, 'timeout', output, time.monotonic() - start


def run_each(label, command, jobs, timeout):
    """One process per test, at most `jobs` at a time. Returns (status, passed).

    A test binary shares one process among its tests, and these tests interfere
    in a shared process: run two at once inside one binary and a different test
    fails on each attempt, even at two threads. Giving every test its own process
    removes the interference, and the process is the unit these commands already
    name. `--test-threads=1` stays because one process runs one test, so it costs
    nothing and states the guarantee.
    """
    build, filters = selection(command)
    binaries = executables(build)
    if binaries is None:
        return 1, 0
    work = [(executable, name) for executable in binaries
            for name in tests(executable)
            if all(part in name for part in filters)]
    if not work:
        print(f'FAIL {label}: no passing tests matched; update the focused suite.')
        return 1, 0
    started = time.monotonic()
    tmp_root, tmpdirs = private_tmpdirs(len(work))
    passed = failed = timed_out = 0
    failures = []
    with ThreadPoolExecutor(max_workers=jobs) as pool:
        futures = [pool.submit(execute, executable, name, tmpdir, timeout)
                   for (executable, name), tmpdir in zip(work, tmpdirs)]
        for future in as_completed(futures):
            _, name, code, output, _ = future.result()
            if code == 0:
                passed += 1
            elif code == 'timeout':
                timed_out += 1
                failures.append((name, f'timed out after {timeout:.0f}s'))
            else:
                failed += 1
                failures.append((name, output[-8000:]))
    shutil.rmtree(tmp_root, ignore_errors=True)
    for name, output in failures:
        print(f'\nFAIL {name}\n{output}', flush=True)
    verdict = 'PASS' if not failed and not timed_out else 'FAIL'
    print(f'{verdict} {label}: {passed} passed, {failed} failed, {timed_out} timed out '
          f'in {time.monotonic() - started:.1f}s', flush=True)
    return (1 if failed or timed_out else 0), passed


def run(suite, jobs, timeout):
    print(f'Focused {suite} checks: {jobs} parallel processes '
          f'(each test runs alone with a private TMPDIR).', flush=True)
    print('Full durability validation: python3 scripts/test.py full', flush=True)
    started = time.monotonic()
    total = 0
    for label, command in plan(suite):
        print(f'\nRunning {label}\n$ {shlex.join(command)}', flush=True)
        status, passed = run_each(label, command, jobs, timeout)
        if status:
            return status
        total += passed
    print(f'\nPASS {suite}: {total} tests in {time.monotonic() - started:.1f}s', flush=True)
    return 0


def artifacts():
    return executables(FULL_BUILD)


def listed(executable, *options):
    result = subprocess.run([executable, '--list', '--format', 'terse', *options],
                            cwd=ROOT, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(f'{executable} could not list tests: {result.stderr}')
    names = set()
    for line in result.stdout.splitlines():
        name = line.strip().split(': ', 1)[0]
        if name:
            names.add(name)
    return names


def tests(executable):
    ignored = listed(executable, '--ignored')
    return sorted(listed(executable) - ignored)


def priority(name):
    # Longest exhaustion tests first so the tail does not decide total wall time.
    heavy = ('process_exit', 'forward_inverse', 'every_', 'recovery_itself', 'unsafe_ancestor')
    return 0 if name.startswith(heavy) or any(part in name for part in heavy) else 1


def run_full(jobs, timeout):
    started = time.monotonic()
    executables = artifacts()
    if executables is None:
        return 1
    work = []
    for executable in executables:
        for name in tests(executable):
            work.append((executable, name))
    work.sort(key=lambda item: (priority(item[1]), item[0], item[1]))
    if not work:
        print('FAIL Full workspace: no tests discovered.')
        return 1
    tmp_root, tmpdirs = private_tmpdirs(len(work))
    print(f'Full workspace: {len(work)} tests in {len(executables)} binaries, '
          f'{jobs} parallel processes (each test runs alone with private TMPDIR).', flush=True)
    passed = failed = timed_out = 0
    failures = []
    slowest = []

    done = 0
    with ThreadPoolExecutor(max_workers=jobs) as pool:
        futures = [pool.submit(execute, executable, name, tmpdir, timeout)
                   for (executable, name), tmpdir in zip(work, tmpdirs)]
        for future in as_completed(futures):
            executable, name, code, output, elapsed = future.result()
            done += 1
            slowest.append((elapsed, name))
            if code == 0:
                passed += 1
            elif code == 'timeout':
                timed_out += 1
                failures.append((name, f'timed out after {timeout:.0f}s'))
            else:
                failed += 1
                failures.append((name, output[-8000:]))
            if done % 50 == 0 or done == len(work):
                print(f'  {done}/{len(work)} tests, {passed} passed, {failed} failed', flush=True)

    shutil.rmtree(tmp_root, ignore_errors=True)
    for name, output in failures:
        print(f'\nFAIL {name}\n{output}', flush=True)
    doc = subprocess.run(FULL_DOC, cwd=ROOT, capture_output=True, text=True)
    if doc.returncode:
        print('\nFAIL doc tests\n' + doc.stdout + doc.stderr, flush=True)
        failed += 1
    print('\nSlowest tests:', flush=True)
    for elapsed, name in sorted(slowest, reverse=True)[:5]:
        print(f'  {elapsed:6.1f}s {name}', flush=True)
    status = 1 if failed or timed_out else 0
    result = 'PASS' if status == 0 else 'FAIL'
    print(f'\n{result} full: {passed} passed, {failed} failed, {timed_out} timed out '
          f'in {time.monotonic() - started:.1f}s', flush=True)
    return status


class PlanTests(unittest.TestCase):
    def test_focused_commands_are_locked_serial_and_keep_existing_tests(self):
        for suite in FOCUSED:
            for _, command in plan(suite):
                self.assertEqual(command[:2], ['cargo', 'test'])
                self.assertIn('--locked', command)
                self.assertEqual(command[-2:], ['--', '--test-threads=1'])
                self.assertFalse(any(arg in command for arg in ('--ignored', '--skip', '--include-ignored')))

    def test_full_builds_once_and_runs_every_test_in_its_own_process(self):
        self.assertEqual(FULL_BUILD, ['cargo', 'test', '--workspace', '--no-run', '--locked',
                                      '--message-format=json'])
        self.assertEqual(FULL_DOC[-2:], ['--', '--test-threads=1'])
        self.assertIn('--workspace', FULL_DOC)
        self.assertFalse(any(arg in FULL_DOC for arg in ('--ignored', '--skip', '--include-ignored')))

    def test_quick_is_explicitly_unit_only_and_npm_has_both_frontends(self):
        self.assertEqual(plan('quick')[0][1], cargo('--workspace', '--lib', '--bins'))
        commands = [command for _, command in plan('npm')]
        self.assertTrue(any('npm_installation' in command for command in commands))
        self.assertTrue(any('npm_inventory_cli' in command for command in commands))
        self.assertTrue(any('installed_tools_tests' in command for command in commands))

    def test_selection_recovers_the_filters_and_builds_without_running(self):
        build, filters = selection(cargo('-p', 'dort', '--bin', 'dort', 'tools::'))
        self.assertNotIn('--quiet', build)
        self.assertEqual(build[-2:], ['--no-run', '--message-format=json'])
        self.assertEqual(filters, ['tools::'])
        self.assertNotIn('--test-threads=1', build)
        self.assertEqual(selection(cargo('--workspace', '--lib', '--bins'))[1], [])

    def test_every_focused_command_can_run_one_process_per_test(self):
        # The runner derives a build command and a filter from each entry, so an
        # entry without a `--` separator would silently run nothing.
        for suite in FOCUSED:
            for label, command in plan(suite):
                self.assertIn('--', command, label)
                build, _ = selection(command)
                self.assertEqual(build[:3], ['cargo', 'test', '-p'] if '-p' in command
                                 else ['cargo', 'test', '--workspace'], label)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('suite', nargs='?', choices=(*FOCUSED, 'full'), default='quick')
    parser.add_argument('--list', action='store_true', help='Show the exact commands without running them')
    parser.add_argument('--self-test', action='store_true', help='Check suite selection without running Rust tests')
    parser.add_argument('--jobs', type=int, default=min(8, os.cpu_count() or 4),
                        help='Parallel processes for full (default: up to 8)')
    parser.add_argument('--timeout', type=float, default=600,
                        help='Per-test timeout in seconds for full (default: 600)')
    args = parser.parse_args()
    if args.self_test:
        result = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(PlanTests))
        return 0 if result.wasSuccessful() else 1
    if args.list:
        if args.suite == 'full':
            print('Full workspace: ' + shlex.join(FULL_BUILD))
            print('  then every test in its own process, at most --jobs concurrently, each --exact --test-threads=1')
            print('Doc tests: ' + shlex.join(FULL_DOC))
            return 0
        print('Focused suites run each test in its own process too, at most --jobs concurrently,')
        print('each --exact --test-threads=1. These commands select the tests; they are not the runner.')
        for label, command in plan(args.suite):
            build, filters = selection(command)
            print(f'{label}: {shlex.join(build)}' + (f' {shlex.join(filters)}' if filters else ''))
        return 0
    if args.jobs < 1 or args.timeout <= 0:
        print('--jobs and --timeout must be positive.')
        return 2
    if args.suite == 'full':
        return run_full(args.jobs, args.timeout)
    return run(args.suite, args.jobs, args.timeout)


if __name__ == '__main__':
    raise SystemExit(main())
