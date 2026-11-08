#!/usr/bin/env python3
"""Mount-free source-package guards; never upload or access registry credentials."""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import posixpath
import re
import subprocess
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
NAMES = ("dort-core", "dort")
LIMIT = 16 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def resources(files):
    """Check literal Rust include_str!/include_bytes! resources without executing Rust."""
    for name, data in files.items():
        if not name.endswith(".rs"):
            continue
        for relative in re.findall(r'include_(?:str|bytes)!\(\s*"([^"\n]+)"\s*\)', data.decode()):
            path = posixpath.normpath(str(PurePosixPath(name).parent / relative))
            require(not path.startswith(("../", "/")) and path in files,
                    "literal Rust include must resolve inside the source package")


def metadata():
    return json.loads(subprocess.check_output([
        "cargo", "metadata", "--locked", "--no-deps", "--format-version", "1",
    ], cwd=ROOT, text=True))


def source(package, root=ROOT):
    name, version = package["name"], package["version"]
    directory = root / "crates" / name
    require(package["publish"] == ["crates-io"], "publication must target crates.io explicitly")
    require(package["license"] == "MIT" and package["description"], "missing license/description")
    require(package["repository"] == "https://github.com/katesongsy/dort", "unexpected repository")
    require(Path(package["readme"]).name == "README.md", "missing package README")
    require((directory / "LICENSE").read_bytes() == (root / "LICENSE").read_bytes(), "license copy differs")
    require((directory / "examples/dort.toml").read_bytes() == (root / "examples/dort.toml").read_bytes(),
            "example copy differs")
    if name == "dort":
        dependencies = [d for d in package["dependencies"] if d["name"] == "dort-core" and d["kind"] is None]
        require(len(dependencies) == 1 and dependencies[0]["req"] == "^" + version,
                "CLI must declare the matching registry core version")
    files = {}
    for folder in ("src", "tests", "examples"):
        for path in (directory / folder).rglob("*"):
            if path.is_file():
                require(not path.is_symlink(), "source package must not traverse symlinks")
                files[path.relative_to(directory).as_posix()] = path.read_bytes()
    for name in ("LICENSE", "README.md", "Cargo.toml"):
        files[name] = (directory / name).read_bytes()
    resources(files)
    return files


def unpack(path, name, version):
    require(path.is_file() and not path.is_symlink() and path.stat().st_size <= LIMIT,
            "unexpected source archive identity/size")
    prefix = name + "-" + version + "/"
    files = {}
    with tarfile.open(path, "r:gz") as archive:
        entries = archive.getmembers()
        require(len(entries) <= 1000 and sum(e.size for e in entries) <= LIMIT, "source archive exceeds bounds")
        for entry in entries:
            require(entry.isfile() and entry.name.startswith(prefix), "unexpected source archive entry")
            name = entry.name[len(prefix):]
            require(name and posixpath.normpath(name) == name and not name.startswith(("../", "/"))
                    and name not in files, "unsafe/duplicate source archive path")
            files[name] = archive.extractfile(entry).read()
    return files


def archive(package, path, expected, commit):
    files = unpack(path, package["name"], package["version"])
    allowed = set(expected) | {"Cargo.toml.orig", "Cargo.lock", ".cargo_vcs_info.json"}
    require(set(files) == allowed, "source archive has missing or unexpected files")
    for name, data in expected.items():
        if name != "Cargo.toml":
            require(files[name] == data, "packaged source/resource differs from checkout")
    require(files["Cargo.toml.orig"] == expected["Cargo.toml"], "original manifest differs")
    vcs = json.loads(files[".cargo_vcs_info.json"])
    require(vcs.get("git", {}).get("sha1") == commit and not vcs.get("git", {}).get("dirty", False),
            "archive must bind the clean current commit")
    require(vcs.get("path_in_vcs") == "crates/" + package["name"], "unexpected source provenance")
    normalized = files["Cargo.toml"].decode()
    require("workspace = true" not in normalized and "[workspace]" not in normalized,
            "published manifest must be standalone")
    if package["name"] == "dort":
        require(re.search(r'\[dependencies\.dort-core\]\nversion = "' + re.escape(package["version"]) + r'"\n', normalized),
                "published CLI must resolve the versioned registry dependency")
    resources(files)
    return hashlib.sha256(path.read_bytes()).hexdigest()


def self_test():
    class Guards(unittest.TestCase):
        def test_resources(self):
            resources({"src/main.rs": b'include_str!("../examples/dort.toml")', "examples/dort.toml": b"schema=1"})
        def test_missing_resource(self):
            with self.assertRaises(ValueError):
                resources({"src/main.rs": b'include_str!("../examples/missing.toml")'})
        def test_external_resource(self):
            with self.assertRaises(ValueError):
                resources({"src/main.rs": b'include_str!("../../outside.toml")', "../outside.toml": b"outside"})
        def tar(self, directory, names):
            path = directory / "fixture.crate"
            with tarfile.open(path, "w:gz") as tar:
                for name in names:
                    entry = tarfile.TarInfo(name); entry.size = 1
                    tar.addfile(entry, io.BytesIO(b"x"))
            return path
        def test_archive_paths(self):
            with tempfile.TemporaryDirectory() as tmp:
                path = self.tar(Path(tmp), ["dort-0.1.0/src/main.rs"])
                self.assertEqual(unpack(path, "dort", "0.1.0"), {"src/main.rs": b"x"})
        def test_archive_escape(self):
            with tempfile.TemporaryDirectory() as tmp:
                path = self.tar(Path(tmp), ["dort-0.1.0/../outside"])
                with self.assertRaises(ValueError): unpack(path, "dort", "0.1.0")
        def test_archive_duplicate(self):
            with tempfile.TemporaryDirectory() as tmp:
                path = self.tar(Path(tmp), ["dort-0.1.0/LICENSE", "dort-0.1.0/LICENSE"])
                with self.assertRaises(ValueError): unpack(path, "dort", "0.1.0")
        def fixture(self, directory, dirty=False, changed=False):
            package = {"name": "dort-core", "version": "0.1.0"}
            expected = {"Cargo.toml": b'[package]\nname = "dort-core"\n',
                        "src/lib.rs": b"pub fn fixture() {}", "LICENSE": b"MIT", "README.md": b"Fixture"}
            files = expected | {"Cargo.toml.orig": expected["Cargo.toml"], "Cargo.lock": b"version = 4\n",
                                ".cargo_vcs_info.json": json.dumps({"git": {"sha1": "a" * 40, "dirty": dirty},
                                                                    "path_in_vcs": "crates/dort-core"}).encode()}
            if changed:
                files["src/lib.rs"] = b"pub fn substituted() {}"
            path = directory / "fixture.crate"
            with tarfile.open(path, "w:gz") as tar:
                for name, data in files.items():
                    entry = tarfile.TarInfo("dort-core-0.1.0/" + name); entry.size = len(data)
                    tar.addfile(entry, io.BytesIO(data))
            return package, path, expected
        def test_clean_archive(self):
            with tempfile.TemporaryDirectory() as tmp:
                self.assertEqual(len(archive(*self.fixture(Path(tmp)), "a" * 40)), 64)
        def test_dirty_archive(self):
            with tempfile.TemporaryDirectory() as tmp:
                with self.assertRaises(ValueError): archive(*self.fixture(Path(tmp), dirty=True), "a" * 40)
        def test_wrong_commit(self):
            with tempfile.TemporaryDirectory() as tmp:
                with self.assertRaises(ValueError): archive(*self.fixture(Path(tmp)), "b" * 40)
        def test_changed_source(self):
            with tempfile.TemporaryDirectory() as tmp:
                with self.assertRaises(ValueError): archive(*self.fixture(Path(tmp), changed=True), "a" * 40)
    result = unittest.TextTestRunner(verbosity=1).run(unittest.defaultTestLoader.loadTestsFromTestCase(Guards))
    if not result.wasSuccessful():
        raise SystemExit(1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--archives", action="store_true", help="also verify clean Cargo package archives")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    data = metadata()
    packages = {p["name"]: p for p in data["packages"]}
    require(set(packages) == set(NAMES), "unexpected workspace packages")
    require(len({p["version"] for p in packages.values()}) == 1, "workspace package versions differ")
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    if args.archives:
        require(not subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT), "archive validation requires a clean checkout")
    for name in NAMES:
        package = packages[name]
        files = source(package)
        if args.archives:
            folder = Path(data["target_directory"]) / "package"
            paths = [folder / (name + "-" + package["version"] + ".crate"),
                     folder / "tmp-crate" / (name + "-" + package["version"] + ".crate")]
            path = next((p for p in paths if p.is_file()), None)
            require(path is not None, "run cargo package --workspace --locked first")
            digest = archive(package, path, files, commit)
            print(f"PASS {name} {package['version']}: clean source archive sha256={digest}")
        else:
            print(f"PASS {name} {package['version']}: publication metadata, license, example and literal resources")


if __name__ == "__main__":
    main()
