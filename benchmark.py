import ctypes,hashlib,json,os,pathlib,signal,subprocess,time,sys
P=pathlib.Path("/workspace/scratch/module-scope-walk");root=P/"workload";text=(root/"src/lib.rs").read_text()
assert ctypes.CDLL(None).prctl(36,1,0,0,0)==0
def owned_processes(group):
 result={}
 for d in pathlib.Path('/proc').iterdir():
  if not d.name.isdigit():continue
  try:
   fields=(d/'stat').read_text().rsplit(')',1)[1].split()
   if int(fields[2])==group or int(fields[1])==os.getpid():result[int(d.name)]={'ppid':int(fields[1]),'pgrp':int(fields[2])}
  except (OSError,ValueError,IndexError):pass
 return result
def input_hashes():return {str(q.relative_to(root)):hashlib.sha256(q.read_bytes()).hexdigest() for q in [root/'src/lib.rs',root/'Cargo.toml',root/'Cargo.lock',root/'change.diff']}
frozen_inputs=input_hashes()
records=[];semantics=None
for pair in range(1 if "--baseline-only" in sys.argv else 4):
 for variant in (['baseline'] if '--baseline-only' in sys.argv else (['baseline','candidate'] if pair%2==0 else ['candidate','baseline'])):
  binary=P/f'{variant}-ripr';label=f'preflight-{variant}' if '--baseline-only' in sys.argv else f'{pair}-{variant}';env=os.environ.copy();env.update(PATH='/workspace/.cargo/bin:'+env['PATH'],CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',RUSTUP_TOOLCHAIN='1.95.0',RAYON_NUM_THREADS='4',RIPR_CACHE_DIR=str(P/('bench-cache-'+label)))
  env.pop('RIPR_INDEX_PROFILE',None);env.pop('RIPR_REPO_EXPOSURE_LATENCY_TRACE',None)
  assert not pathlib.Path(env['RIPR_CACHE_DIR']).exists(),'cache must start absent'
  assert not (P/(label+'.json')).exists(),'preserve prior run output'
  argv=[str(binary),'check','--root',str(root),'--diff',str(root/'change.diff'),'--format','json','--mode','ready']
  with (P/(label+'.json')).open('wb') as out,(P/(label+'.stderr')).open('wb') as err:
   started=time.monotonic();p=subprocess.Popen(argv,cwd=root,env=env,stdout=out,stderr=err,start_new_session=True);censored=False
   while True:
    got,status,usage=os.wait4(p.pid,os.WNOHANG)
    if got:break
    if time.monotonic()-started>30:
     censored=True;os.killpg(p.pid,signal.SIGKILL);_,status,usage=os.wait4(p.pid,0);break
    time.sleep(.01)
   elapsed=time.monotonic()-started;p.returncode=os.waitstatus_to_exitcode(status)
   # No descendants are allowed to survive either completion or deadline.
   try:os.killpg(p.pid,signal.SIGKILL)
   except ProcessLookupError:pass
   for pid in owned_processes(p.pid):
    try:os.kill(pid,signal.SIGKILL)
    except ProcessLookupError:pass
   while True:
    try:os.waitpid(-1,0)
    except ChildProcessError:break
  assert input_hashes()==frozen_inputs,'input mutation'
  raw=(P/(label+'.json')).read_bytes();r={'label':label,'argv':argv,'environment':{'RAYON_NUM_THREADS':'4','RUSTUP_TOOLCHAIN':'1.95.0','RIPR_CACHE_DIR':env['RIPR_CACHE_DIR'],'profiling_disabled':True},'elapsed_s':elapsed,'cpu_s':usage.ru_utime+usage.ru_stime,'maxrss_kib':usage.ru_maxrss,'exit':p.returncode,'censored':censored,'stdout_sha256':hashlib.sha256(raw).hexdigest(),'binary_sha256':hashlib.file_digest(binary.open('rb'),'sha256').hexdigest(),'remaining_owned_processes':owned_processes(p.pid)}
  records.append(r);(P/('preflight.json' if '--baseline-only' in sys.argv else 'benchmark.json')).write_text(json.dumps({'input_hashes':frozen_inputs,'input_sha256':hashlib.sha256(text.encode()).hexdigest(),'input_bytes':len(text.encode()),'functions':97,'workload_functions':96,'records':records},indent=2)+'\n')
  assert p.returncode==0 and not censored and not r['remaining_owned_processes'],r
  report=json.loads(raw)
  if semantics is None:semantics=raw
  assert raw==semantics,'whole public JSON bytes changed'
  assert report.get('findings'),'empty workload findings'
  print(label,round(elapsed,4),flush=True)
print('Completed reports have identical JSON bytes; no owned process remains.',flush=True)
