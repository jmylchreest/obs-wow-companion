#!/usr/bin/env python3
"""Export only aide decisions and reject machine-specific shared context before staging."""
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
SHARED = ROOT / '.aide/shared'
PRIVATE_PATH = re.compile(r'/home/[^\s/]+|/Users/[^\s/]+|/root/|[A-Za-z]:[\\/]Users[\\/][^\s\\/]+')


def check(directory=None):
    directory = SHARED if directory is None else directory
    errors = []
    for path in directory.rglob('*') if directory.exists() else []:
        if path.is_symlink():
            errors.append(f'{path.relative_to(directory)}: shared context must not contain symlinks')
        elif path.is_file() and path.suffix in ('.md', '.json'):
            for number, line in enumerate(path.read_text(encoding='utf-8').splitlines(), 1):
                if PRIVATE_PATH.search(line):
                    errors.append(f'{path.relative_to(directory)}:{number}: developer-specific absolute path; fix the source decision and export again')
    if errors:
        raise ValueError('\n'.join(errors))


def export():
    local = ROOT / '.aide/bin' / ('aide.exe' if os.name == 'nt' else 'aide')
    binary = str(local) if local.is_file() else shutil.which('aide')
    if not binary or not (ROOT / '.aide/memory').exists():
        print('aide decisions: no local binary/store; checking existing shared context only')
        check()
        return
    subprocess.run([binary, 'share', 'export', '--decisions'], cwd=ROOT, check=True)
    check()
    if SHARED.exists():
        subprocess.run(['git', 'add', '--', '.aide/shared'], cwd=ROOT, check=True)


if __name__ == '__main__':
    try:
        if sys.argv[1:] == ['export']:
            export()
        elif sys.argv[1:] == ['check']:
            check()
        else:
            sys.exit('Usage: python scripts/shared-context.py export|check')
    except (ValueError, subprocess.CalledProcessError) as error:
        sys.exit(str(error))
