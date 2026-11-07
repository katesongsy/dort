#!/usr/bin/env python3
"""Opt-in bounded macOS cross-device refusal checks; never a storage-fault test."""
import argparse
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import signal
import stat
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
HDIUTIL = "/usr/bin/hdiutil"
DISKUTIL = "/usr/sbin/diskutil"
SIZE = 128 * 1024 * 1024
TOKEN = "dort-disposable-cross-device-v1\n"
CASES = {
    "adoption": "different_filesystem_state_has_no_cross_device_copy_fallback",
    "lifecycle": "separate_filesystems_are_refused_without_live_mutations_when_available",
    "bootstrap": "cross_device_bootstrap_refuses_before_repository_or_registration_publication",
}


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def run(args, *, env=None, timeout=120):
    """Own and reap the entire child group before any mount cleanup."""
    child = subprocess.Popen(
        args, cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        start_new_session=True,
    )
    try:
        out, err = child.communicate(timeout=timeout)
    except BaseException:
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        child.communicate()
        raise
    require(child.returncode == 0,
            f"{Path(args[0]).name} failed ({child.returncode}): "
            + (out + err).decode(errors="replace")[-6000:])
    return out, err


def plist(args):
    return plistlib.loads(run(args)[0])


def image_entry(info, image):
    entries = [entry for entry in info.get("images", [])
               if Path(entry.get("image-path", "")).resolve() == image]
    require(len(entries) == 1, "cannot identify exactly one owned attached image")
    return entries[0]


def image_devices(entry, volume):
    entities = entry.get("system-entities", [])
    whole = [e["dev-entry"] for e in entities
             if re.fullmatch(r"/dev/disk[0-9]+", e.get("dev-entry", ""))]
    mounted = [e for e in entities if e.get("mount-point") == str(volume)]
    require(len(whole) == 1 and len(mounted) == 1,
            "unexpected image device or mount layout")
    device = mounted[0].get("dev-entry", "")
    require(re.fullmatch(re.escape(whole[0]) + r"s[0-9]+", device) is not None,
            "mount is not a partition of the exact owned image")
    require(all(not e.get("mount-point") or e["mount-point"] == str(volume)
                for e in entities), "image has an unexpected mount")
    return whole[0], device


def private(path, directory):
    metadata = path.lstat()
    require(stat.S_ISDIR(metadata.st_mode) if directory else stat.S_ISREG(metadata.st_mode),
            f"unexpected fixture type: {path}")
    require(metadata.st_uid == os.geteuid(), f"fixture owner mismatch: {path}")
    require(stat.S_IMODE(metadata.st_mode) == (0o700 if directory else 0o600),
            f"fixture permissions mismatch: {path}")
    require(path.resolve() == path, f"noncanonical fixture: {path}")
    if not directory:
        require(metadata.st_nlink == 1, f"hard-linked fixture: {path}")
    return metadata


def test_binaries():
    out, _ = run([
        "cargo", "test", "-p", "dort-core", "--locked", "--no-run",
        "--test", "adoption", "--test", "lifecycle", "--test", "bootstrap",
        "--message-format=json",
    ], timeout=600)
    binaries = {}
    for line in out.splitlines():
        entry = json.loads(line)
        if entry.get("reason") == "compiler-artifact" and entry.get("executable"):
            name = entry["target"]["name"]
            if name in CASES:
                binaries[name] = entry["executable"]
    require(set(binaries) == set(CASES), "missing freshly discovered test binaries")
    return binaries


def check_result(out, err):
    text = (out + err).decode(errors="strict")
    require("SKIP" not in text, "a required cross-device observation was skipped")
    require("test result: ok. 1 passed; 0 failed; 0 ignored;" in text,
            "expected exactly one passing, nonignored test")


def self_test():
    import unittest

    class Guards(unittest.TestCase):
        def test_identity(self):
            image, volume = Path("/private/owned/image.dmg"), Path("/private/owned/mount")
            entry = {"image-path": str(image), "system-entities": [
                {"dev-entry": "/dev/disk98"},
                {"dev-entry": "/dev/disk98s1", "mount-point": str(volume)},
            ]}
            self.assertEqual(image_devices(image_entry({"images": [entry]}, image), volume),
                             ("/dev/disk98", "/dev/disk98s1"))
            for info in ({"images": []}, {"images": [entry, entry]}):
                with self.assertRaises(RuntimeError):
                    image_entry(info, image)
            for bad in ("/dev/disk99s1", "/dev/disk98;bad"):
                entry["system-entities"][1]["dev-entry"] = bad
                with self.assertRaises(RuntimeError):
                    image_devices(entry, volume)

        def test_results(self):
            passed = b"test result: ok. 1 passed; 0 failed; 0 ignored; 5 filtered out"
            check_result(passed, b"")
            for out in (b"0 passed", b"1 failed", passed + b"SKIP missing fixture"):
                with self.assertRaises(RuntimeError):
                    check_result(out, b"")

        def test_private_scope(self):
            with tempfile.TemporaryDirectory() as name:
                root = Path(name).resolve()
                private(root, True)
                marker = root / "marker"
                marker.write_text(TOKEN)
                marker.chmod(0o600)
                private(marker, False)
                link = root / "link"
                link.symlink_to(marker)
                with self.assertRaises(RuntimeError):
                    private(link, False)
                os.link(marker, root / "hardlink")
                with self.assertRaises(RuntimeError):
                    private(marker, False)

    result = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(Guards))
    require(result.wasSuccessful(), "launcher self-test failed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--approved-disposable-image", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    require(args.approved_disposable_image, "explicit disposable-image approval is required")
    require(platform.system() == "Darwin" and os.geteuid() != 0,
            "requires a non-root macOS user")
    require(Path(HDIUTIL).is_file() and Path(DISKUTIL).is_file(), "system disk tools unavailable")
    binaries = test_binaries()  # Build before mounting; no tool installation.
    scope = Path(tempfile.mkdtemp(prefix="dort-cross-device-")).resolve()
    image, volume = scope / "fixture.dmg", scope / "mount"
    attached = False
    attach_attempted = False
    cleaned = False
    print(f"Owned disposable scope: {scope}", flush=True)
    try:
        private(scope, True)
        volume.mkdir(mode=0o700)
        local = scope / "local"
        local.mkdir(mode=0o700)
        run([HDIUTIL, "create", "-size", "128m", "-fs", "HFS+", "-type", "UDIF",
             "-layout", "GPTSPUD", "-volname", "DortDisposable", str(image)])
        image.chmod(0o600)
        metadata = private(image, False)
        require(metadata.st_size <= SIZE + 2 * 1024 * 1024, "unexpected image size")
        attach_attempted = True
        attachment = plist([HDIUTIL, "attach", str(image), "-nobrowse", "-noautoopen",
                            "-owners", "on", "-mountpoint", str(volume), "-plist"])
        attached = True
        whole, device = image_devices(attachment, volume)
        info = image_entry(plist([HDIUTIL, "info", "-plist"]), image)
        require(image_devices(info, volume) == (whole, device), "attached image identity mismatch")
        disk = plist([DISKUTIL, "info", "-plist", str(volume)])
        require(disk.get("DeviceNode") == device and disk.get("MountPoint") == str(volume),
                "mounted device identity mismatch")
        require(disk.get("FilesystemType") == "hfs" and 0 < disk.get("TotalSize", 0) <= SIZE,
                "expected bounded HFS+ volume")
        require(disk.get("GlobalPermissionsEnabled") is True, "volume ownership is disabled")
        require(volume.resolve() == volume and volume.stat().st_dev != local.stat().st_dev,
                "fixture devices must be canonical and distinct")
        fixture = volume / "fixtures"
        fixture.mkdir(mode=0o700)
        private(fixture, True)
        marker = fixture / ".dort-cross-device-fixture"
        with marker.open("x") as stream:
            stream.write(TOKEN)
        marker.chmod(0o600)
        private(marker, False)
        env = os.environ.copy()
        env["DORT_TEST_CROSS_DEVICE_ROOT"] = str(fixture)
        env["TMPDIR"] = str(local)
        for key in ("HOME", "XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME"):
            path = local / key.lower()
            path.mkdir(mode=0o700)
            env[key] = str(path)
        for name, case in CASES.items():
            out, err = run([binaries[name], "--exact", case, "--nocapture"], env=env)
            check_result(out, err)
            print(f"PASS {name}: {case}", flush=True)
        print(json.dumps({"platform": platform.platform(), "filesystem": "HFS+",
                          "image_bytes": metadata.st_size, "volume_bytes": disk["TotalSize"],
                          "host_device": local.stat().st_dev, "image_device": volume.stat().st_dev,
                          "cases_passed": len(CASES), "skips": 0}), flush=True)
    finally:
        # Even failed tests detach only the exact image created above. An uncertain
        # attach or any failed cleanup retains everything; never force/lazy detach.
        if attach_attempted:
            try:
                require(attached, "attachment did not finish; retain scope for inspection")
                info = image_entry(plist([HDIUTIL, "info", "-plist"]), image)
                whole, _ = image_devices(info, volume)
                run([HDIUTIL, "detach", whole])
                remaining = plist([HDIUTIL, "info", "-plist"])
                require(not any(Path(e.get("image-path", "")).resolve() == image
                                for e in remaining.get("images", [])), "image remains attached")
                require(volume.stat().st_dev == scope.stat().st_dev, "mount remains active")
                cleaned = True
            except BaseException as error:
                raise RuntimeError(f"STOP: retained exact scope {scope}; no force/delete: {error}") from error
        else:
            cleaned = True
        if cleaned:
            shutil.rmtree(scope)
            print("Normal detach/owned-scope cleanup complete.", flush=True)


if __name__ == "__main__":
    main()
