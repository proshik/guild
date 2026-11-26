#!/usr/bin/env python3
"""Single sequential owned Root trial. No builds, other-PID capture or settings changes."""
import argparse,hashlib,json,os,pathlib,subprocess,time,ctypes
p=argparse.ArgumentParser();p.add_argument('binary',type=pathlib.Path);p.add_argument('output',type=pathlib.Path);p.add_argument('--display',default='1');p.add_argument('--mode',default='stripe');p.add_argument('--policy',default='adaptive');p.add_argument('--count',default='1');p.add_argument('--seconds',type=int,default=12);p.add_argument('--no-capture',action='store_true');p.add_argument('--standalone',action='store_true');p.add_argument('--scenario',choices=['','scroll','selection','migration','settings'],default='');a=p.parse_args()
root=pathlib.Path(__file__).resolve().parents[2];out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
state=out/'state';env=os.environ.copy();env.update(GUILD_STATE_DIR=str(state),GIT_CEILING_DIRECTORIES=str(state),GUILD_NO_GLOBAL_HOOKS='1',GUILD_PROBE_MODE=a.mode,GUILD_PROBE_SECONDS=str(a.seconds),GUILD_PROBE_TABS=a.count,GUILD_PROBE_DISPLAY=a.display,GUILD_PROBE_SCENARIO=a.scenario,GUILD_PROBE_REFRESH=a.policy,GUILD_PROBE_EMISSIONS=str(out/'emissions.jsonl'))
if not a.standalone:env['GUILD_PROBE_ROOT']='1'
else:env.pop('GUILD_PROBE_ROOT',None);env['GUILD_PROBE_AA']='1'
if a.policy=='adaptive':env['GUILD_TERMINAL_REFRESH']='adaptive'
else:env.pop('GUILD_TERMINAL_REFRESH',None)
binary=a.binary.resolve();meta={'binary':str(binary),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'parameters':vars(a)|{'binary':str(binary),'output':str(out)},'endpoint':'software WindowServer displayTime, not photon calibrated','capture_sha256':hashlib.sha256((root/'target/adaptive-refresh/stripe-capture').read_bytes()).hexdigest(),'acceptance':'pilot only','env':{k:v for k,v in env.items() if k.startswith('GUILD_PROBE_') or k in ['GUILD_TERMINAL_REFRESH','GIT_CEILING_DIRECTORIES','GUILD_STATE_DIR','GUILD_NO_GLOBAL_HOOKS']}}
class Timebase(ctypes.Structure):
 _fields_=[('numer',ctypes.c_uint32),('denom',ctypes.c_uint32)]
tb=Timebase();ctypes.CDLL('/usr/lib/libSystem.B.dylib').mach_timebase_info(ctypes.byref(tb))
meta.update(mach_numer=tb.numer,mach_denom=tb.denom,os_build=subprocess.check_output(['sw_vers','-buildVersion'],text=True).strip(),visibility='observed only; continuous visibility UNVERIFIED',settings_mutated=False)
manifest=binary.parent.parent/'sources.json'
if manifest.exists():
 frozen=json.loads(manifest.read_text());name=binary.name
 if name in frozen:meta['source_digest']=frozen[name]['source_digest'];meta['diagnostics_patch_sha256']=frozen['diagnostics_patch_sha256']
meta['tools_sha256']={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in pathlib.Path(__file__).parent.iterdir() if p.is_file()}
children=[]
try:
 with (out/'native.jsonl').open('w') as stdout,(out/'native.stderr').open('w') as stderr:
  native=subprocess.Popen([str(binary)],cwd='/',env=env,stdout=stdout,stderr=stderr);children.append(native);meta['owned_pid']=native.pid
  with (out/'usage.jsonl').open('w') as usage:
   sampler=subprocess.Popen([str(root/'target/adaptive-refresh/owned-usage'),str(native.pid),str((a.seconds-1)*4)],stdout=usage);children.append(sampler)
   time.sleep(1)
   if not a.no_capture:
    with (out/'capture.jsonl').open('w') as co,(out/'capture.stderr').open('w') as ce:
     args=[str(root/'target/adaptive-refresh/stripe-capture'),str(native.pid),str(a.seconds-3)]
     if not a.mode.startswith('stripe'):args.append('counts')
     elif a.mode=='stripe-ack':args.append('ack')
     capture=subprocess.Popen(args,cwd='/',stdout=co,stderr=ce);children.append(capture);meta['capture_pid']=capture.pid
     with (out/'capture-usage.jsonl').open('w') as cu:
      capture_sampler=subprocess.Popen([str(root/'target/adaptive-refresh/owned-usage'),str(capture.pid),str(max(1,(a.seconds-4)*4))],stdout=cu);children.append(capture_sampler)
      meta['capture_exit']=capture.wait(timeout=a.seconds+10)
      meta['capture_usage_exit']=capture_sampler.wait(timeout=a.seconds+10)
   meta['usage_exit']=sampler.wait(timeout=a.seconds+10)
  meta['native_exit']=native.wait(timeout=a.seconds+10)
finally:
 for child in children:
  if child.poll() is None:child.terminate();child.wait(timeout=5)
 (out/'manifest.json').write_text(json.dumps(meta,indent=2)+'\n')
print(json.dumps({k:meta[k] for k in ['owned_pid','native_exit','usage_exit'] if k in meta}))
