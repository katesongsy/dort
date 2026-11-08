#!/usr/bin/env python3
"""Validate direct GitHub-backed mise installation in a disposable canonical HOME.
Never modifies workstation mise configuration or performs a real-home app task.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TOOL = "github:katesongsy/dort@0.1.0"


def run(mise, arguments, environment, directory):
    process = subprocess.Popen(
        [str(mise), *arguments], cwd=directory, env=environment,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True,
    )
    try:
        output, error = process.communicate(timeout=180)
    except BaseException:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.communicate()
        raise
    if process.returncode:
        # JSON escaping keeps untrusted error bytes from becoming terminal controls.
        raise RuntimeError(json.dumps({"arguments": arguments, "exit": process.returncode,
                                       "stderr": error[:8192].decode("utf-8", "backslashreplace")}))
    if len(output) > 256 * 1024 or len(error) > 256 * 1024:
        raise RuntimeError("unexpectedly large metadata output")
    return output.decode("utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mise", default=shutil.which("mise"))
    args = parser.parse_args()
    if not args.mise:
        parser.error("supply the trusted, pinned mise executable")
    mise = Path(args.mise).resolve(strict=True)
    targets = {
        ("Darwin", "arm64"): "aarch64-apple-darwin",
        ("Darwin", "x86_64"): "x86_64-apple-darwin",
        ("Linux", "aarch64"): "aarch64-unknown-linux-gnu",
        ("Linux", "x86_64"): "x86_64-unknown-linux-gnu",
    }
    target = targets[(platform.system(), platform.machine())]
    release = json.loads((ROOT / "scripts/fixtures/mise-v0.1.0.json").read_text())
    expected = release["targets"][target]
    with tempfile.TemporaryDirectory(prefix="dort-mise-test-") as temporary:
        root = Path(temporary).resolve()
        home = root / "home"
        home.mkdir(mode=0o700)
        environment = {
            "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
            "HOME": str(home), "TERM": "dumb", "CI": "1",
            "XDG_CONFIG_HOME": str(root / "xdg/config"),
            "XDG_DATA_HOME": str(root / "xdg/data"),
            "XDG_CACHE_HOME": str(root / "xdg/cache"),
            "XDG_STATE_HOME": str(root / "xdg/state"),
            "MISE_CONFIG_DIR": str(root / "config"),
            "MISE_GLOBAL_CONFIG_FILE": str(root / "config/config.toml"),
            "MISE_DATA_DIR": str(root / "data"),
            "MISE_CACHE_DIR": str(root / "cache"),
            "MISE_STATE_DIR": str(root / "state"),
            "MISE_YES": "1",
        }
        version = run(mise, ["--version"], environment, root)
        if "2026.11.4" not in version:
            raise RuntimeError("expected reviewed mise 2026.11.4")
        run(mise, ["use", "--global", TOOL], environment, root)
        installed = Path(run(mise, ["which", "dort"], environment, root).strip()).resolve(strict=True)
        if not installed.is_relative_to(root / "data"):
            raise RuntimeError("mise selected an executable outside its disposable installation")
        if hashlib.sha256(installed.read_bytes()).hexdigest() != expected["binary_sha256"]:
            raise RuntimeError("mise selected a binary differing from the verified release asset")
        application_home = root / "application-home"
        application_home.mkdir(mode=0o700)
        output = run(mise, [
            "exec", TOOL, "--", "/usr/bin/env",
            "HOME=" + str(application_home),
            "XDG_CONFIG_HOME=" + str(application_home / "config"),
            "XDG_STATE_HOME=" + str(application_home / "state"),
            "dort", "--version",
        ], environment, root)
        if output != "dort 0.1.0\n" or list(application_home.iterdir()):
            raise RuntimeError("installed version/no-state smoke check failed")
    print("PASS mise GitHub backend: " + target + "; exact release binary/version; isolated HOME stayed empty")


if __name__ == "__main__":
    main()
