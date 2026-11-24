#!/usr/bin/env python3
from html.parser import HTMLParser
from pathlib import Path
import subprocess
import tempfile
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parent


class Page(HTMLParser):
    def __init__(self, path):
        super().__init__()
        self.ids = set()
        self.links = []
        self.frames = []
        self.size = None
        self.feed(path.read_text(encoding='utf-8'))

    def handle_starttag(self, tag, attrs):
        fields = dict(attrs)
        if tag == 'iframe':
            self.frames.append((fields.get('src'), fields.get('style')))
        if tag == 'main' and 'data-columns' in fields:
            self.size = fields['data-columns'], fields['data-rows']
        for key, value in attrs:
            if key == 'id':
                self.ids.add(value)
            elif key in ('href', 'src'):
                self.links.append(value)


def check_links():
    pages = {path: Page(path) for path in ROOT.glob('*.html')}
    for path, page in pages.items():
        for link in page.links:
            url = urlsplit(link)
            target = (path.parent / unquote(url.path)).resolve() if url.path else path
            if url.scheme or url.netloc or not target.is_relative_to(ROOT) or not target.exists():
                raise SystemExit(f'{path.name}: invalid offline link {link!r}')
            if url.fragment and unquote(url.fragment) not in Page(target).ids:
                raise SystemExit(f'{path.name}: missing anchor in {link!r}')
        for src, style in page.frames:
            columns, rows = Page(path.parent / src).size or (None, None)
            if not columns or style != f'--columns: {columns}; --rows: {rows}':
                raise SystemExit(f'{path.name}: update the frame size for {src!r}')


def check_captures():
    with tempfile.TemporaryDirectory(prefix='dort-design-') as scratch:
        subprocess.run([
            'cargo', 'run', '--locked', '--manifest-path', ROOT / 'reference/Cargo.toml',
            '--', 'gallery', scratch,
        ], check=True, cwd=ROOT)
        generated = {path.name: path.read_bytes() for path in Path(scratch).glob('*.html')}
    checked_in = {path.name: path.read_bytes() for path in (ROOT / 'captures').glob('*.html')}
    stale = sorted(name for name in generated.keys() | checked_in.keys()
                   if generated.get(name) != checked_in.get(name))
    if not generated or stale:
        raise SystemExit('Refresh the terminal captures. Difference: ' + ', '.join(stale))


if __name__ == '__main__':
    check_links()
    check_captures()
    print('Offline links and terminal captures are current.')
