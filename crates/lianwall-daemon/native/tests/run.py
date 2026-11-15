#!/usr/bin/env python3
"""Build/run the native decoder and GPU crossfade checks without installing artifacts."""
import argparse
import os
from pathlib import Path
import shlex
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--decode-only', action='store_true', help='skip GPU initialization')
parser.add_argument('--sanitize', action='store_true', help='enable ASan/LSan; do not suppress driver leaks')
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
flags = shlex.split(subprocess.check_output([
    'pkg-config', '--cflags', '--libs', 'libplacebo', 'vulkan', 'libavformat',
    'libavcodec', 'libavutil', 'libswscale', 'lcms2',
], text=True))
with tempfile.TemporaryDirectory(prefix='lianwall-image-check-') as directory:
    binary = str(Path(directory) / 'check')
    command = [os.environ.get('CC', 'cc'), '-O2', '-Wall', '-Wextra', '-Wno-unused-function', '-I' + str(root)]
    if os.environ.get('VULKAN_HEADERS'):
        command += ['-I' + os.environ['VULKAN_HEADERS']]
    if args.sanitize:
        command += ['-fsanitize=address', '-g']
    subprocess.run(command + [str(root / 'tests/check.c'), '-o', binary] + flags + ['-lm'], check=True)
    result = subprocess.run([binary, str(root / 'tests/fixtures')] + (['--decode-only'] if args.decode_only else []))
    raise SystemExit(result.returncode)
