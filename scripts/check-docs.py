#!/usr/bin/env python3
"""Check maintained root/docs Markdown recursively, without depending on ignored specs."""

import re
from pathlib import Path
import sys
import tempfile
import unittest
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parent.parent


def maintained_paths(root):
    paths = [root / name for name in ("README.md", "ARCHITECTURE.md", "AGENTS.md", "CHANGLOG.md")]
    paths.extend(sorted((root / "docs").rglob("*.md")))
    return paths


def prose(text):
    """Ignore fenced examples; this checker handles the guides' inline-link style."""
    fence = None
    for number, line in enumerate(text.splitlines(), 1):
        match = re.match(r"^\s*(`{3,}|~{3,})", line)
        if match:
            marker = match[1]
            if fence is None:
                fence = marker
            elif marker[0] == fence[0] and len(marker) >= len(fence):
                fence = None
        elif fence is None:
            yield number, line


def anchors(text):
    seen = {}
    result = set()
    for _, line in prose(text):
        match = re.match(r"^#{1,6}\s+(.+?)\s*#*\s*$", line)
        if not match:
            continue
        title = re.sub(r"\[([^]]+)\]\([^)]*\)", r"\1", match[1])
        slug = re.sub(r"[^\w\- ]", "", title.lower()).replace(" ", "-")
        count = seen.get(slug, 0)
        seen[slug] = count + 1
        result.add(slug if count == 0 else f"{slug}-{count}")
    return result


def check(root, paths):
    failures = []
    count = 0
    for path in paths:
        for number, line in prose(path.read_text()):
            for match in re.finditer(r"\[[^\]]*\]\(([^\s)]+)(?:\s+\"[^\"]*\")?\)", line):
                url = urlsplit(match[1])
                if url.scheme or url.netloc:
                    continue
                count += 1
                target = (path.parent / unquote(url.path)).resolve() if url.path else path
                problem = None
                if not target.is_relative_to(root):
                    problem = "link escapes repository"
                elif target.is_relative_to(root / "specs"):
                    problem = "link targets ignored specs"
                elif not target.exists():
                    problem = "missing file or directory"
                elif url.fragment and target.is_file() and target.suffix == ".md":
                    if unquote(url.fragment) not in anchors(target.read_text()):
                        problem = "missing heading fragment"
                if problem:
                    failures.append(f"{path.relative_to(root)}:{number}: {problem}: {match[1]}")
    return count, failures


class CheckerTests(unittest.TestCase):
    def fixture(self, text, other="# Heading\n"):
        temporary = tempfile.TemporaryDirectory(prefix="dort-docs-")
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name).resolve()
        page = root / "index.md"
        page.write_text(text)
        (root / "other.md").write_text(other)
        return check(root, [page])

    def test_valid_duplicate_headings_and_external_links(self):
        count, failures = self.fixture(
            "[one](other.md#heading) [two](other.md#heading-1) [web](https://example.invalid)\n",
            "# Heading\n# Heading\n",
        )
        self.assertEqual((count, failures), (2, []))

    def test_directory_links_are_valid(self):
        self.assertEqual(self.fixture("[directory](.)\n"), (1, []))

    def test_missing_file_and_fragment(self):
        count, failures = self.fixture("[file](absent.md) [fragment](other.md#absent)\n")
        self.assertEqual(count, 2)
        self.assertEqual(len(failures), 2)

    def test_fenced_examples_do_not_create_links(self):
        self.assertEqual(self.fixture("```md\n[example](absent.md)\n```\n"), (0, []))

    def test_paths_cannot_escape_repository(self):
        _, failures = self.fixture("[outside](../outside.md)\n")
        self.assertIn("escapes repository", failures[0])

    def test_ignored_spec_links_are_rejected_even_when_present(self):
        with tempfile.TemporaryDirectory(prefix="dort-docs-") as temporary:
            root = Path(temporary).resolve()
            (root / "specs").mkdir()
            (root / "specs" / "plan.md").write_text("# Temporary plan\n")
            page = root / "index.md"
            page.write_text("[plan](specs/plan.md) [absent](specs/absent.md)\n")
            count, failures = check(root, [page])
            self.assertEqual(count, 2)
            self.assertEqual(len(failures), 2)
            self.assertTrue(all("link targets ignored specs" in f for f in failures))

    def test_nested_user_and_technical_docs_are_scanned(self):
        with tempfile.TemporaryDirectory(prefix="dort-docs-") as temporary:
            root = Path(temporary).resolve()
            user = root / "docs" / "user" / "README.md"
            technical = root / "docs" / "internals" / "note.md"
            for page in (user, technical):
                page.parent.mkdir(parents=True, exist_ok=True)
                page.write_text("# Maintained documentation\n")
            paths = maintained_paths(root)
            self.assertIn(user, paths)
            self.assertIn(technical, paths)
            self.assertTrue(all(not p.is_relative_to(root / "specs") for p in paths))


def main():
    paths = maintained_paths(ROOT)
    count, failures = check(ROOT, paths)
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print(f"PASS {len(paths)} maintained Markdown files, {count} local links/fragments")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        unittest.main(argv=[sys.argv[0]])
    else:
        sys.exit(main())
