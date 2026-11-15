#!/usr/bin/env python3
"""Offscreen libplacebo-owned Vulkan device -> strict FFmpeg hardware decoder -> mapping.

No Wayland/daemon/config changes. The rendered RGBA output is read back for validation;
no decoder-frame hwdownload is requested. This is not a physical presentation test.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--corpus', type=Path, required=True)
parser.add_argument('--output-dir', type=Path)
parser.add_argument('--validation', action='store_true')
parser.add_argument('--sanitize', action='store_true')
parser.add_argument('--source-4k', type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
workspace = tempfile.TemporaryDirectory(prefix='lianwall-video-interop-') if not args.output_dir else None
out = args.output_dir.resolve() if args.output_dir else Path(workspace.name)
out.mkdir(parents=True, exist_ok=True)
summary = {'result': 'incomplete', 'cases': [], 'not_tested': ['daemon hardware playback',
           'asynchronous hardware queue', 'physical presentation', 'exhaustive driver copy tracing']}
env = os.environ.copy()
if args.validation:
    env['LIANWALL_TEST_VALIDATION'] = '1'

def run(command, name, allowed=(0,)):
    r = subprocess.run(list(map(str, command)), env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=90)
    (out/(name+'.stdout')).write_bytes(r.stdout)
    (out/(name+'.log')).write_bytes(r.stderr)
    if r.returncode not in allowed or any(s in r.stderr for s in [b'Validation Error', b'VUID-', b'runtime error:', b'ERROR: AddressSanitizer', b'ERROR: LeakSanitizer']):
        raise RuntimeError(f'{name}: exit {r.returncode}\n{r.stdout.decode()}\n{r.stderr.decode()[-5000:]}')
    return r

try:
    flags = shlex.split(run(['pkg-config', '--cflags', '--libs', 'libplacebo', 'vulkan', 'libavcodec', 'libavformat', 'libavutil'], 'flags').stdout.decode())
    binary = out/'interop'
    command = [os.environ.get('CC', 'cc'), '-std=c11', '-D_POSIX_C_SOURCE=200809L', '-O1', '-Wall', '-Wextra', '-I'+str(root)]
    if os.environ.get('VULKAN_HEADERS'):
        command += ['-I'+os.environ['VULKAN_HEADERS']]
    if args.sanitize:
        command += ['-g', '-fsanitize=address,undefined', '-fno-omit-frame-pointer']
    run(command+[root/'tests/video-interop.c', root/'video.c',root/'video_hw.c', '-o', binary]+flags+['-pthread', '-lm'], 'compile')
    main10=out/'hevc-main10.mp4'
    run(['ffmpeg','-hide_banner','-nostdin','-v','error','-y','-f','lavfi','-i','testsrc2=size=320x180:rate=30',
         '-frames:v','8','-an','-c:v','libx265','-threads','1','-filter_threads','1','-preset','ultrafast',
         '-x265-params','log-level=error:pools=none:frame-threads=1','-pix_fmt','yuv420p10le',main10], 'encode-main10')
    cases = [args.corpus.resolve()/name for name in ['cfr-bframes.mp4', 'hevc.mp4', 'vp9.webm', 'av1.mkv']]+[main10]
    if args.source_4k:
        cases.append(args.source_4k.resolve())
    for index, source in enumerate(cases):
        assert source.is_file(), f'missing corpus fixture: {source}'
        probe = json.loads(run(['ffprobe', '-v', 'error', '-select_streams', 'v:0', '-show_frames', '-of', 'json', source], str(index)+'-reference').stdout)
        result = run([binary, source], str(index)+'-interop')
        match = re.search(r'INTEROP frames=(\d+) loops=3 decoder_downloads=0 rendered_readbacks=(\d+) max_channel_delta=(\d+) errors=0', result.stdout.decode())
        assert match and int(match[1]) == len(probe['frames'])*3 and int(match[2]) == int(match[1])*2 and int(match[3])<=4, result.stdout.decode()
        case = {'source': source.name, 'hardware_frames': int(match[1]), 'loops': 3,
                'same_vkdevice': True, 'result': 'pass', 'rendered_output_readback': True}
        case['max_channel_delta']=int(match[3])
        summary['cases'].append(case)
        print(json.dumps(case), flush=True)
    negative=run([binary,args.corpus.resolve()/'high10.mp4'],'h264-high10',allowed=(0,77))
    if negative.returncode==77:
        assert b'INTEROP_UNSUPPORTED frames=0 errors=0' in negative.stdout,negative.stdout.decode()
    summary['profile_probe']={'source':'high10.mp4','result':'unsupported, no CPU fallback' if negative.returncode==77 else 'hardware decoded', 'exit':negative.returncode}
    summary['result'] = 'pass'
finally:
    (out/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    if workspace:
        workspace.cleanup()
