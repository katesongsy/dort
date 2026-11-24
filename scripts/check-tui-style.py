#!/usr/bin/env python3
"""Enforce the shared TUI visual boundary; no dependencies or automatic blessing."""
import re
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
TUI = Path('crates/dort/src/tui')
STYLE_OWNERS = {TUI / 'components/mod.rs', TUI / 'components/design.rs', TUI / 'appearance.rs'}
COLLECTION_OWNER = TUI / 'components/collection.rs'
TABLE_OWNERS = {COLLECTION_OWNER, TUI / 'components/design.rs'}
READER_OWNER = TUI / 'components/reader.rs'
APPROVAL_OWNER = TUI / 'components/approval.rs'
GLYPH_OWNER = TUI / 'components/glyphs.rs'
CLOCK_OWNER = TUI / 'mod.rs'
CLOCK_OWNERS = {CLOCK_OWNER, TUI / 'app.rs'}  # Event loop and timestamped mouse boundary.


def mask_rust(source, strings):
    """Mask comments and optionally literals, retaining offsets/newlines for diagnostics."""
    out = list(source)
    i = 0
    size = len(source)

    def hide(a, b):
        for k in range(a, b):
            if out[k] != '\n':
                out[k] = ' '

    while i < size:
        if source.startswith('//', i):
            end = source.find('\n', i)
            end = size if end < 0 else end
            hide(i, end)
            i = end
        elif source.startswith('/*', i):
            start, depth = i, 1
            i += 2
            while i < size and depth:
                if source.startswith('/*', i):
                    depth += 1
                    i += 2
                elif source.startswith('*/', i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            hide(start, i)
        else:
            raw = re.match(r'(?:br|r)(#*)"', source[i:])
            char = re.match(r"'(?:\\(?:u\{[^}]*\}|x[0-9a-fA-F]{2}|.)|[^'\\\n])'", source[i:])
            if raw:
                end_marker = '"' + raw.group(1)
                end = source.find(end_marker, i + raw.end())
                end = size if end < 0 else end + len(end_marker)
                if strings:
                    hide(i, end)
                i = end
            elif source[i] == '"':
                start = i
                i += 1
                while i < size:
                    if source[i] == '\\':
                        i += 2
                    elif source[i] == '"':
                        i += 1
                        break
                    else:
                        i += 1
                if strings:
                    hide(start, min(i, size))
            elif char:
                if strings:
                    hide(i, i + char.end())
                i += char.end()
            else:
                i += 1
    return ''.join(out)


def violations(path, source):
    path = Path(path)
    if 'tests' in path.parts:
        return []
    code = mask_rust(source, strings=True)
    literals = mask_rust(source, strings=False)
    rules = []
    if path not in STYLE_OWNERS:
        rules += [
            ('raw-color', r'\bColor\b|\.\s*(?:fg|bg|underline_color)\s*\(', code),
            ('raw-emphasis', r'\bModifier\b|\.\s*(?:add_modifier|remove_modifier|bold|italic|underlined|reversed|dim)\s*\(', code),
        ]
    if path != GLYPH_OWNER:
        rules.append(('semantic-glyph', r'[✓×▸▾›█░\u2800-\u28ff]', literals))
    if path not in CLOCK_OWNERS:
        rules.append(('private-clock', r'\bInstant\s*::|\b(?:set_interval|setInterval)\s*\(', code))
    if path not in TABLE_OWNERS:
        rules.append(('raw-table', r'\b(?:Table|TableState)\b|\.\s*row_highlight_style\s*\(', code))
    if path != COLLECTION_OWNER:
        rules += [
            ('private-selection-marker', r'\.\s*highlight_(?:symbol|spacing)\s*\(', code),
            ('private-row-wheel', r'\.\s*wheel\s*\([^;]*,\s*(?:\w+\s*::\s*)*Wheel\s*::\s*(?:Rows|Choices|PreviewRows)\s*\)', code),
        ]
    if path != READER_OWNER:
        rules.append(('unshared-report-reader', r'\bParagraph\s*::\s*new\s*\(\s*(?:\w+\s*::\s*)*review_text\s*\(', code))
    if path != APPROVAL_OWNER:
        primary = r'(?:\w+\s*::\s*)*Target\s*::\s*(?:ConfirmAction|Continue)\b'
        rules.append(('unshared-approval-control', rf'\bpresented\s*\([^;]*,\s*{primary}\s*,?\s*\)|\bActionButton\s*\{{[^{{}}]*\btarget\s*:\s*{primary}', code))
    rules.append(('literal-selection-marker', r'\.\s*highlight_symbol\s*\(\s*(?:"|r#*")', literals))
    rules.append(('blocking-animation', r'\bthread\s*::\s*sleep\s*\(', code))
    found = []
    for rule, pattern, text in rules:
        for match in re.finditer(pattern, text):
            line = source.count('\n', 0, match.start()) + 1
            item = (str(path), line, rule)
            if item not in found:
                found.append(item)
    return sorted(found)


def check(root):
    found = []
    for path in sorted((root / TUI).rglob('*.rs')):
        found.extend(violations(path.relative_to(root), path.read_text()))
    return found


class StyleCheckerTests(unittest.TestCase):
    def test_page_cannot_invent_colors_emphasis_glyphs_or_clock(self):
        source = 'let s = Style::default().fg(Color::Red).bold();\nlet icon = "✓";\nInstant::now();\n'
        rules = {item[2] for item in violations(TUI / 'page.rs', source)}
        self.assertEqual(rules, {'raw-color', 'raw-emphasis', 'semantic-glyph', 'private-clock'})

    def test_aliases_and_shorthand_methods_do_not_bypass_boundary(self):
        source = 'use ratatui::style::Color as Paint;\nstyle.bg(paint).add_modifier(bits);'
        self.assertEqual({v[2] for v in violations(TUI / 'page.rs', source)}, {'raw-color', 'raw-emphasis'})

    def test_comments_and_quoted_rust_are_not_code(self):
        source = '/* outer Color /* inner Modifier */ end */\n// ✓ Color::Red\nlet x = r##"Color::Red // /*"##; let y = "Modifier::BOLD \\";\n'
        self.assertEqual(violations(TUI / 'page.rs', source), [])

    def test_glyph_literals_are_checked_but_metadata_punctuation_is_not(self):
        self.assertEqual(violations(TUI / 'page.rs', 'let title = "Package · version";'), [])
        self.assertEqual(violations(TUI / 'page.rs', 'let marker = r#"›"#;')[0][2], 'semantic-glyph')

    def test_selection_marker_uses_shared_glyph_even_for_ascii(self):
        for literal in ['"> "', 'r#"> "#']:
            self.assertEqual({v[2] for v in violations(COLLECTION_OWNER, '.highlight_symbol(' + literal + ')')}, {'literal-selection-marker'})
        symbol = '.highlight_symbol(format!("{} ", ui::glyphs::SELECTED))'
        self.assertEqual(violations(COLLECTION_OWNER, symbol), [])
        self.assertEqual(violations(TUI / 'page.rs', symbol)[0][2], 'private-selection-marker')
        self.assertEqual(violations(TUI / 'page.rs', '// .highlight_symbol("> ")'), [])

    def test_table_and_wheel_bypasses_include_aliases_and_qualified_paths(self):
        source = ('use ratatui::widgets::{Table as Grid, TableState as GridState};\n'
                  'hits.wheel(area, crate::tui::input::Wheel::PreviewRows);\n'
                  'let x = grid.row_highlight_style(ui::selected());')
        self.assertEqual({v[2] for v in violations(TUI / 'page.rs', source)}, {'raw-table', 'private-row-wheel'})
        self.assertEqual(violations(COLLECTION_OWNER, source), [])
        self.assertEqual(violations(TUI / 'components/design.rs', 'Table::new(rows, widths)'), [])
        self.assertEqual(violations(TUI / 'components/new_widget.rs', 'Table::new(rows, widths)')[0][2], 'raw-table')
        self.assertEqual(violations(TUI / 'page.rs', 'Row::new(facts); hits.wheel(area, Wheel::Content);'), [])

    def test_review_uses_reader_and_approval_control(self):
        path = TUI / 'workbench.rs'
        source = ('Paragraph::new(ui::review_text(&preview.text));\n'
                  'app.presented(label, "Enter", Target::Continue);\n'
                  'hits.present(ui::ActionButton { label, key, target: Target::ConfirmAction });')
        self.assertEqual([(v[1], v[2]) for v in violations(path, source)], [
            (1, 'unshared-report-reader'), (2, 'unshared-approval-control'), (3, 'unshared-approval-control'),
        ])
        self.assertEqual(violations(path, 'approval.present(hits, label, Target::Continue);'), [])
        self.assertEqual(violations(path, 'Paragraph::new(patch_text);'), [])
        self.assertEqual(violations(path, 'match target { Target::Continue => apply(), _ => {} }'), [])

    def test_owners_are_exact_not_any_file_in_component_folder(self):
        source = 'let s = Color::Red; let icon = "✓";'
        self.assertEqual({v[2] for v in violations(TUI / 'components/design.rs', source)}, {'semantic-glyph'})
        self.assertEqual({v[2] for v in violations(TUI / 'components/new_widget.rs', source)}, {'raw-color', 'semantic-glyph'})
        self.assertEqual(violations(GLYPH_OWNER, 'let icon = "✓";'), [])

    def test_test_fixtures_may_assert_raw_cells(self):
        self.assertEqual(violations(TUI / 'tests/design.rs', 'Color::Red; "✓"'), [])

    def test_clock_boundary_does_not_allow_blocking_sleep(self):
        self.assertEqual({v[2] for v in violations(CLOCK_OWNER, 'Instant::now(); thread::sleep(delay);')}, {'blocking-animation'})

    def test_multiline_diagnostics_keep_original_line_numbers(self):
        self.assertEqual(violations(TUI / 'page.rs', '/*\n Color\n*/\nColor::Red;')[0][1], 4)


if __name__ == '__main__':
    if sys.argv[1:] == ['--self-test']:
        unittest.main(argv=[sys.argv[0]])
    elif sys.argv[1:]:
        raise SystemExit('Usage: check-tui-style.py [--self-test]')
    else:
        errors = check(ROOT)
        for path, line, rule in errors:
            print(f'{path}:{line}: {rule}: use the shared components/ design-system vocabulary')
        if errors:
            raise SystemExit(1)
        print('PASS TUI shared styling, collection, reader, approval and motion boundaries')
