#!/usr/bin/env python3
"""Local FFmpeg Vulkan capability investigation; no wallpaper surface or daemon changes.

hwdownload is intentional for pixel comparison, and is not a zero-copy claim.
The separate libplacebo filter check tests upstream interop, not LianWall integration.
"""
import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('--corpus', type=Path, required=True)
parser.add_argument('--output-dir', type=Path, required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
out = args.output_dir.resolve()
out.mkdir(parents=True, exist_ok=True)
summary = {'result': 'incomplete', 'cases': [], 'not_tested': ['LianWall hardware frame import',
           'zero-copy presentation', 'hardware playback lifecycle', 'other GPUs']}

def run(command, name, check=True):
    r = subprocess.run(list(map(str, command)), stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=90)
    (out/(name+'.stdout')).write_bytes(r.stdout)
    (out/(name+'.log')).write_bytes(r.stderr)
    if check and r.returncode:
        raise RuntimeError(f'{name}: exit {r.returncode}\n{r.stderr.decode()[-4000:]}')
    return r

def hashes(data):
    return [line.split(',')[-1].strip() for line in data.decode().splitlines() if line and not line.startswith('#')]

try:
    flags = shlex.split(run(['pkg-config', '--cflags', '--libs', 'libavutil', 'libplacebo'], 'flags').stdout.decode())
    command = [os.environ.get('CC', 'cc'), '-std=c11', '-Wall', '-Wextra']
    if os.environ.get('VULKAN_HEADERS'):
        command += ['-I'+os.environ['VULKAN_HEADERS']]
    binary = out/'video-device'
    run(command+[root/'tests/video-device.c', '-o', binary]+flags, 'compile')
    summary['device'] = json.loads(run([binary], 'device').stdout)
    run(['ffmpeg', '-hide_banner', '-version'], 'versions')
    base = ['ffmpeg', '-hide_banner', '-nostdin', '-nostats', '-v', 'verbose', '-threads', '1', '-filter_threads', '1']
    for name, codec in [('cfr-bframes.mp4', 'h264'), ('hevc.mp4', 'hevc'), ('vp9.webm', 'vp9'), ('av1.mkv', 'av1')]:
        path = args.corpus.resolve()/name
        assert path.is_file(), f'missing corpus fixture: {path}'
        probe = json.loads(run(['ffprobe', '-v', 'error', '-select_streams', 'v:0', '-show_frames', '-of', 'json', path], codec+'-reference').stdout)
        expected = len(probe['frames'])
        tail = ['-map', '0:v:0', '-an', '-fps_mode', 'passthrough', '-c:v', 'rawvideo', '-f', 'framemd5', '-']
        hardware = run(base+['-hwaccel', 'vulkan', '-hwaccel_output_format', 'vulkan', '-c:v', codec, '-i', path,
                            '-vf', 'hwdownload,format=nv12']+tail, codec+'-hardware', check=False)
        if hardware.returncode:
            summary['cases'].append({'codec': codec, 'result': 'hardware failed', 'exit': hardware.returncode})
            continue
        # hwdownload rejects software frames, so a successful path cannot be silent CPU fallback.
        software_codec = 'libdav1d' if codec == 'av1' else codec
        reference = run(base+['-hwaccel', 'none', '-c:v', software_codec, '-i', path, '-vf', 'format=nv12']+tail, codec+'-software')
        actual, wanted = hashes(hardware.stdout), hashes(reference.stdout)
        assert len(actual) == len(wanted) == expected, (codec, len(actual), expected)
        summary['cases'].append({'codec': codec, 'result': 'hardware decoded', 'frames': expected,
                                 'reference_hashes_equal': actual == wanted, 'software_reference': software_codec,
                                 'cpu_download_for_comparison': True})
    shared = run(base+['-init_hw_device', 'vulkan=lw:0', '-filter_hw_device', 'lw', '-hwaccel', 'vulkan',
                      '-hwaccel_device', 'lw', '-hwaccel_output_format', 'vulkan', '-i', args.corpus.resolve()/'cfr-bframes.mp4',
                      '-an', '-vf', 'libplacebo=w=320:h=180', '-f', 'null', '-'], 'shared-filter', check=False)
    summary['upstream_shared_filter'] = {'exit': shared.returncode, 'result': 'completed' if not shared.returncode else 'failed',
        'queue_flag_gate': summary['device']['import_queue_flags_representable']}
    summary['result'] = 'probe completed; inspect individual capabilities, not a backend acceptance pass'
    print(json.dumps(summary, indent=2), flush=True)
finally:
    (out/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
