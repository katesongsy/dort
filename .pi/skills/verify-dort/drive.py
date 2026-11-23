#!/usr/bin/env python3
"""Drive a disposable Dort instance and capture proof.

Subcommands:
  build                 Build target/debug/dort once.
  doctor                Read-only check of the environment and last run.
  smoke                 Build, then prove create, read and cleanup in one run.
  new NAME              Create an isolated HOME + repository for one run.
  env NAME              Print exports that put the current shell in that run.
  cli NAME -- ARGS...   Run the dort binary with ARGS inside that run.
  tui NAME STEP...      Drive the TUI through scripted steps.
  cleanup [NAME|--all]  Tear down scratch state. Evidence always survives.

Evidence lands in temp/verify-dort/runs/NAME/evidence/ (temp/ is gitignored).
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
BINARY = ROOT / "target" / "debug" / "dort"
STORE = ROOT / "temp" / "verify-dort" / "runs"


def fail(message):
    print(f"FAIL {message}", file=sys.stderr)
    raise SystemExit(1)


def run_dir(name):
    return STORE / name


def load(name):
    manifest = run_dir(name) / "run.json"
    if not manifest.is_file():
        fail(f"no run named {name!r}; create it with: drive.py new {name}")
    return json.loads(manifest.read_text())


def save(name, data):
    path = run_dir(name) / "run.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2) + "\n")


def evidence_dir(name):
    directory = run_dir(name) / "evidence"
    directory.mkdir(parents=True, exist_ok=True)
    return directory


def record(name, label, body):
    directory = evidence_dir(name)
    existing = sorted(p for p in directory.glob("*.txt"))
    path = directory / f"{len(existing) + 1:02d}-{label}.txt"
    path.write_text(body)
    return path


def iso_env(home):
    home = Path(home)
    return {
        "HOME": str(home),
        "XDG_CONFIG_HOME": str(home / ".config"),
        "XDG_STATE_HOME": str(home / ".local" / "state"),
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "TERM": "xterm-256color",
        "LANG": "C.UTF-8",
        "NO_COLOR": os.environ.get("NO_COLOR", ""),
    }


def scratch_base():
    return Path(os.environ.get("DORT_VERIFY_SCRATCH") or tempfile.gettempdir())


def scratch(name):
    # Dort refuses to open a state directory through a symlink, and macOS /tmp is
    # one. Resolve every path to its real form before handing it to the binary.
    root = Path(tempfile.mkdtemp(prefix=f"dort-verify-{name}-", dir=scratch_base())).resolve()
    return root, scratch_real_ok(root)


def scratch_real_ok(path):
    """True when path resolves to itself, so Dort's no-symlink state check passes."""
    path = Path(path)
    return Path(os.path.realpath(path)) == path


def command_build(_):
    result = subprocess.run(
        ["cargo", "build", "-p", "dort", "--locked"], cwd=ROOT,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
    )
    if result.returncode or not BINARY.is_file():
        print(result.stdout)
        fail("cargo build -p dort --locked failed")
    print(f"binary {BINARY} ({command_version()})")


def command_version():
    out = subprocess.run([str(BINARY), "--version"], capture_output=True, text=True)
    return out.stdout.strip() or "unknown"


def command_doctor(_):
    checks = []

    def check(label, ok, detail=""):
        checks.append((label, bool(ok), detail))

    check("cargo on PATH", shutil.which("cargo"), shutil.which("cargo") or "missing")
    check("git on PATH", shutil.which("git"), shutil.which("git") or "missing")
    check("binary built", BINARY.is_file(), str(BINARY))
    if BINARY.is_file():
        check("binary runs", command_version(), command_version())
    check("scratch base writable", os.access(scratch_base(), os.W_OK), str(scratch_base()))
    for name in sorted(p.name for p in STORE.glob("*") if p.is_dir()):
        kept = run_dir(name) / "evidence"
        if not (run_dir(name) / "run.json").is_file():
            check(f"run {name} record present", False,
                  f"evidence only at {kept}; no run.json (created by an older drive.py)")
            continue
        data = load(name)
        root = Path(data["scratch"])
        kept = run_dir(name) / "evidence"
        if data.get("cleaned"):
            check(f"run {name} cleaned", not root.exists(), str(root))
        else:
            check(f"run {name} scratch present", root.is_dir(), str(root))
            check(f"run {name} scratch symlink-free", scratch_real_ok(root), str(root))
        check(f"run {name} evidence kept", kept.is_dir() and any(kept.iterdir()), str(kept))
    for label, ok, detail in checks:
        print(f"{'ok  ' if ok else 'FAIL'} {label}: {detail}")
    if not all(ok for _, ok, _ in checks):
        raise SystemExit(1)


def command_new(args):
    if run_dir(args.name).is_dir():
        fail(f"run {args.name!r} exists; use cleanup {args.name} first")
    root, real = scratch(args.name)
    if not real:
        fail(f"scratch root {root} still contains a symlink; set DORT_VERIFY_SCRATCH to a real directory")
    home = root / "home"
    home.mkdir(mode=0o700)
    repo = root / "repo"
    data = {"name": args.name, "scratch": str(root), "home": str(home),
            "repo": str(repo), "created": time.time()}
    save(args.name, data)
    env = iso_env(home)
    result = subprocess.run([str(BINARY), "--yes", "init", str(repo)],
                            env=env, capture_output=True, text=True)
    record(args.name, "init", f"$ dort --yes init {repo}\nexit {result.returncode}\n{result.stdout}{result.stderr}")
    if result.returncode:
        shutil.rmtree(root, ignore_errors=True)  # keep evidence, drop the half-made root
        save(args.name, {**data, "cleaned": True, "failed": "init"})
        fail(f"init failed (exit {result.returncode}); inspect {evidence_dir(args.name)}, then cleanup {args.name}")
    (repo / "demo.txt").write_text("hello\n")
    print(json.dumps({**data, "evidence": str(evidence_dir(args.name))}, indent=2))


def command_cli(name, label, arguments):
    data = load(name)
    root = Path(data["scratch"])
    env = iso_env(Path(data["home"]))
    arguments = [a.replace("@REPO", data["repo"]).replace("@HOME", data["home"])
                 for a in arguments]
    result = subprocess.run([str(BINARY), *arguments], cwd=root, env=env,
                            capture_output=True, text=True)
    record(name, label or "cli",
           f"$ dort {' '.join(arguments)}\nexit {result.returncode}\n"
           f"--- stdout ---\n{result.stdout}--- stderr ---\n{result.stderr}")
    print(result.stdout, end="")
    if result.stderr:
        print(result.stderr, end="", file=sys.stderr)
    raise SystemExit(result.returncode)


def load_pty_harness():
    """Import Screen/Session from scripts/test-terminal.py (a hyphen file)."""
    import importlib.util

    path = ROOT / "scripts" / "test-terminal.py"
    sys.path.insert(0, str(ROOT / "scripts"))  # test-terminal imports terminal_tools
    spec = importlib.util.spec_from_file_location("dort_test_terminal", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.Session


def parse_steps(steps):
    parsed = []
    for step in steps:
        kind, _, value = step.partition(":")
        if kind == "wait":
            parsed.append(("wait", value.encode()))
        elif kind == "send":
            parsed.append(("send", eval(value).encode() if value else b""))
        elif kind == "click":
            parsed.append(("click", value))
        elif kind == "snap":
            parsed.append(("snap", value or "frame"))
        elif kind == "resize":
            rows, _, columns = value.partition("x")
            parsed.append(("resize", (int(rows), int(columns))))
        elif kind == "sleep":
            parsed.append(("sleep", float(value)))
        else:
            fail(f"unknown step {step!r}")
    return parsed


def command_tui(args):
    Session = load_pty_harness()

    data = load(args.name)
    env = iso_env(Path(data["home"]))
    steps = parse_steps(args.steps)
    frames = []
    failure = None
    try:
        with Session([str(BINARY), "tui"], env, tuple(args.size)) as session:
            for kind, value in steps:
                if kind == "wait":
                    session.wait(value)
                elif kind == "send":
                    session.send(value)
                elif kind == "click":
                    session.click_label(value)
                elif kind == "snap":
                    session.settle()
                    frames.append(f"--- {value} ---\n{session.screen.text().decode()}")
                elif kind == "resize":
                    session.resize(value)
                elif kind == "sleep":
                    time.sleep(value)
            session.send(b"q")
            session.finish()
    except BaseException as error:  # a failed attempt still leaves a trace
        failure = f"--- failed ---\n{type(error).__name__}: {error}\n"
        frames.append(failure)
        record(args.name, args.label or "tui", "\n".join(frames))
        raise

    record(args.name, args.label or "tui",
           "\n".join(frames) + "\n--- exit ---\n0 (alternate screen left, terminal restored)\n")
    for frame in frames:
        print(frame)


def command_smoke(_):
    """Prove the disposable-HOME path end to end: build, create, read, clean up."""
    command_build(None)
    name = "smoke"
    if run_dir(name).is_dir():
        command_cleanup(argparse.Namespace(name=name, all=False))
    command_new(argparse.Namespace(name=name))
    for label, arguments in (("version", ["--version"]), ("doctor", ["doctor"])):
        try:
            command_cli(name, label, arguments)
        except SystemExit as status:
            if status.code:
                fail(f"dort {label} exited {status.code}; inspect {evidence_dir(name)}")
    command_cleanup(argparse.Namespace(name=name, all=False))
    print("PASS disposable HOME: build, init, version, doctor, cleanup")


def command_cleanup(args):
    names = ([p.name for p in STORE.glob("*") if p.is_dir()]
             if args.all else [args.name])
    for name in names:
        if not name:
            fail("name the run, or pass --all")
        manifest = run_dir(name) / "run.json"
        if not manifest.is_file():
            print(f"skip {name}: no run")
            continue
        data = json.loads(manifest.read_text())
        root = Path(data["scratch"])
        if root.name.startswith("dort-verify-") and root.is_dir():
            # Kill only children this tool started; runs are synchronous, so none
            # outlive a command. Then drop scratch, keeping evidence.
            shutil.rmtree(root, ignore_errors=True)
        save(name, {**data, "cleaned": True})
        print(f"cleaned {name}; evidence kept at {run_dir(name) / 'evidence'}")
    if args.all:
        sweep_orphans()


def sweep_orphans():
    """Remove scratch roots whose run record is gone, never a live one."""
    live = {json.loads(p.read_text())["scratch"]
            for p in STORE.glob("*/run.json") if p.is_file()}
    for root in scratch_base().glob("dort-verify-*"):
        if str(root.resolve()) not in live:
            shutil.rmtree(root, ignore_errors=True)
            print(f"swept orphan {root}")


def command_env(args):
    data = load(args.name)
    env = iso_env(Path(data["home"]))
    lines = [f"export {key}={value!r}" for key, value in env.items() if value]
    lines.append(f"export DORT_REPO={data['repo']!r}")
    lines.append(f"export DORT_EVIDENCE={str(evidence_dir(args.name))!r}")
    lines.append("cd $HOME")
    print("\n".join(lines))


def main():
    # `cli NAME [--label L] -- ARGS...` hands everything after `--` to dort, so
    # dort's own flags never collide with this tool's.
    argv = sys.argv[1:]
    if argv[:1] == ["cli"] and "--" in argv:
        head = argv[:argv.index("--")]
        tail = argv[argv.index("--") + 1:]
        label = None
        if "--label" in head:
            at = head.index("--label")
            label = head[at + 1]
            del head[at:at + 2]
        if len(head) != 2:
            fail("usage: drive.py cli NAME [--label LABEL] -- ARGS...")
        return command_cli(head[1], label, tail)

    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("build").set_defaults(handler=command_build)
    sub.add_parser("doctor").set_defaults(handler=command_doctor)
    sub.add_parser("smoke").set_defaults(handler=command_smoke)

    new = sub.add_parser("new")
    new.add_argument("name")
    new.set_defaults(handler=command_new)

    env = sub.add_parser("env")
    env.add_argument("name")
    env.set_defaults(handler=command_env)

    tui = sub.add_parser("tui")
    tui.add_argument("name")
    tui.add_argument("--label")
    tui.add_argument("--size", nargs=2, type=int, default=(24, 80))
    tui.add_argument("steps", nargs="+")
    tui.set_defaults(handler=command_tui)

    cleanup = sub.add_parser("cleanup")
    cleanup.add_argument("name", nargs="?")
    cleanup.add_argument("--all", action="store_true")
    cleanup.set_defaults(handler=command_cleanup)

    args = parser.parse_args()
    args.handler(args)


if __name__ == "__main__":
    main()
