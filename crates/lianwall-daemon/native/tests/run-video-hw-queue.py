#!/usr/bin/env python3
"""Shared-device asynchronous hardware queue, mapping, cancellation and fallback checks."""
import argparse,json,os,re,shlex,subprocess
from pathlib import Path
parser=argparse.ArgumentParser()
parser.add_argument('--corpus',type=Path,required=True)
parser.add_argument('--source-4k',type=Path)
parser.add_argument('--output-dir',type=Path,required=True)
parser.add_argument('--validation',action='store_true')
parser.add_argument('--sanitize',action='store_true')
args=parser.parse_args();root=Path(__file__).resolve().parent.parent
out=args.output_dir.resolve();out.mkdir(parents=True,exist_ok=True)
env=os.environ.copy()
if args.validation:env['LIANWALL_TEST_VALIDATION']='1'
summary={'result':'incomplete','cases':[],'not_tested':['daemon hardware playback','total VRAM hard quota','physical presentation']}
def run(command,name):
 r=subprocess.run(list(map(str,command)),env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=120)
 (out/(name+'.stdout')).write_bytes(r.stdout);(out/(name+'.log')).write_bytes(r.stderr)
 if r.returncode or any(s in r.stderr for s in [b'VUID-',b'Validation Error',b'runtime error:',b'ERROR: AddressSanitizer',b'ERROR: LeakSanitizer']):raise RuntimeError(f'{name}: exit={r.returncode}\n{r.stdout.decode()}\n{r.stderr.decode()[-5000:]}')
 return r
try:
 flags=shlex.split(run(['pkg-config','--cflags','--libs','libplacebo','vulkan','libavcodec','libavformat','libavutil'],'flags').stdout.decode())
 command=[os.environ.get('CC','cc'),'-std=c11','-D_POSIX_C_SOURCE=200809L','-O1','-Wall','-Wextra','-I'+str(root)]
 if os.environ.get('VULKAN_HEADERS'):command+=['-I'+os.environ['VULKAN_HEADERS']]
 if args.sanitize:command+=['-g','-fsanitize=address,undefined','-fno-omit-frame-pointer']
 binary=out/'hw-queue'
 run(command+[root/'tests/video-hw-queue.c',*[root/name for name in ['video.c','video_hw.c','video_gpu.c','video_queue.c']],'-o',binary]+flags+['-pthread','-lm'],'compile')
 cases=[(name,args.corpus.resolve()/name,1,0) for name in ['cfr-bframes.mp4','hevc.mp4','vp9.webm','av1.mkv']]
 if args.source_4k:cases.append(('4k',args.source_4k.resolve(),1,0))
 cases += [('high10-strict',args.corpus.resolve()/'high10.mp4',3,0),('high10-auto',args.corpus.resolve()/'high10.mp4',2,0),('budget',args.corpus.resolve()/'cfr-bframes.mp4',1,1)]
 for name,source,mode,budget in cases:
  r=run([binary,source,mode,budget],name);text=r.stdout.decode();assert 'errors=0' in text,text
  summary['cases'].append({'case':name,'result':'pass','raw':text.strip()});print(text.strip(),flush=True)
 summary['result']='pass'
finally:(out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
