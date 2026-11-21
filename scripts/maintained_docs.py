"""Maintained Markdown paths shared by the documentation gates."""

from pathlib import Path

ROOT_FILES = ("README.md", "ARCHITECTURE.md", "AGENTS.md", "CHANGELOG.md")


def paths(root):
    """Root files, `docs/` guides and crate READMEs. Ignored `specs/` stays out."""
    root = Path(root)
    found = [root / name for name in ROOT_FILES]
    found.extend(sorted((root / "docs").rglob("*.md")))
    found.extend(sorted((root / "crates").rglob("README.md")))
    return found
