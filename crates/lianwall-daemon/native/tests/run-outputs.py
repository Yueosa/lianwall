#!/usr/bin/env python3
"""No GPU initialization. Negative cases use private sockets; live checks opt in."""
import argparse, json, os, shlex, socket, subprocess, tempfile, time
from pathlib import Path
parser = argparse.ArgumentParser(__doc__)
parser.add_argument('--output-dir', type=Path, required=True)
parser.add_argument('--live-output', help='explicit output in a private compositor')
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
out = args.output_dir.resolve(); out.mkdir(parents=True, exist_ok=True)
flags = shlex.split(subprocess.check_output(['pkg-config', '--cflags', '--libs', 'wayland-client']).decode())
subprocess.run([os.environ.get('CC', 'cc'), '-std=c11', '-D_POSIX_C_SOURCE=200809L', '-O1', '-g', '-I'+str(root), str(root/'tests/outputs.c'), str(root/'outputs.c'), '-o', str(out/'outputs'), *flags], check=True)
records = []
with tempfile.TemporaryDirectory(prefix='lw-outputs-') as temporary:
    env = os.environ.copy(); env.pop('WAYLAND_SOCKET', None)
    env.update(XDG_RUNTIME_DIR=temporary, WAYLAND_DISPLAY=str(Path(temporary)/'missing'))
    def run(name, env, loops='1', expected=None):
        command = [str(out/'outputs'), '150', loops]
        if expected: command.append(expected)
        start = time.monotonic()
        result = subprocess.run(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=8)
        text = result.stdout.decode(); (out/(name+'.log')).write_text(text)
        assert result.returncode == (0 if expected else 1), text
        assert ('OUTPUT_DISCOVERY_PASS' if expected else 'UNAVAILABLE') in text, text
        elapsed = time.monotonic()-start
        if not expected: assert elapsed < 1, (name, elapsed)
        records.append({'case': name, 'seconds': elapsed, 'exit': result.returncode})
    run('missing', env)
    inherited = env.copy(); inherited['WAYLAND_SOCKET'] = '999999'
    run('inherited_socket_rejected', inherited)
    with socket.socket(socket.AF_UNIX) as server:
        path = str(Path(temporary)/'stalled'); server.bind(path); server.listen(1)
        stalled = env.copy(); stalled['WAYLAND_DISPLAY'] = path
        run('stalled', stalled)
    if args.live_output:
        live = os.environ.copy(); live.pop('WAYLAND_SOCKET', None)
        run('live', live, '32', args.live_output)
(out/'summary.json').write_text(json.dumps(records, indent=2)+'\n')
print(json.dumps(records), flush=True)
