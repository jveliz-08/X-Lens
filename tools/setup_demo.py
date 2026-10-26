#!/usr/bin/env python3
"""Generate LOCAL secrets using OS randomness. Never writes into source files."""
import os
import pathlib
import secrets
import sys
root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else 'runtime')
root.mkdir(mode=0o700, parents=True, exist_ok=True)
path = root / 'config.secret'
fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
with os.fdopen(fd, 'w') as f:
    f.write(f'viewer={secrets.token_hex(32)}\noperator={secrets.token_hex(32)}\ndevice.demo={secrets.token_hex(32)}\ndevice.webcam={secrets.token_hex(32)}\n')
print(f'Created {path}; read viewer token locally. Existing files are never replaced.')
