#!/usr/bin/env python3
"""Native release packaging/verification. Self-tests never build or publish anything."""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import re
import struct
import subprocess
import tarfile
import tempfile
import time
import unittest
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {
    "x86_64-unknown-linux-gnu": ("Linux", "x86_64", "Ubuntu 24.04 or newer (glibc 2.39+)"),
    "aarch64-unknown-linux-gnu": ("Linux", "aarch64", "Ubuntu 24.04 or newer (glibc 2.39+)"),
    "x86_64-apple-darwin": ("Darwin", "x86_64", "macOS 15 or newer"),
    "aarch64-apple-darwin": ("Darwin", "arm64", "macOS 15 or newer"),
}
MEMBERS = {"dort", "LICENSE", "BUILD-INFO.json", "THIRD-PARTY-NOTICES.txt", "RUST-COPYRIGHT.html"}
LIMIT = 64 * 1024 * 1024


def version():
    table = (ROOT / "Cargo.toml").read_text().partition("[workspace.package]\n")[2].split("\n[", 1)[0]
    matches = re.findall(r'^version = "([^"]+)"$', table, re.M)
    if len(matches) != 1:
        raise ValueError("expected one literal workspace version")
    return matches[0]


def validate_tag(tag, expected):
    if not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", tag) or tag != "v" + expected:
        raise ValueError("stable release tag must match the workspace version")


def head():
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()


def require_clean():
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT):
        raise ValueError("release packaging requires a clean source checkout")


def archive_name(tag, target):
    if target not in TARGETS:
        raise ValueError("unsupported release target")
    return f"dort-{tag}-{target}.tar.gz"


def check_binary(data, target):
    if "linux" in target:
        expected = 183 if target.startswith("aarch64") else 62
        valid = len(data) >= 20 and data[:6] == b"\x7fELF\x02\x01" and struct.unpack_from("<H", data, 18)[0] == expected
    else:
        expected = 0x100000C if target.startswith("aarch64") else 0x1000007
        valid = len(data) >= 8 and data[:4] == b"\xcf\xfa\xed\xfe" and struct.unpack_from("<I", data, 4)[0] == expected
    if not valid or len(data) > LIMIT:
        raise ValueError("binary format/architecture/size does not match target")


def dependency_notices(target):
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", target,
    ], cwd=ROOT, text=True))
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    pending = [p["id"] for p in packages.values() if p["name"] == "dort" and p["source"] is None]
    seen = set()
    while pending:
        identity = pending.pop()
        if identity in seen:
            continue
        seen.add(identity)
        pending.extend(d["pkg"] for d in nodes[identity]["deps"]
                       if any(k["kind"] != "dev" for k in d["dep_kinds"]))
    sections = ["Third-party dependency notices\n\nNormal/build dependency closure resolved for " + target + ".\nDeclared license expressions and upstream notice texts follow; inclusion does not relicense Dort or assert every listed component is linked.\n"]
    for package in sorted((packages[i] for i in seen if packages[i]["source"]), key=lambda p: (p["name"], p["version"])):
        root = Path(package["manifest_path"]).parent
        files = {p for p in root.iterdir() if p.is_file() and p.name.lower().startswith(
            ("license", "licence", "notice", "copying", "copyright"))}
        if package.get("license_file"):
            files.add(root / package["license_file"])
        if not package.get("license") or not files:
            raise ValueError("missing dependency license expression/notices: " + package["name"])
        sections.append(f"\n=== {package['name']} {package['version']} | {package['license']} ===\n")
        for path in sorted(files):
            if not path.resolve().is_relative_to(root.resolve()) or path.stat().st_size > 2 * 1024 * 1024:
                raise ValueError("unexpected dependency notice path/size")
            sections.append("\n--- " + path.name + " ---\n" + path.read_text())
    return "\n".join(sections).encode()


def pack(output, tag, target, commit, binary, rustc, notices, rust_copyright):
    check_binary(binary, target)
    output.mkdir(parents=True, exist_ok=True)
    name = archive_name(tag, target)
    info = {"version": tag[1:], "target": target, "commit": commit, "rustc": rustc,
            "minimum_runtime": TARGETS[target][2]}
    if not notices or not rust_copyright or max(len(notices), len(rust_copyright)) > 8 * 1024 * 1024:
        raise ValueError("missing or oversized third-party notice bundle")
    files = {"dort": binary, "LICENSE": (ROOT / "LICENSE").read_bytes(),
             "BUILD-INFO.json": (json.dumps(info, indent=2) + "\n").encode(),
             "THIRD-PARTY-NOTICES.txt": notices, "RUST-COPYRIGHT.html": rust_copyright}
    with (output / name).open("xb") as raw:
        with gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w") as archive:
                for path, data in files.items():
                    entry = tarfile.TarInfo(path)
                    entry.size = len(data)
                    entry.mode = 0o755 if path == "dort" else 0o644
                    archive.addfile(entry, io.BytesIO(data))
    digest = hashlib.sha256((output / name).read_bytes()).hexdigest()
    (output / (name + ".sha256")).write_text(f"{digest}  {name}\n")
    return name


def verify(output, tag, commit):
    names = {archive_name(tag, target) for target in TARGETS}
    expected = names | {name + ".sha256" for name in names}
    if {p.name for p in output.iterdir()} != expected:
        raise ValueError("release must contain exactly four archives and their checksums")
    sums = []
    for target in TARGETS:
        name = archive_name(tag, target)
        path = output / name
        if path.is_symlink() or not path.is_file() or path.stat().st_size > LIMIT:
            raise ValueError("unexpected archive identity/size")
        line = f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {name}\n"
        if (output / (name + ".sha256")).read_text() != line:
            raise ValueError("archive checksum mismatch")
        with tarfile.open(path, "r:gz") as archive:
            entries = archive.getmembers()
            if len(entries) != len(MEMBERS) or {e.name for e in entries} != MEMBERS:
                raise ValueError("unexpected archive entries")
            for entry in entries:
                if not entry.isfile() or entry.uid or entry.gid or entry.mtime or entry.size > LIMIT:
                    raise ValueError("unexpected archive metadata")
                if entry.mode != (0o755 if entry.name == "dort" else 0o644):
                    raise ValueError("unexpected archive permissions")
            check_binary(archive.extractfile("dort").read(), target)
            info = json.load(archive.extractfile("BUILD-INFO.json"))
            if info != {"version": tag[1:], "target": target, "commit": commit,
                        "rustc": info.get("rustc"), "minimum_runtime": TARGETS[target][2]}:
                raise ValueError("release provenance mismatch")
            if not info["rustc"].startswith("rustc 1.99.1 "):
                raise ValueError("unexpected Rust toolchain")
            for name in ["THIRD-PARTY-NOTICES.txt", "RUST-COPYRIGHT.html"]:
                if not 0 < archive.getmember(name).size <= 8 * 1024 * 1024:
                    raise ValueError("missing or oversized third-party notice bundle")
            if not archive.extractfile("THIRD-PARTY-NOTICES.txt").read().startswith(b"Third-party dependency notices\n"):
                raise ValueError("unexpected dependency notice bundle")
            if archive.extractfile("LICENSE").read() != (ROOT / "LICENSE").read_bytes():
                raise ValueError("license mismatch")
        sums.append(line)
    return "".join(sorted(sums))


def package(output, tag, target):
    require_clean()
    system, machine, _ = TARGETS[target]
    if (platform.system(), platform.machine()) != (system, machine):
        raise ValueError("release builds must run natively on their runtime platform")
    rustc = subprocess.check_output(["rustc", "-vV"], cwd=ROOT, text=True)
    if f"host: {target}\n" not in rustc or not rustc.startswith("rustc 1.99.1 "):
        raise ValueError("expected pinned native Rust toolchain")
    if system == "Darwin" and os.environ.get("MACOSX_DEPLOYMENT_TARGET") != "15.0":
        raise ValueError("macOS packages require explicit deployment target 15.0")
    if system == "Linux" and platform.libc_ver() != ("glibc", "2.39"):
        raise ValueError("Linux packages require reviewed Ubuntu 24.04 glibc baseline")
    result = subprocess.run(["cargo", "build", "--release", "--locked", "-p", "dort", "--target", target,
                             "--message-format=json"], cwd=ROOT, text=True, stdout=subprocess.PIPE, check=True)
    artifacts = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
    binaries = [a["executable"] for a in artifacts if a.get("reason") == "compiler-artifact"
                and a.get("target", {}).get("name") == "dort" and a.get("executable")]
    if len(binaries) != 1:
        raise ValueError("expected one fresh Cargo executable artifact")
    binary = Path(binaries[0])
    with tempfile.TemporaryDirectory(prefix="dort-release-smoke-") as home:
        env = os.environ | {"HOME": home, "XDG_CONFIG_HOME": home + "/config", "XDG_STATE_HOME": home + "/state"}
        observed = subprocess.check_output([str(binary), "--version"], env=env, cwd=home, text=True, timeout=15)
        if observed.strip() != "dort " + tag[1:] or list(Path(home).iterdir()):
            raise ValueError("isolated binary version/no-state smoke check failed")
    require_clean()
    notices = dependency_notices(target)
    sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip())
    rust_copyright = (sysroot / "share/doc/rust/COPYRIGHT-library.html").read_bytes()
    require_clean()
    print(pack(output, tag, target, head(), binary.read_bytes(), rustc.splitlines()[0], notices, rust_copyright))


def check_ci(commit):
    repository = os.environ["GITHUB_REPOSITORY"]
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ValueError("invalid GitHub repository")
    url = f"https://api.github.com/repos/{repository}/actions/workflows/ci.yml/runs?head_sha={commit}&status=success&per_page=100"
    headers = {"Accept": "application/vnd.github+json", "User-Agent": "dort-release",
               "Authorization": "Bearer " + os.environ["GH_TOKEN"]}
    for _ in range(30):
        with urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=30) as response:
            runs = json.load(response)["workflow_runs"]
        successful = [r for r in runs if r["head_sha"] == commit and r["conclusion"] == "success" and r["event"] == "push"]
        if successful:
            run_id = successful[0]["id"]
            if not isinstance(run_id, int):
                raise ValueError("unexpected CI run identity")
            with Path(os.environ["GITHUB_OUTPUT"]).open("a") as output:
                output.write(f"run_id={run_id}\n")
            print(f"PASS successful CI for exact release commit: {run_id}")
            return
        time.sleep(20)
    raise ValueError("no successful push CI for exact release commit")


def notes(tag):
    return f"""# Dort {tag}

Experimental, TUI-first local dotfile management with a shared Rust core and CLI automation.

- Guided local repository creation/attachment, profiles and standalone manifest checks.
- File discovery, reviewed adoption/link apply, Keep/Delete unmanage and guarded recovery.
- Read-only Git status/diff with deliberate patch disclosure; Diagnostics and contextual help.
- Keyboard-first compact/wide navigation, retained drafts and explicit write confirmations.

## Installation
Choose the archive matching your CPU and OS. Linux binaries require Ubuntu 24.04+ (glibc 2.39+); macOS binaries require macOS 15+. Verify its SHA-256 against `SHA256SUMS`, extract into an empty directory and run `./dort --version`. Checksums detect corruption, not an independent publisher signature. Each archive includes the MIT license, third-party dependency and Rust standard-library notices, and build provenance.

See the [getting-started guide](https://github.com/katesongsy/dort/blob/{tag}/docs/user/getting-started.md) for installation, a disposable-HOME walkthrough and TUI workflows, and the [safety guide](https://github.com/katesongsy/dort/blob/{tag}/docs/user/safety.md) before touching real configuration.

## Boundaries
Dort remains experimental, not a production-ready dotfile manager. No package-manager/provider execution, cloning/sync, Git staging/commit/push, templates or secrets. Repository/HOME/state mutations require supported same-filesystem operations. Recovery never automatically replays or overwrites later edits. Backups are private, not encrypted. Tests do not establish real power-loss or failing-fsync safety.

[Full generated history](https://github.com/katesongsy/dort/blob/{tag}/CHANGLOG.md). Rust 1.99.1 is required only for building from source; neither crate is published to crates.io.
"""


def self_test():
    class Guards(unittest.TestCase):
        def fixtures(self, output):
            for target in TARGETS:
                data = bytearray(64)
                if "linux" in target:
                    data[:6] = b"\x7fELF\x02\x01"
                    struct.pack_into("<H", data, 18, 183 if target.startswith("aarch64") else 62)
                else:
                    data[:4] = b"\xcf\xfa\xed\xfe"
                    struct.pack_into("<I", data, 4, 0x100000C if target.startswith("aarch64") else 0x1000007)
                pack(output, "v0.1.0", target, "a" * 40, bytes(data), "rustc 1.99.1 (fixture)", b"Third-party dependency notices\nfixture", b"Rust library fixture notice")
        def test_tag(self):
            validate_tag("v0.1.0", "0.1.0")
            for tag in ["v0.2.0", "v0.1.0-dev", "../v0.1.0"]:
                with self.assertRaises(ValueError): validate_tag(tag, "0.1.0")
        def test_valid(self):
            with tempfile.TemporaryDirectory() as scope:
                output = Path(scope); self.fixtures(output)
                self.assertEqual(len(verify(output, "v0.1.0", "a" * 40).splitlines()), 4)
        def test_missing_platform(self):
            with tempfile.TemporaryDirectory() as scope:
                output = Path(scope); self.fixtures(output)
                next(output.glob("*.sha256")).unlink()
                with self.assertRaises(ValueError): verify(output, "v0.1.0", "a" * 40)
        def test_corruption(self):
            with tempfile.TemporaryDirectory() as scope:
                output = Path(scope); self.fixtures(output)
                next(output.glob("*.tar.gz")).write_bytes(b"corrupt")
                with self.assertRaises(ValueError): verify(output, "v0.1.0", "a" * 40)
        def test_wrong_commit(self):
            with tempfile.TemporaryDirectory() as scope:
                output = Path(scope); self.fixtures(output)
                with self.assertRaises(ValueError): verify(output, "v0.1.0", "b" * 40)
        def test_missing_notices(self):
            with tempfile.TemporaryDirectory() as scope:
                with self.assertRaises(ValueError):
                    pack(Path(scope), "v0.1.0", "x86_64-apple-darwin", "a" * 40,
                         b"\xcf\xfa\xed\xfe" + struct.pack("<I", 0x1000007), "rustc 1.99.1 (fixture)", b"", b"")
        def test_wrong_architecture(self):
            with self.assertRaises(ValueError): check_binary(b"not a binary", "x86_64-apple-darwin")
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(Guards))
    if not result.wasSuccessful(): raise SystemExit(1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    for name in ["self-test", "package", "verify", "check-ci"]: mode.add_argument("--" + name, action="store_true")
    parser.add_argument("--tag")
    parser.add_argument("--target", choices=TARGETS)
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    args = parser.parse_args()
    if args.self_test:
        self_test(); return
    validate_tag(args.tag or "", version())
    if args.check_ci: check_ci(head())
    elif args.package:
        if not args.target: parser.error("--package requires --target")
        package(args.output, args.tag, args.target)
    else:
        require_clean()
        sums = verify(args.output, args.tag, head())
        for name, body in [("SHA256SUMS", sums), ("RELEASE.md", notes(args.tag))]:
            with (args.output / name).open("x") as output: output.write(body)
        print("PASS four archives, checksums, license, target and commit provenance")


if __name__ == "__main__":
    main()
