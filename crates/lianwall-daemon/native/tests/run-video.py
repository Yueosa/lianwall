#!/usr/bin/env python3
"""CPU-only video correctness checks. Requires cc, pkg-config, FFmpeg and ffprobe.

Fixtures/results are temporary unless --output-dir is supplied. No wallpaper surface,
GPU, daemon, user configuration or external wallpaper process is touched.
"""
import argparse
from fractions import Fraction
import json
import math
import os
from pathlib import Path
import shlex
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--sanitize', action='store_true')
parser.add_argument('--output-dir', type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
workspace = tempfile.TemporaryDirectory(prefix='lianwall-video-check-') if not args.output_dir else None
out = args.output_dir.resolve() if args.output_dir else Path(workspace.name)
out.mkdir(parents=True, exist_ok=True)
summary = {'result': 'incomplete', 'stage': 'software decoder/PTS/loop/cancellation only', 'sanitize': args.sanitize,
           'cases': [], 'skipped': [], 'not_tested': ['GPU mapping', 'hardware decoding',
           'Wayland video playback', 'video transitions', 'seamless presentation at loop boundaries']}

def run(command, *, timeout=45, check=True):
    result = subprocess.run(list(map(str, command)), stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, timeout=timeout)
    if args.sanitize and any(marker in result.stderr for marker in [b'runtime error:', b'ERROR: AddressSanitizer', b'ERROR: LeakSanitizer']):
        raise RuntimeError(f'Sanitizer reported a failure: {command}\n{result.stderr.decode(errors="replace")}')
    if check and result.returncode:
        raise RuntimeError(f'{command}\nexit={result.returncode}\n{result.stderr.decode(errors="replace")}\n{result.stdout.decode(errors="replace")}')
    return result

base = ['ffmpeg', '-hide_banner', '-v', 'error', '-y', '-threads', '1', '-filter_threads', '1']
def encode(name, *, codec='libx264', extension='mp4', frames=36, rate=30, size='320x180', pixel_format='yuv420p', extra=()):
    path = out / (name + '.' + extension)
    options = {'libx264': ['-preset', 'veryfast', '-bf', '3', '-g', '12', '-x264-params', 'b-adapt=0:scenecut=0'],
               'libx265': ['-preset', 'ultrafast', '-x265-params', 'log-level=error:pools=none:frame-threads=1'],
               'libvpx-vp9': ['-deadline', 'realtime', '-cpu-used', '8', '-lag-in-frames', '0'],
               'libaom-av1': ['-usage', 'realtime', '-cpu-used', '8', '-lag-in-frames', '0']}[codec]
    run(base + ['-f', 'lavfi', '-i', f'testsrc2=size={size}:rate={rate}', '-frames:v', str(frames),
                '-an', '-c:v', codec, '-threads', '1', '-pix_fmt', pixel_format] + options + list(extra) +
        (['-movflags', '+faststart'] if extension == 'mp4' else []) + [path])
    return path

try:
    flags = shlex.split(run(['pkg-config', '--cflags', '--libs', 'libavformat', 'libavcodec', 'libavutil']).stdout.decode())
    binary = out / 'video-check'
    command = [os.environ.get('CC', 'cc'), '-std=c11', '-D_POSIX_C_SOURCE=200809L', '-O1',
               '-Wall', '-Wextra', '-I' + str(root)]
    if args.sanitize:
        command += ['-fsanitize=address,undefined', '-g', '-fno-omit-frame-pointer']
    run(command + [root / 'tests/video.c', root / 'video.c', '-o', binary] + flags + ['-pthread', '-lm'])
    cfr = encode('cfr-bframes')
    vfr = encode('vfr-bframes', frames=12, extra=['-vf', r'setpts=if(lt(N\,4)\,N\,if(lt(N\,8)\,4+2*(N-4)\,12+3*(N-8)))', '-fps_mode', 'vfr'])
    offset = out / 'offset.mp4'
    run(base + ['-i', cfr, '-map', '0:v:0', '-c', 'copy', '-output_ts_offset', '5', offset])
    large_offset = out / 'large-offset.mp4'
    run(base + ['-i', cfr, '-map', '0:v:0', '-c', 'copy', '-output_ts_offset', '1000000000', large_offset])
    mkv = out / 'cfr.mkv'
    run(base + ['-i', cfr, '-map', '0:v:0', '-c', 'copy', mkv])
    transport = out / 'cfr.ts'
    run(base + ['-i', cfr, '-map', '0:v:0', '-c', 'copy', transport])
    av = out / 'audio-video.mp4'
    run(base + ['-f', 'lavfi', '-i', 'testsrc2=size=320x180:rate=25', '-f', 'lavfi', '-i', 'sine=frequency=400:sample_rate=48000', '-t', '1', '-map', '0:v:0', '-map', '1:a:0', '-c:v', 'libx264', '-threads', '1', '-bf', '2', '-c:a', 'aac', '-pix_fmt', 'yuv420p', av])
    raw = out / 'missing-pts.h264'
    run(base + ['-i', cfr, '-map', '0:v:0', '-c', 'copy', '-bsf:v', 'h264_mp4toannexb', '-f', 'h264', raw])
    smaller = encode('smaller', frames=8, size='160x90')
    raw_small = out / 'smaller.h264'
    run(base + ['-i', smaller, '-map', '0:v:0', '-c', 'copy', '-bsf:v', 'h264_mp4toannexb', '-f', 'h264', raw_small])
    changing = out / 'resolution-change.h264'
    changing.write_bytes(raw.read_bytes() + raw_small.read_bytes())
    high10 = encode('high10', frames=8, pixel_format='yuv420p10le')
    raw_high10 = out / 'high10.h264'
    run(base + ['-i', high10, '-map', '0:v:0', '-c', 'copy', '-bsf:v', 'h264_mp4toannexb', '-f', 'h264', raw_high10])
    changing_format = out / 'pixel-format-change.h264'
    changing_format.write_bytes(raw.read_bytes() + raw_high10.read_bytes())
    cases = [(cfr, 3), (vfr, 3), (offset, 3), (large_offset, 3), (mkv, 3), (transport, 3), (av, 3), (raw, 1), (changing, 1), (changing_format, 1)]
    available = run(['ffmpeg', '-hide_banner', '-encoders']).stdout.decode()
    for codec, name, ext in [('libx265', 'hevc', 'mp4'), ('libvpx-vp9', 'vp9', 'webm'), ('libaom-av1', 'av1', 'mkv')]:
        if codec not in available:
            summary['skipped'].append({'codec': codec, 'reason': 'encoder unavailable'})
            continue
        cases.append((encode(name, codec=codec, extension=ext, frames=8), 3))
    for path, loops in cases:
        probe = json.loads(run(['ffprobe', '-v', 'error', '-select_streams', 'v:0', '-show_frames', '-show_streams', '-of', 'json', path]).stdout)
        (out / (path.name + '.reference.ffprobe.json')).write_text(json.dumps(probe, indent=2) + '\n')
        frames = probe['frames']; stream = probe['streams'][0]
        if path == cfr:
            assert any(f.get('pict_type') == 'B' for f in frames), 'fixture must contain B frames'
        raw_times = [Fraction(f['best_effort_timestamp']) * Fraction(stream['time_base'])
                     if 'best_effort_timestamp' in f else None for f in frames]
        if path == vfr:
            assert len({raw_times[i + 1] - raw_times[i] for i in range(len(raw_times)-1)}) >= 2, 'fixture must be VFR'
        if path == offset:
            assert float(raw_times[0]) >= 4.9
        if path == large_offset:
            assert float(raw_times[0]) >= 999999999
        reference_hashes = []
        if path not in [changing, changing_format]:
            md5 = run(base + ['-i', path, '-map', '0:v:0', '-an', '-fps_mode', 'passthrough', '-c:v', 'rawvideo', '-f', 'framemd5', '-']).stdout.decode()
            (out / (path.name + '.reference.framemd5')).write_text(md5)
            reference_hashes = [line.split(',')[-1].strip() for line in md5.splitlines() if line and not line.startswith('#')]
            assert len(reference_hashes) == len(frames)
        result = run([binary, 'decode', path, loops], timeout=45)
        (out / (path.name + '.decode.jsonl')).write_bytes(result.stdout)
        if result.stderr:
            (out / (path.name + '.stderr.log')).write_bytes(result.stderr)
        events = [json.loads(line) for line in result.stdout.decode().splitlines()]
        for loop in range(loops):
            actual = [e for e in events if e['event'] == 'frame' and e['loop'] == loop]
            assert len(actual) == len(frames), (path.name, loop, len(actual), len(frames))
            assert actual[0]['pts'] == 0
            assert all(a['pts'] < b['pts'] for a, b in zip(actual, actual[1:])), path.name
            for i, (a, reference) in enumerate(zip(actual, frames)):
                assert a['index'] == i and a['duration'] > 0 and math.isfinite(a['duration'])
                assert (a['width'], a['height'], a['format']) == (reference['width'], reference['height'], reference['pix_fmt']), (path.name, i)
                if raw_times[i] is not None and raw_times[0] is not None:
                    assert abs(a['pts'] - float(raw_times[i] - raw_times[0])) < 1e-8, (path.name, i, a['pts'], raw_times[i])
                if reference_hashes:
                    assert a['md5'] == reference_hashes[i], (path.name, loop, i)
            first_loop = [e for e in events if e['event'] == 'frame' and e['loop'] == 0]
            assert [(e['pts'], e['md5']) for e in actual] == [(e['pts'], e['md5']) for e in first_loop]
        if path in [raw, changing, changing_format]:
            assert all(e['synthetic'] for e in events if e['event'] == 'frame')
        if path == changing:
            assert len({(e['width'], e['height']) for e in events if e['event'] == 'frame'}) == 2
        if path == changing_format:
            assert len({e['format'] for e in events if e['event'] == 'frame'}) == 2
        case = {'file': path.name, 'frames': len(frames), 'loops': loops,
                'hash_reference': bool(reference_hashes), 'result': 'pass'}
        summary['cases'].append(case)
        print(json.dumps(case), flush=True)
    invalid = [out / 'missing.mp4', out / 'empty.mp4', out / 'corrupt.mp4', out / 'truncated.mp4', out / 'audio-only.m4a', out / 'pipe.mp4', out]
    (out / 'empty.mp4').write_bytes(b'')
    (out / 'corrupt.mp4').write_bytes(b'not a video')
    (out / 'truncated.mp4').write_bytes(cfr.read_bytes()[:len(cfr.read_bytes()) // 2])
    run(base + ['-f', 'lavfi', '-i', 'sine=duration=0.1', '-c:a', 'aac', out / 'audio-only.m4a'])
    if not (out / 'pipe.mp4').exists(): os.mkfifo(out / 'pipe.mp4')
    for path in invalid:
        result = run([binary, 'reject', path])
        (out / (path.name + '.reject.jsonl')).write_bytes(result.stdout)
        if result.stderr: (out / (path.name + '.reject.stderr.log')).write_bytes(result.stderr)
    print(f'rejection cases: {len(invalid)} PASS', flush=True)
    summary['rejected'] = len(invalid)
    result = run([binary, 'check', cfr], timeout=60)
    (out / 'lifecycle.jsonl').write_bytes(result.stdout)
    if result.stderr: (out / 'lifecycle.stderr.log').write_bytes(result.stderr)
    summary['lifecycle'] = [json.loads(line) for line in result.stdout.decode().splitlines()]
    for item in summary['lifecycle']: print(json.dumps(item), flush=True)
    summary['result'] = 'pass'
finally:
    (out / 'summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2) + '\n')
    if not workspace: print('results:', out, flush=True)
    if workspace: workspace.cleanup()
