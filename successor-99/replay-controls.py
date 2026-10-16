import pathlib,json,hashlib,subprocess,signal,os,time,ctypes
P=pathlib.Path('/workspace/scratch/8262-frozen-replay');S=P/'successor-99';assert ctypes.CDLL(None).prctl(36,1,0,0,0)==0
binary=S/'candidate-ripr';digest=lambda p:hashlib.file_digest(p.open('rb'),'sha256').hexdigest();assert digest(binary)=='3b3c4beb2e7a1bd1ef12485fdf11db8a5ebd41e6b5413f47cc34e6fc6f858aee';snapshots={**json.loads((P/'control-receipts.json').read_text())['inputs'],**json.loads((P/'public-extra-receipts.json').read_text())['inputs']};rows=[]
def owned(group):
 r={}
 for d in pathlib.Path('/proc').iterdir():
  if not d.name.isdigit():continue
  try:
   f=(d/'stat').read_text().rsplit(')',1)[1].split()
   if int(f[2])==group or int(f[1])==os.getpid():r[int(d.name)]={'ppid':int(f[1]),'pgrp':int(f[2])}
  except (OSError,ValueError,IndexError):pass
 return r
for name,files in snapshots.items():
 root=P/'controls'/name;assert {f:digest(root/f) for f in files}==files;cache=S/('cache-'+name);assert not cache.exists();env=os.environ.copy();env.update(PATH='/workspace/.cargo/bin:'+env['PATH'],CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',RUSTUP_TOOLCHAIN='1.95.0',RAYON_NUM_THREADS='4',RIPR_CACHE_DIR=str(cache));env.pop('RIPR_INDEX_PROFILE',None);env.pop('RIPR_REPO_EXPOSURE_LATENCY_TRACE',None);argv=[str(binary),'check','--root',str(root),'--diff',str(root/'change.diff'),'--format','json','--mode','ready'];t=time.monotonic();censored=False
 with (S/(name+'.json')).open('wb') as out,(S/(name+'.stderr')).open('wb') as err:
  child=subprocess.Popen(argv,cwd=root,env=env,stdout=out,stderr=err,start_new_session=True)
  while True:
   got,status,usage=os.wait4(child.pid,os.WNOHANG)
   if got:break
   if time.monotonic()-t>=30:censored=True;os.killpg(child.pid,signal.SIGKILL);_,status,usage=os.wait4(child.pid,0);break
   time.sleep(.01)
  elapsed=time.monotonic()-t;child.returncode=os.waitstatus_to_exitcode(status)
  try:os.killpg(child.pid,signal.SIGKILL)
  except ProcessLookupError:pass
  for pid in owned(child.pid):
   try:os.kill(pid,signal.SIGKILL)
   except ProcessLookupError:pass
  while True:
   try:os.waitpid(-1,0)
   except ChildProcessError:break
 row={'case':name,'argv':argv,'cwd':str(root),'exit':child.returncode,'censored':censored,'elapsed_s':elapsed,'root_cpu_s':usage.ru_utime+usage.ru_stime,'root_maxrss_kib':usage.ru_maxrss,'remaining_owned':owned(child.pid),'stdout_bytes':(S/(name+'.json')).stat().st_size,'stdout_sha256':digest(S/(name+'.json')),'baseline_stdout_sha256':digest(P/(name+'-baseline.json')),'head55_stdout_sha256':digest(P/(name+'-candidate.json')),'binary_sha256':digest(binary),'inputs':files};rows.append(row);(S/'receipts.json').write_text(json.dumps(rows,indent=2)+'\n');assert row['exit']==0 and not censored and not row['remaining_owned'];print(name,child.returncode,flush=True)
comparisons=[]
for row in rows:
 name=row['case'];o=json.loads((S/(name+'.json')).read_text());assert o['analysis_outcome']['analysis_complete'] and o['findings'];comparisons.append({'case':name,'baseline_equal':row['stdout_sha256']==row['baseline_stdout_sha256'],'head55_equal':row['stdout_sha256']==row['head55_stdout_sha256'],'summary':o['summary']})
(S/'comparisons.json').write_text(json.dumps(comparisons,indent=2)+'\n');print(json.dumps(comparisons,indent=2))
