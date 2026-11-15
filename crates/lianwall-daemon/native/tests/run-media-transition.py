#!/usr/bin/env python3
"""Run only in an isolated layer-shell compositor; never select a user's output."""
import argparse, json, os, re, shlex, subprocess
from pathlib import Path
parser = argparse.ArgumentParser(__doc__)
parser.add_argument('--video', type=Path, required=True, help='one-second moving SDR loop fixture')
parser.add_argument('--output', required=True)
parser.add_argument('--output-dir', type=Path, required=True)
parser.add_argument('--sanitize', action='store_true')
parser.add_argument('--lsan-baseline', type=Path, help='raw device-only LSan log; allocations remain reported')
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
out = args.output_dir.resolve(); out.mkdir(parents=True, exist_ok=True)
def run(command):
    return subprocess.check_output(list(map(str, command)), stderr=subprocess.STDOUT).decode()
protocols = Path(run(['pkg-config', '--variable=pkgdatadir', 'wayland-protocols']).strip())
sources = []
for name, xml in [('layer', root/'wlr-layer-shell.xml'), ('xdg', protocols/'stable/xdg-shell/xdg-shell.xml'), ('viewporter', protocols/'stable/viewporter/viewporter.xml'), ('fractional', protocols/'staging/fractional-scale/fractional-scale-v1.xml'), ('presentation', protocols/'stable/presentation-time/presentation-time.xml')]:
    run(['wayland-scanner', 'client-header', xml, out/(name+'-client.h')])
    run(['wayland-scanner', 'private-code', xml, out/(name+'-protocol.c')])
    sources.append(out/(name+'-protocol.c'))
flags = shlex.split(run(['pkg-config', '--cflags', '--libs', 'libplacebo', 'vulkan', 'wayland-client', 'libavcodec', 'libavformat', 'libavutil', 'libswscale', 'lcms2']))
command = [os.environ.get('CC', 'cc'), '-std=c11', '-D_POSIX_C_SOURCE=200809L', '-O1', '-g', '-I'+str(root), '-I'+str(out)]
if os.environ.get('VULKAN_HEADERS'): command += ['-I'+os.environ['VULKAN_HEADERS']]
if args.sanitize: command += ['-fsanitize=address,undefined', '-fno-omit-frame-pointer']
command += [root/'tests/media-transition.c', *[root/f for f in ['video.c', 'video_hw.c', 'video_gpu.c', 'video_queue.c']], *sources, '-o', out/'media-transition', *flags, '-pthread', '-lm']
(out/'compile.log').write_text(run(command))
records = []
for decode in [0, 1]:
    result = subprocess.run([str(out/'media-transition'), str(root/'tests/fixtures/red.png'), str(root/'tests/fixtures/blue.png'), str(args.video.resolve()), args.output, str(decode)], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=90)
    text = result.stdout.decode(); (out/f'decode-{decode}.log').write_text(text)
    assert 'MEDIA_TRANSITION_PASS' in text and 'VUID-' not in text and 'Validation Error' not in text, text
    assert 'runtime error:' not in text and 'ERROR: AddressSanitizer:' not in text, text
    if args.sanitize:
        # Preserve the raw LSan result for a separate device-baseline comparison.
        assert result.returncode in [0, 1], text
        if result.returncode:
            assert args.lsan_baseline, 'LSan is not clean; supply a device-only --lsan-baseline to compare raw reports.\n'+text
            def allocations(report):
                blocks = re.split(r'(?:Direct|Indirect) leak of ', report)[1:]
                assert len(blocks) == 3 and all('libdbus-1.so' in block for block in blocks), report
                sizes = [int(re.match(r'(\d+) byte', block)[1]) for block in blocks]
                assert sizes[0] == 184 and sizes[2] == 88, report
                return sizes
            actual = allocations(text)
            baseline = allocations(args.lsan_baseline.read_text())
            assert abs(sum(actual)-sum(baseline)) <= 64, (actual, baseline)
    else:
        assert result.returncode == 0, text
    records.append({'decode': decode, 'exit': result.returncode, 'functional_pass': True, 'clean_lsan': result.returncode == 0 if args.sanitize else None})
(out/'summary.json').write_text(json.dumps({'cases': records, 'sanitized': args.sanitize}, indent=2)+'\n')
print(json.dumps(records), flush=True)
