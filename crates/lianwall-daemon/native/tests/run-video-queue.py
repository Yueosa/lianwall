#!/usr/bin/env python3
"""CPU-only asynchronous decoder queue tests. Generate corpus with run-video.py first."""
import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--corpus', type=Path, required=True)
parser.add_argument('--output-dir', type=Path)
parser.add_argument('--sanitize', action='store_true')
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
workspace = tempfile.TemporaryDirectory(prefix='lianwall-video-queue-') if not args.output_dir else None
out = args.output_dir.resolve() if args.output_dir else Path(workspace.name)
out.mkdir(parents=True, exist_ok=True)
summary = {'result': 'incomplete', 'mode': 'asynchronous software decoder queue', 'cases': [],
           'not_tested': ['hardware decoding', 'production daemon video integration', 'hard I/O cancellation deadline']}

def run(command):
    r = subprocess.run(list(map(str, command)), stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=90)
    if r.returncode or any(s in r.stderr for s in [b'runtime error:', b'ERROR: AddressSanitizer', b'ERROR: LeakSanitizer']):
        raise RuntimeError(f'{command}\nexit={r.returncode}\n{r.stdout.decode()}\n{r.stderr.decode()}')
    return r

try:
    flags = shlex.split(run(['pkg-config', '--cflags', '--libs', 'libavformat', 'libavcodec', 'libavutil']).stdout.decode())
    binary = out / 'video-queue-check'
    command = [os.environ.get('CC', 'cc'), '-std=c11', '-D_POSIX_C_SOURCE=200809L', '-O1', '-Wall', '-Wextra', '-I' + str(root)]
    if args.sanitize:
        command += ['-fsanitize=address,undefined', '-g', '-fno-omit-frame-pointer']
    run(command + [root/'tests/video-queue.c', root/'video.c', root/'video_queue.c', '-o', binary] + flags + ['-pthread', '-lm'])
    for name in ['cfr-bframes.mp4', 'vfr-bframes.mp4', 'large-offset.mp4', 'cfr.mkv', 'cfr.ts',
                 'audio-video.mp4', 'resolution-change.h264', 'pixel-format-change.h264']:
        path = args.corpus.resolve() / name
        assert path.is_file(), f'missing corpus fixture: {path}'
        loops = 1 if path.suffix == '.h264' else 3
        r = run([binary, 'compare', path, loops])
        (out/(name+'.log')).write_bytes(r.stdout+r.stderr)
        summary['cases'].append({'file': name, **json.loads(r.stdout)})
    r = run([binary, 'lifecycle', args.corpus.resolve()/'cfr-bframes.mp4'])
    (out/'lifecycle.log').write_bytes(r.stdout+r.stderr)
    summary['lifecycle'] = json.loads(r.stdout)
    summary['result'] = 'pass'
    print(json.dumps(summary, indent=2), flush=True)
finally:
    (out/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    if workspace:
        workspace.cleanup()
