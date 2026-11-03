#!/usr/bin/env python3
from pathlib import Path


workflow = Path('.github/workflows/build-installers.yml').read_text(encoding='utf-8')

required = [
    'publish-release:',
    "if: startsWith(github.ref, 'refs/tags/v')",
    'contents: write',
    'actions/download-artifact@v8',
    'softprops/action-gh-release@v2',
    'NewPic-0.1.0-x64-setup.exe',
    'NewPic-0.1.0-x64.msi',
    'NewPic-0.1.0-Apple-Silicon.dmg',
    'NewPic-0.1.0-Intel.dmg',
    'NewPic-0.1.0-x86_64.AppImage',
    'NewPic-0.1.0-amd64.deb',
    'SHA256SUMS.txt',
]

missing = [value for value in required if value not in workflow]
if missing:
    raise SystemExit(f'missing release workflow contracts: {missing}')

print('release workflow contract passed')
