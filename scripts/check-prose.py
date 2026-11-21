#!/usr/bin/env python3
"""Enforce the writing rules on the shapes the rule names, stated as a negation of scope.

Only hollow negations are gated. A contrast that carries a fact a reader needs for a
safe decision, such as "Backups are private, not encrypted", stays.
"""
import re
from pathlib import Path
import sys
import unittest

from maintained_docs import paths as maintained_paths

ROOT = Path(__file__).resolve().parents[1]
SOURCES = ('crates/dort-core/src', 'crates/dort/src')
PROJECT_SKILLS = '.pi/skills'
RULES = (
    ('scope-negation', re.compile(r'\bIt does not\b'), 'state the scope as what it covers'),
    ('not-just', re.compile(r'\bnot just\b', re.I), 'state the point'),
)
FENCED = re.compile(r'^\s*(`{3,}|~{3,})')
INLINE_CODE = re.compile(r'`[^`\n]*`')


def comment_bodies(path):
    """Yield (line number, comment text) for source comments, skipping test modules."""
    test_start = None
    lines = path.read_text().splitlines()
    for number, line in enumerate(lines, 1):
        if re.match(r'\s*#\[cfg\(test\)\]', line):
            test_start = number
            break
    for number, line in enumerate(lines, 1):
        if test_start is not None and number >= test_start:
            return
        stripped = line.strip()
        if stripped.startswith('//'):
            yield number, stripped.lstrip('/').strip()


def markdown_prose(path):
    """Yield (line number, prose) for Markdown outside fenced blocks and inline code."""
    fence = None
    for number, line in enumerate(path.read_text().splitlines(), 1):
        match = FENCED.match(line)
        if match:
            marker = match[1]
            if fence is None:
                fence = marker
            elif marker[0] == fence[0] and len(marker) >= len(fence):
                fence = None
            continue
        if fence is None:
            yield number, INLINE_CODE.sub('', line)


def scan(text, rules=RULES):
    """Return the rule names matched in one prose line."""
    return [name for name, pattern, _ in rules if pattern.search(text)]


def check(root):
    root = Path(root)
    found = []
    for source_root in SOURCES:
        for path in sorted((root / source_root).rglob('*.rs')):
            if path.name.endswith('_tests.rs') or 'tests' in path.relative_to(root).parts:
                continue
            relative = path.relative_to(root)
            for number, text in comment_bodies(path):
                for rule in scan(text):
                    found.append((str(relative), number, rule))
    for path in maintained_paths(root):
        if not path.exists():
            continue
        relative = path.relative_to(root)
        for number, text in markdown_prose(path):
            for rule in scan(text):
                found.append((str(relative), number, rule))
    for path in sorted((root / PROJECT_SKILLS).rglob('*.md')):
        relative = path.relative_to(root)
        for number, text in markdown_prose(path):
            for rule in scan(text):
                found.append((str(relative), number, rule))
    return found


def violations(text):
    return scan(text)


class ProseCheckerTests(unittest.TestCase):
    def test_scope_negation_is_rejected(self):
        self.assertEqual(violations('It does not clone or sync.'), ['scope-negation'])
        self.assertEqual(violations('Checks declarations. It does not run package managers.'), ['scope-negation'])

    def test_hollow_promotion_is_rejected(self):
        self.assertEqual(violations('Reads metadata not just names.'), ['not-just'])

    def test_scope_stated_as_coverage_passes(self):
        self.assertEqual(violations('Checks declarations and references only.'), [])

    def test_contrast_carrying_a_needed_fact_passes(self):
        self.assertEqual(violations('Backups are private, not encrypted.'), [])
        self.assertEqual(violations('Resolve links rather than copying them.'), [])

    def test_inline_code_and_fences_are_not_prose(self):
        text = 'Run `It does not` first.\n\n```sh\n# It does not run here\n```\n'
        self.assertEqual([scan(t) for _, t in _markdown(text)], [[], []])

    def test_test_module_comments_are_ignored(self):
        source = '// Reads the plan.\n#[cfg(test)]\nmod tests {\n    // It does not matter here.\n}\n'
        path = _temporary_source(source)
        self.assertEqual(list(comment_bodies(path)), [(1, 'Reads the plan.')])

    def test_rule_names_stay_stable(self):
        self.assertEqual([name for name, _, _ in RULES], ['scope-negation', 'not-just'])


def _markdown(text):
    path = _temporary(text, '.md')
    return list(markdown_prose(path))


def _temporary_source(text):
    return _temporary(text, '.rs')


def _temporary(text, suffix):
    import tempfile
    handle = tempfile.NamedTemporaryFile('w', suffix=suffix, delete=False)
    handle.write(text)
    handle.close()
    path = Path(handle.name)
    unittest.TestCase().addCleanup(path.unlink)
    return path


if __name__ == '__main__':
    if sys.argv[1:] == ['--self-test']:
        unittest.main(argv=[sys.argv[0]])
    elif sys.argv[1:]:
        raise SystemExit('Usage: check-prose.py [--self-test]')
    else:
        errors = check(ROOT)
        for path, line, rule in errors:
            print(f'{path}:{line}: {rule}: {dict((n, fix) for n, _, fix in RULES)[rule]}')
        if errors:
            raise SystemExit(1)
        print('PASS comments and maintained docs state what the code does')
