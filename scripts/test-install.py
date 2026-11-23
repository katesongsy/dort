#!/usr/bin/env python3
"""Offline installer checks. A loopback HTTP server serves a fake release; no real network."""

import argparse
import functools
import hashlib
import http.server
import io
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile
import threading
import unittest

ROOT = Path(__file__).resolve().parents[1]
INSTALLER = ROOT / "install.sh"
BASH = shutil.which("bash") or "/bin/bash"
TAG = "v9.9.9"
FAKE_VERSION = "dort 9.9.9"
TIMEOUT = 60
ARCHITECTURES = {
    "arm64": "aarch64",
    "aarch64": "aarch64",
    "x86_64": "x86_64",
    "amd64": "x86_64",
    "x86-64": "x86_64",
}
HOST_SUFFIXES = {"Darwin": "apple-darwin", "Linux": "unknown-linux-gnu"}
# Tools install.sh calls, mirrored into a symlink farm. Git, Homebrew and Node.js stay out.
FARM_TOOLS = (
    "uname", "mktemp", "tar", "gzip", "curl", "wget", "shasum", "sha256sum", "awk",
    "cp", "chmod", "mv", "rm", "mkdir", "id", "cat",
)


def host_target(system, machine):
    """Map uname output to a release target, as install.sh does."""
    architecture = ARCHITECTURES.get(machine)
    suffix = HOST_SUFFIXES.get(system)
    if architecture is None or suffix is None:
        raise ValueError(f"unsupported host {system}/{machine}")
    return f"{architecture}-{suffix}"


def fake_binary(version=FAKE_VERSION):
    """A stand-in dort binary that answers --version."""
    return (
        "#!/bin/sh\n"
        "if [ \"$1\" = \"--version\" ]; then\n"
        f"  printf '%s\\n' '{version}'\n"
        "  exit 0\n"
        "fi\n"
        "exit 0\n"
    ).encode()


def archive_bytes(payload, member="dort"):
    """Build the release archive shape: one executable member named dort."""
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
        entry = tarfile.TarInfo(member)
        entry.size = len(payload)
        entry.mode = 0o755
        entry.mtime = 0
        archive.addfile(entry, io.BytesIO(payload))
    return buffer.getvalue()


def checksums(entries):
    """Render a SHA256SUMS body from (digest, name) pairs."""
    return "".join(f"{digest}  {name}\n" for digest, name in entries)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def release(directory, target, *, covered=None, corrupt=False):
    """Write one release fixture. `corrupt` flips archive bytes after the checksums are written."""
    directory.mkdir(parents=True, exist_ok=True)
    name = f"dort-{TAG}-{target}.tar.gz"
    data = archive_bytes(fake_binary())
    (directory / name).write_bytes(data)
    listed = name if covered is None else covered
    (directory / "SHA256SUMS").write_text(checksums([(digest(data), listed)]))
    if corrupt:
        flipped = bytearray(data)
        flipped[len(flipped) // 2] ^= 0xFF
        (directory / name).write_bytes(bytes(flipped))
    return name


def version_of(binary):
    """Run a candidate dort binary and return its version line."""
    result = subprocess.run(
        [str(binary), "--version"], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        text=True, check=True, timeout=30, env={"PATH": "/usr/bin:/bin", "LC_ALL": "C"},
    )
    return result.stdout.strip()


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *arguments):
        pass


def serve(directory):
    server = http.server.ThreadingHTTPServer(
        ("127.0.0.1", 0), functools.partial(QuietHandler, directory=str(directory)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def run_installer(scope, base, *arguments, path=None, target=None):
    """Run install.sh with a detached session, so no terminal prompt can reach a person."""
    environment = {
        "HOME": str(scope / "home"),
        "PATH": path or os.environ.get("PATH", "/usr/bin:/bin"),
        "TMPDIR": str(scope / "tmp"),
        "DORT_RELEASE_BASE": base,
        "LC_ALL": "C",
    }
    if target is not None:
        environment["DORT_TARGET"] = target
    return subprocess.run(
        [BASH, str(INSTALLER), *arguments],
        stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        text=True, env=environment, start_new_session=True, timeout=TIMEOUT, check=False,
    )


def describe(result):
    return f"exit {result.returncode}\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}"


class InstallerCases(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix="dort-install-test-")
        cls.root = Path(cls.temporary.name).resolve()
        cls.server = serve(cls.root)
        cls.url = f"http://127.0.0.1:{cls.server.server_address[1]}"
        cls.host = host_target(platform.system(), platform.machine())

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.temporary.cleanup()

    def scope(self, name):
        """A private run directory with HOME and TMPDIR, both normally empty."""
        directory = self.root / name
        (directory / "home").mkdir(parents=True)
        (directory / "tmp").mkdir()
        return directory

    def base(self, directory):
        return f"{self.url}/{directory.relative_to(self.root).as_posix()}"

    def fixture(self, scope, **options):
        directory = scope / "release"
        release(directory, self.host, **options)
        return self.base(directory)

    def farm(self, scope):
        """PATH holding only the tools install.sh needs, without git, Homebrew or Node.js."""
        directory = scope / "farm"
        directory.mkdir()
        real = os.environ.get("PATH", "/usr/bin:/bin")
        for name in FARM_TOOLS:
            found = shutil.which(name, path=real)
            if found:
                (directory / name).symlink_to(found)
        if not (directory / "curl").exists() and not (directory / "wget").exists():
            self.skipTest("host has no curl or wget to place in the farm")
        return str(directory)

    def test_fresh_install(self):
        scope = self.scope("fresh")
        install_dir = scope / "bin"
        result = run_installer(scope, self.fixture(scope), "--install-dir", str(install_dir), "--skip-deps")
        self.assertEqual(result.returncode, 0, describe(result))
        binary = install_dir / "dort"
        self.assertTrue(binary.is_file(), describe(result))
        self.assertTrue(os.access(binary, os.X_OK), "installed binary is not executable")
        self.assertEqual(version_of(binary), FAKE_VERSION)
        self.assertEqual(binary.read_bytes(), fake_binary())
        self.assertIn(str(binary), result.stdout)

    def test_rerun_replaces_binary(self):
        scope = self.scope("rerun")
        install_dir = scope / "bin"
        base = self.fixture(scope)
        first = run_installer(scope, base, "--install-dir", str(install_dir), "--skip-deps")
        self.assertEqual(first.returncode, 0, describe(first))
        binary = install_dir / "dort"
        second = run_installer(scope, base, "--install-dir", str(install_dir), "--skip-deps")
        self.assertEqual(second.returncode, 0, describe(second))
        self.assertEqual(binary.read_bytes(), fake_binary())
        self.assertEqual(version_of(binary), FAKE_VERSION)
        self.assertEqual(sorted(path.name for path in install_dir.iterdir()), ["dort"])

    def test_corrupt_archive_is_refused(self):
        scope = self.scope("corrupt")
        base = self.fixture(scope, corrupt=True)
        empty = scope / "empty-bin"
        result = run_installer(scope, base, "--install-dir", str(empty), "--skip-deps")
        self.assertNotEqual(result.returncode, 0, describe(result))
        self.assertIn("SHA-256 mismatch", result.stderr)
        self.assertFalse((empty / "dort").exists(), "a corrupt archive installed a binary")
        kept = scope / "kept-bin"
        kept.mkdir()
        existing = kept / "dort"
        existing.write_bytes(fake_binary("dort 0.0.1"))
        existing.chmod(0o755)
        result = run_installer(scope, base, "--install-dir", str(kept), "--skip-deps")
        self.assertNotEqual(result.returncode, 0, describe(result))
        self.assertEqual(version_of(existing), "dort 0.0.1")
        self.assertEqual(sorted(path.name for path in kept.iterdir()), ["dort"])

    def test_missing_checksum_entry_is_refused(self):
        scope = self.scope("checksums")
        base = self.fixture(scope, covered=f"dort-{TAG}-riscv64-unknown-linux-gnu.tar.gz")
        install_dir = scope / "bin"
        result = run_installer(scope, base, "--install-dir", str(install_dir), "--skip-deps")
        self.assertNotEqual(result.returncode, 0, describe(result))
        self.assertIn("no checksum entry", result.stderr)
        self.assertFalse((install_dir / "dort").exists())

    def test_unknown_target_is_refused(self):
        scope = self.scope("target")
        base = self.fixture(scope)
        install_dir = scope / "bin"
        result = run_installer(scope, base, "--install-dir", str(install_dir), "--skip-deps",
                               target="bogus-target")
        self.assertNotEqual(result.returncode, 0, describe(result))
        self.assertIn("unsupported host", result.stderr)
        self.assertIn("bogus-target", result.stderr)
        self.assertFalse((install_dir / "dort").exists())

    def test_missing_prerequisite_with_piped_stdin(self):
        scope = self.scope("prompt")
        base = self.fixture(scope)
        install_dir = scope / "bin"
        result = run_installer(scope, base, "--install-dir", str(install_dir), path=self.farm(scope))
        self.assertEqual(result.returncode, 0, describe(result))
        for name in ("brew", "git", "node"):
            self.assertIn(f"Skipped {name}. Install it with:", result.stdout)
        self.assertNotIn("Installed brew", result.stdout)
        self.assertEqual(sorted(path.name for path in (scope / "home").iterdir()), [],
                         "a prerequisite installer wrote into HOME")
        self.assertEqual(version_of(install_dir / "dort"), FAKE_VERSION)

    def test_help_and_version_flags(self):
        scope = self.scope("flags")
        base = self.fixture(scope)
        helped = run_installer(scope, base, "--help")
        self.assertEqual(helped.returncode, 0, describe(helped))
        self.assertIn("--install-dir", helped.stdout)
        printed = run_installer(scope, base, "--version")
        self.assertEqual(printed.returncode, 0, describe(printed))
        self.assertEqual(printed.stdout.strip(), "1.0.0")
        unknown = run_installer(scope, base, "--bogus")
        self.assertEqual(unknown.returncode, 2, describe(unknown))


def self_test():
    class Guards(unittest.TestCase):
        def test_host_target(self):
            self.assertEqual(host_target("Darwin", "arm64"), "aarch64-apple-darwin")
            self.assertEqual(host_target("Darwin", "x86_64"), "x86_64-apple-darwin")
            self.assertEqual(host_target("Darwin", "amd64"), "x86_64-apple-darwin")
            self.assertEqual(host_target("Darwin", "x86-64"), "x86_64-apple-darwin")
            self.assertEqual(host_target("Linux", "aarch64"), "aarch64-unknown-linux-gnu")
            self.assertEqual(host_target("Linux", "x86_64"), "x86_64-unknown-linux-gnu")
            with self.assertRaises(ValueError):
                host_target("Plan9", "x86_64")
            with self.assertRaises(ValueError):
                host_target("Linux", "s390x")

        def test_archive_matches_release_layout(self):
            payload = fake_binary()
            with tarfile.open(fileobj=io.BytesIO(archive_bytes(payload)), mode="r:gz") as archive:
                members = archive.getmembers()
                self.assertEqual([entry.name for entry in members], ["dort"])
                self.assertTrue(members[0].isfile())
                self.assertEqual(members[0].mode, 0o755)
                self.assertEqual(archive.extractfile("dort").read(), payload)

        def test_checksum_line_shape(self):
            body = checksums([("a" * 64, f"dort-{TAG}-x86_64-apple-darwin.tar.gz")])
            self.assertEqual(body, "a" * 64 + f"  dort-{TAG}-x86_64-apple-darwin.tar.gz\n")
            self.assertEqual(digest(b""), hashlib.sha256(b"").hexdigest())

        def test_corruption_lands_after_the_checksums(self):
            with tempfile.TemporaryDirectory() as name:
                directory = Path(name) / "release"
                written = release(directory, "aarch64-apple-darwin", corrupt=True)
                recorded = (directory / "SHA256SUMS").read_text().split("  ")[0]
                self.assertNotEqual(digest((directory / written).read_bytes()), recorded)
                self.assertEqual(len(recorded), 64)

    result = unittest.TextTestRunner(verbosity=2).run(
        unittest.defaultTestLoader.loadTestsFromTestCase(Guards))
    if not result.wasSuccessful():
        raise SystemExit(1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="check the fixture helpers only")
    arguments = parser.parse_args()
    if arguments.self_test:
        self_test()
        return
    result = unittest.TextTestRunner(verbosity=2).run(
        unittest.defaultTestLoader.loadTestsFromTestCase(InstallerCases))
    raise SystemExit(0 if result.wasSuccessful() else 1)


if __name__ == "__main__":
    main()
