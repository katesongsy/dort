#!/usr/bin/env python3
"""Export actual Ratatui component/workflow cells using synthetic data and fake providers."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True, help='Fresh output directory below repository temp/')
    args = parser.parse_args()
    temporary_root = ROOT / 'temp'
    temporary_root.mkdir(exist_ok=True)
    output = (ROOT / args.output).resolve()
    if not output.is_relative_to(temporary_root.resolve()) or output == temporary_root.resolve():
        parser.error('output must be a dedicated directory below repository temp/')
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        parser.error('output directory must be empty; never overwrite a visual review artifact')
    command = ['cargo', 'test', '-p', 'dort', '--locked', '--bin', 'dort', '--no-run', '--message-format=json']
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=300)
    if result.returncode:
        raise SystemExit('Gallery build failed; run cargo test -p dort --locked --bin dort --no-run for diagnostics')
    executable = None
    for line in result.stdout.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if message.get('reason') == 'compiler-artifact' and message.get('target', {}).get('name') == 'dort' and message.get('profile', {}).get('test') and message.get('executable'):
            executable = message['executable']
    if executable is None:
        raise SystemExit('Fresh Cargo test artifact not found')
    with tempfile.TemporaryDirectory(prefix='dort-gallery-') as directory:
        home = Path(directory).resolve()
        env = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(home / '.config'),
                   XDG_STATE_HOME=str(home / '.local/state'), PATH='/no-external-programs',
                   DORT_TUI_GALLERY_DIR=str(output))
        for probe in ['tui::design_system_tests::component_gallery_export_probe',
                      'tui::declared_workflow_tests::declared_workflow_gallery_export_probe']:
            result = subprocess.run([executable, probe, '--exact', '--nocapture'],
                                    cwd=ROOT, env=env, capture_output=True, text=True, timeout=30)
            if result.returncode:
                raise SystemExit('Isolated gallery probe failed: ' + result.stdout[-2000:] + result.stderr[-1000:])
    print(output / 'components.html')
    print(output / 'declared-tools.html')
    print('Fixture-only actual component/workflow cells; visual review remains required.')


if __name__ == '__main__':
    main()
