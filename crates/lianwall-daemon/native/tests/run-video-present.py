#!/usr/bin/env python3
"""Explicit Wayland timing prototype. Use an isolated compositor/test output."""
import argparse,json,math,os,re,shlex,signal,statistics,subprocess,tempfile,time
from pathlib import Path
parser=argparse.ArgumentParser()
parser.add_argument('--source',type=Path,required=True)
parser.add_argument('--output',required=True,help='an output name in the isolated Wayland session')
parser.add_argument('--loops',type=int,default=3)
parser.add_argument('--output-dir',type=Path)
parser.add_argument('--cancel-after',type=float)
parser.add_argument('--allow-dropped',type=int,default=0,help='maximum late frames allowed by this timing check')
args=parser.parse_args()
if args.allow_dropped<0:parser.error('--allow-dropped must be nonnegative')
root=Path(__file__).resolve().parent.parent
workspace=tempfile.TemporaryDirectory(prefix='lianwall-video-present-') if not args.output_dir else None
out=args.output_dir.resolve() if args.output_dir else Path(workspace.name);out.mkdir(parents=True,exist_ok=True)
summary={'result':'incomplete','mode':'bounded asynchronous software decode/Wayland timing prototype','not_tested':['hardware decode','production daemon video integration','cross-media transitions','physical scanout']}
def run(command):
 r=subprocess.run(list(map(str,command)),stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=60)
 if r.returncode:raise RuntimeError(f'{command}\n{r.stderr.decode()}\n{r.stdout.decode()}')
 return r
try:
 protocols=run(['pkg-config','--variable=pkgdatadir','wayland-protocols']).stdout.decode().strip()
 sources=[]
 for name,path in [('layer',root/'wlr-layer-shell.xml'),('xdg',Path(protocols)/'stable/xdg-shell/xdg-shell.xml'),('viewporter',Path(protocols)/'stable/viewporter/viewporter.xml'),('fractional',Path(protocols)/'staging/fractional-scale/fractional-scale-v1.xml'),('presentation',Path(protocols)/'stable/presentation-time/presentation-time.xml')]:
  run(['wayland-scanner','client-header',path,out/(name+'-client.h')]);run(['wayland-scanner','private-code',path,out/(name+'-protocol.c')]);sources.append(out/(name+'-protocol.c'))
 flags=shlex.split(run(['pkg-config','--cflags','--libs','libplacebo','vulkan','wayland-client','libavcodec','libavformat','libavutil','libswscale','lcms2']).stdout.decode())
 binary=out/'video-present';command=[os.environ.get('CC','cc'),'-std=c11','-D_POSIX_C_SOURCE=200809L','-O1','-Wall','-Wextra','-I'+str(root),'-I'+str(out)]
 if os.environ.get('VULKAN_HEADERS'):command+=['-I'+os.environ['VULKAN_HEADERS']]
 run(command+[root/'tests/video-present.c',root/'video.c',root/'video_gpu.c',root/'video_queue.c',*sources,'-o',binary]+flags+['-lm','-pthread'])
 with (out/'presentation.log').open('wb') as log:
  child=subprocess.Popen([str(binary),str(args.source.resolve()),args.output,str(args.loops)],stdout=log,stderr=subprocess.STDOUT)
  try:
   if args.cancel_after is not None:
    deadline=time.monotonic()+10
    while b'SUBMIT ' not in (out/'presentation.log').read_bytes() and child.poll() is None and time.monotonic()<deadline:time.sleep(.02)
    assert child.poll() is None,'prototype failed before cancellation'
    time.sleep(args.cancel_after);cancel_started=time.monotonic();child.send_signal(signal.SIGTERM)
   result=child.wait(timeout=25)
  finally:
   if child.poll() is None:child.kill();child.wait()
 text=(out/'presentation.log').read_text()
 assert result==0 and 'CLEANUP result=0' in text,text
 assert 'GPU:' not in text and 'Validation Error' not in text and 'VUID-' not in text,text
 queue_line=next(line for line in text.splitlines() if line.startswith('QUEUE '))
 queue_stats={key:int(value) for key,value in re.findall(r'(\w+)=(\d+)',queue_line)}
 assert queue_stats['peak_frames']<=4 and queue_stats['peak_bytes']<=64*1024*1024,queue_stats
 summary['queue']=queue_stats
 if args.cancel_after is not None:
  assert 'CANCELLED submitted=' in text,text
  summary.update(result='pass',cancelled=True,cancel_exit_ms=(time.monotonic()-cancel_started)*1000)
 else:
  result_line=next(line for line in text.splitlines() if line.startswith('RESULT '))
  counts={key:int(value) for key,value in re.findall(r'(\w+)=(\d+)',result_line)}
  assert counts['dropped']<=args.allow_dropped and counts['discarded']==0 and counts['errors']==0,counts
  assert counts['submitted']==counts['presented'],counts
  events=[json.loads(line) for line in text.splitlines() if line.startswith('{')]
  assert events and all(e['presented']>0 and not e['discarded'] for e in events),text
  assert all(a['pts']<b['pts'] for a,b in zip(events,events[1:])),text
  assert {e['loop'] for e in events}==set(range(args.loops)),text
  epoch=events[0]['presented'];errors=[e['presented']-epoch-e['pts'] for e in events]
  refresh=events[0]['refresh_ns']/1e9 if events[0]['refresh_ns'] else 0
  boundaries=[]
  for a,b in zip(events,events[1:]):
   if b['loop']!=a['loop']:
    assert b['source']==0,(a,b)
    boundaries.append({'from_loop':a['loop'],'to_loop':b['loop'],'expected_gap_ms':(b['pts']-a['pts'])*1000,'actual_gap_ms':(b['presented']-a['presented'])*1000})
  assert len(boundaries)==args.loops-1,text
  assert max(abs(e) for e in errors)<max(.075,refresh*4.5),(max(errors),min(errors),text)
  assert all(abs(b['actual_gap_ms']-b['expected_gap_ms'])<max(75,refresh*4500) for b in boundaries),boundaries
  summary.update(result='pass',presented_frames=len(events),presentation_clock_id=int(next(line.split()[1] for line in text.splitlines() if line.startswith('CLOCK_ID '))),max_abs_error_ms=max(abs(e) for e in errors)*1000,median_error_ms=statistics.median(errors)*1000,refresh_ms=refresh*1000 if refresh else None,loop_boundaries=boundaries,raw_result=next(line for line in text.splitlines() if line.startswith('RESULT ')))
 print(json.dumps(summary,ensure_ascii=False,indent=2),flush=True)
finally:
 (out/'summary.json').write_text(json.dumps(summary,ensure_ascii=False,indent=2)+'\n')
 if not workspace:print('results:',out,flush=True)
 if workspace:workspace.cleanup()
