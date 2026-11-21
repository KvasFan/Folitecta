#!/usr/bin/env python3
"""Use the workspace version for tagged releases and branch preview artifacts alike."""
import os
import re
from pathlib import Path

manifest = Path('Cargo.toml').read_text()
section = re.search(r'(?ms)^\[workspace\.package\]\n(.*?)(?=^\[|\Z)', manifest)
match = re.search(r'^version\s*=\s*"(\d+\.\d+\.\d+)"\s*$', section[1], re.MULTILINE) if section else None
if match is None:
    raise SystemExit('Cannot read the workspace release version')
version = match[1]
tag = os.environ.get('RELEASE_TAG', '')
if tag.startswith('v') and tag != 'v' + version:
    raise SystemExit(f'Release tag {tag!r} does not match workspace version {version}')
if env_file := os.environ.get('GITHUB_ENV'):
    with open(env_file, 'a', encoding='utf-8') as out:
        out.write(f'PACKAGE_VERSION={version}\n')
print(version)
