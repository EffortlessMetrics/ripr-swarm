import os,pathlib,hashlib,json,signal,subprocess,time,threading,resource,ctypes
from observer import NativeObserver,ObserverDenied
P=pathlib.Path('/workspace/scratch/profile-8258-native');ROOT=pathlib.Path('/workspace/profile-8258-merge16a');B=pathlib.Path('/workspace/scratch/probe-shape-reuse/candidate-ripr');run=P/'diagnostic';run.mkdir(exist_ok=False)
def digest(f):
 h=hashlib.sha256()
 with f.open('rb') as s:
  for b in iter(lambda:s.read(1024*1024),b''):h.update(b)
 return h.hexdigest()
assert digest(B)=='65b1773d458cdef819f1163c90728ed4e1e46e7851a71565c07a51327ed04e0f';assert digest(P/'verified.diff')=='e437f1a3015580d482ec9debeee7b7a35a27e398e6b01f564128a2600a97b687'
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT).decode().strip()=='16a535defe78b3414f87b34043f565667e289ea7';assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT)
assert ctypes.CDLL(None).prctl(36,1,0,0,0)==0
hz=os.sysconf('SC_CLK_TCK');page=os.sysconf('SC_PAGE_SIZE')
def stat(path):
 raw=path.read_text();v=raw[raw.rfind(')')+2:].split();return {'state':v[0],'ppid':int(v[1]),'pgrp':int(v[2]),'cpu_s':(int(v[11])+int(v[12]))/hz,'rss_bytes':int(v[21])*page}
def processes():
 result={}
 for d in pathlib.Path('/proc').iterdir():
  if d.name.isdigit():
   try:result[int(d.name)]=stat(d/'stat')
   except (OSError,ValueError,IndexError):pass
 return result
settings={'PATH':'/workspace/.cargo/bin:'+os.environ['PATH'],'CARGO_HOME':'/workspace/.cargo','RUSTUP_HOME':'/workspace/.rustup','RUSTUP_TOOLCHAIN':'1.95.0','RAYON_NUM_THREADS':'4','RIPR_REPO_EXPOSURE_LATENCY_TRACE':'1','RIPR_CACHE_DIR':str(run/'cache'),'RIPR_MAX_DIFF_CHANGED_RUST_LINES':'10000','RIPR_PARTIAL_DIFF_LINE_BUDGET':'10000'}
env=os.environ.copy();env.update(settings);env.pop('RIPR_INDEX_PROFILE',None)
argv=[str(B),'check','--root',str(ROOT),'--base','14afcde1c5e7777ecb5697bdbfb80394dd9e5fb2','--diff',str(P/'verified.diff'),'--format','json']
observer=NativeObserver();known=set();prior_cpu={};status=None;usage=None;fatal=None;deadline_fired=False;kill_at=None;phase_offset=0;snapshot_count=0;stack_count=0;peak=0;paused={};max_cpu={}
parent0=resource.getrusage(resource.RUSAGE_SELF);started=time.monotonic()
with (run/'stdout.json').open('wb') as out,(run/'stderr.log').open('wb') as err,(run/'samples.jsonl').open('w') as samples,(run/'stacks.jsonl').open('w') as stacks,(run/'phase-timeline.jsonl').open('w') as phases:
 child=subprocess.Popen(argv,cwd=ROOT,env=env,stdout=out,stderr=err,start_new_session=True);known.add(child.pid)
 def expire():
  global deadline_fired,kill_at
  deadline_fired=True;kill_at=time.monotonic()-started
  try:os.killpg(child.pid,signal.SIGKILL)
  except ProcessLookupError:pass
 timer=threading.Timer(max(0,90-(time.monotonic()-started)),expire);timer.daemon=True;timer.start();next_sample=started+.5
 try:
  while True:
   if observer.root_exited is not None:status,usage=observer.root_exited;break
   got,st,ru=os.wait4(child.pid,os.WNOHANG)
   if got:
    if os.WIFEXITED(st) or os.WIFSIGNALED(st):status=st;usage=ru;break
    raise RuntimeError('unexpected ptrace stop outside observer')
   now=time.monotonic()
   if now>=next_sample and not deadline_fired:
    elapsed=now-started;table=processes();changed=True
    while changed:
     new={pid for pid,v in table.items() if v['ppid'] in known or v['pgrp']==child.pid}-known;changed=bool(new);known|=new
    live={pid:v for pid,v in table.items() if pid in known};rss=sum(v['rss_bytes'] for v in live.values());peak=max(peak,rss)
    for pid,v in live.items():max_cpu[pid]=max(max_cpu.get(pid,0),v['cpu_s'])
    tids={}
    try:
     for d in pathlib.Path(f'/proc/{child.pid}/task').iterdir():
      try:tids[int(d.name)]=stat(d/'stat')
      except OSError:pass
     maps=pathlib.Path(f'/proc/{child.pid}/maps').read_text();io={k:int(v) for k,v in (line.split(':',1) for line in pathlib.Path(f'/proc/{child.pid}/io').read_text().splitlines())}
    except OSError:maps='';io={}
    active=[]
    for tid,v in tids.items():
     delta=v['cpu_s']-prior_cpu.get(tid,v['cpu_s']);v['cpu_delta_s']=max(0,delta)
     if delta>=.01:active.append((delta,tid))
    prior_cpu={tid:v['cpu_s'] for tid,v in tids.items()}
    samples.write(json.dumps({'elapsed_s':elapsed,'processes':live,'threads':tids,'io':io,'maps':maps,'rss_sum_bytes':rss})+'\n');samples.flush();snapshot_count+=1
    with (run/'stderr.log').open('rb') as f:f.seek(phase_offset);lines=f.read();phase_offset=f.tell()
    if lines:phases.write(json.dumps({'observed_elapsed_s':elapsed,'new_stderr':lines.decode(errors='replace')})+'\n');phases.flush()
    for delta,tid in sorted(active,reverse=True)[:8]:
     if deadline_fired:break
     begin=time.monotonic();sample=observer.sample(tid,child.pid,max_frames=32);sample.update(elapsed_s=time.monotonic()-started,observed_cpu_delta_s=delta,observer_call_wall_s=time.monotonic()-begin,snapshot_index=snapshot_count-1)
     stacks.write(json.dumps(sample)+'\n');stacks.flush();stack_count+=1
    next_sample=time.monotonic()+1
   time.sleep(.02)
 except BaseException as e:
  fatal=type(e).__name__+': '+str(e)
 finally:
  try:os.killpg(child.pid,signal.SIGKILL)
  except ProcessLookupError:pass
  if status is None:
   if observer.root_exited is not None:status,usage=observer.root_exited
   else:
    try:_,status,usage=os.wait4(child.pid,0)
    except ChildProcessError:status=None
  root_elapsed=time.monotonic()-started;timer.cancel();timer.join(timeout=1)
  if status is not None:child.returncode=os.waitstatus_to_exitcode(status)
  for pid,v in processes().items():
   if pid in known or v['pgrp']==child.pid or v['ppid']==os.getpid():
    if pid!=os.getpid():
     try:os.kill(pid,signal.SIGKILL)
     except ProcessLookupError:pass
  while True:
   try:os.waitpid(-1,0)
   except ChildProcessError:break
  remaining={pid:v for pid,v in processes().items() if pid in known or v['pgrp']==child.pid or v['ppid']==os.getpid()}
parent1=resource.getrusage(resource.RUSAGE_SELF)
receipt={'argv':argv,'settings':settings,'input_commit':'16a535defe78b3414f87b34043f565667e289ea7','input_tree':'ab2cb9eb86339ba146150e8568fbd832d8a00f00','base':'14afcde1c5e7777ecb5697bdbfb80394dd9e5fb2','diff_sha256':digest(P/'verified.diff'),'diff_bytes':(P/'verified.diff').stat().st_size,'local_binary_source':'e22fc35e53c40f273dcf0981ce81cacfc15236a7','binary_sha256':digest(B),'hosted_binary_or_cache_identity_reproduced':False,'deadline_s':90,'deadline_fired':deadline_fired,'kill_requested_elapsed_s':kill_at,'root_termination_elapsed_s':root_elapsed,'exit':child.returncode,'fatal_observer_error':fatal,'permission_denied':bool(fatal and 'ObserverDenied' in fatal),'root_wait4_cpu_s':usage.ru_utime+usage.ru_stime if usage else None,'root_wait4_peak_rss_kib':usage.ru_maxrss if usage else None,'sampled_tree_peak_rss_bytes':peak,'samples':snapshot_count,'stack_samples':stack_count,'sampler_stop_unwind_wall_sum_s':observer.sample_wall_s,'sampler_thread_pause_sum_s':observer.pause_sum_s,'observer_self_cpu_s':parent1.ru_utime+parent1.ru_stime-parent0.ru_utime-parent0.ru_stime,'observer_errors':observer.errors,'stack_frame_limit':32,'sampling_period_min_s':1,'stdout_bytes':(run/'stdout.json').stat().st_size,'remaining_owned_processes':remaining,'limitations':['Diagnostic only; not uninstrumented timing or hosted executable/cache reproduction.','CPU delta is assigned to an endpoint stack at one-second sampling; phase transitions can alias.','Thread pause sums and observer CPU do not fully bound perturbation or allocator-lock effects.','Truncated/unresolved stacks retained; missing phases are not zero cost.']}
(run/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt,indent=2))
assert not remaining,remaining
assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT)
if fatal:raise SystemExit(2)
