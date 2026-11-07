#!/usr/bin/env python3
"""Opt-in 128 MiB ext4 ENOSPC launcher, ONLY inside the approved disposable Lima VM.

Requires an administrator-created guest marker and explicit --approved-disposable-vm.
Never run as root, on a workstation, or with shared host mounts. Not an ordinary CI gate.
This administrative guard is not proof of isolation; inspect the VM configuration first.
"""

import argparse
import json
import os
from pathlib import Path
import platform
import stat
import shutil
import signal
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
MARKER = Path("/run/dort-approved-storage-vm")
TOKEN = "dort-isolated-lima-a12-v1\n"
SIZE = 128 * 1024 * 1024


def run(*args, **kwargs):
    # Own/reap the test process group before attempting to unmount on errors/timeouts.
    # Fixture Git must create private entries even if the guest login umask is 0002.
    # Set only the child mask; never mutate the supervisor's process-global mask.
    process = subprocess.Popen(args, start_new_session=True, umask=0o077, **kwargs)
    try:
        code = process.wait(timeout=300)
    except BaseException:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()
        raise
    if code:
        raise subprocess.CalledProcessError(code, args)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--approved-disposable-vm", action="store_true", required=True)
    parser.parse_args()
    if platform.system() != "Linux" or os.geteuid() == 0:
        raise SystemExit("Requires a non-root Linux guest user")
    if platform.node() != "lima-a12":
        raise SystemExit("This launcher is restricted to the reviewed lima-a12 guest")
    metadata = MARKER.lstat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != 0:
        raise SystemExit("Missing administrator-owned VM approval marker")
    if stat.S_IMODE(metadata.st_mode) != 0o644 or MARKER.read_text() != TOKEN:
        raise SystemExit("Unexpected VM approval marker")
    mounts = json.loads(subprocess.check_output(["findmnt", "--json", "-o", "FSTYPE"], timeout=15))

    def shared(entries):
        return any(
            e["fstype"] in {"virtiofs", "9p", "fuse.sshfs"}
            or shared(e.get("children", []))
            for e in entries
        )

    if shared(mounts["filesystems"]):
        raise SystemExit("Refusing guest with host-sharing filesystem mounts")
    os.chdir(ROOT)
    run("cargo", "test", "-p", "dort-core", "--test", "storage_faults", "--no-run", "--locked")
    scope = Path(tempfile.mkdtemp(prefix="dort-storage-vm-", dir="/var/tmp")).resolve()
    mounted = False
    try:
        image, volume = scope / "bounded.ext4", scope / "volume"
        volume.mkdir(mode=0o700)
        # Physically allocate only this 128 MiB image on the guest's disk.
        run("fallocate", "--length", str(SIZE), str(image))
        assert image.stat().st_size == SIZE
        run("mkfs.ext4", "-q", "-F", "-b", "4096", "-m", "0", str(image))
        try:
            run("sudo", "-n", "mount", "-o", "loop,nodev,nosuid,noexec,nodelalloc", str(image), str(volume))
            mounted = True
            info = json.loads(subprocess.check_output([
                "findmnt", "--json", "--mountpoint", str(volume), "-o", "SOURCE,FSTYPE,TARGET,OPTIONS"
            ], timeout=15))["filesystems"]
            if len(info) != 1 or info[0]["fstype"] != "ext4" or info[0]["target"] != str(volume):
                raise RuntimeError("Unexpected mounted filesystem")
            source = info[0]["source"]
            backing = subprocess.check_output([
                "sudo", "-n", "losetup", "--noheadings", "--output", "BACK-FILE", source
            ], text=True, timeout=15).strip()
            if Path(backing) != image:
                raise RuntimeError("Loop device does not bind this harness's exact image")
            if os.stat(volume).st_dev == os.stat("/").st_dev:
                raise RuntimeError("Test volume is not a separate filesystem")
            run("sudo", "-n", "chown", f"{os.geteuid()}:{os.getegid()}", str(volume))
            (volume / ".dort-storage-volume").write_text("dort-disposable-ext4-128MiB-v1\n")
            print(f"Verified guest-only ext4 loop volume: {SIZE} bytes; host shares absent", flush=True)
            run("cargo", "test", "-p", "dort-core", "--locked", "--test", "storage_faults", "--",
                "--ignored", "--exact", "kernel_enospc_preserves_intent_and_refuses_uncertain_recovery",
                "--nocapture", env=os.environ | {"DORT_TEST_STORAGE_VOLUME": str(volume)})
        finally:
            if mounted:
                # No lazy/forced unmount. If unmount fails, retain the exact scope for diagnosis.
                try:
                    run("sudo", "-n", "umount", str(volume))
                    mounted = False
                except BaseException:
                    raise SystemExit(f"Unmount failed; retain guest scope {scope} and stop VM")
        print("Storage harness complete; guest loop volume unmounted", flush=True)
    finally:
        if not mounted:
            shutil.rmtree(scope)


if __name__ == "__main__":
    main()
