import pathlib,json,subprocess,os,time,signal,hashlib,ctypes,sys
P=pathlib.Path('/workspace/scratch/8265-reconstruction');variant=sys.argv[1];binary=P/(variant+'-ripr');S=P/(variant+'-controls');S.mkdir(exist_ok=False);assert ctypes.CDLL(None).prctl(36,1,0,0,0)==0
sha=lambda p:hashlib.file_digest(p.open('rb'),'sha256').hexdigest();manifest=json.loads((P/'recovered-input-manifest.json').read_text());rows=[]
def members(group):
 found=[]
 for d in pathlib.Path('/proc').iterdir():
  if not d.name.isdigit():continue
  try:
   f=(d/'stat').read_text().rsplit(')',1)[1].split()
   if int(f[2])==group or int(f[1])==os.getpid():found.append(int(d.name))
  except (OSError,ValueError,IndexError):pass
 return found
for root in sorted((P/'controls').iterdir()):
 name=root.name;inputs={str(p.relative_to(P)):sha(p) for p in root.rglob('*') if p.is_file()};expected={f:v['sha256'] for f,v in manifest.items() if f.startswith('controls/'+name+'/')};assert inputs==expected
 env=os.environ.copy();cache=S/('cache-'+name);assert not cache.exists();env.update(PATH='/workspace/.cargo/bin:'+env['PATH'],CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',RUSTUP_TOOLCHAIN='1.95.0',RAYON_NUM_THREADS='4',RIPR_CACHE_DIR=str(cache));argv=[str(binary),'check','--root',str(root),'--diff',str(root/'change.diff'),'--format','json','--mode','ready'];t=time.monotonic();censored=False
 with (S/(name+'.json')).open('wb') as out,(S/(name+'.stderr')).open('wb') as err:
  child=subprocess.Popen(argv,cwd=root,env=env,stdout=out,stderr=err,start_new_session=True)
  try:code=child.wait(timeout=30)
  except subprocess.TimeoutExpired:censored=True;os.killpg(child.pid,signal.SIGKILL);code=child.wait()
  for pid in members(child.pid):
   try:os.kill(pid,signal.SIGKILL)
   except ProcessLookupError:pass
  while True:
   try:os.waitpid(-1,0)
   except ChildProcessError:break
 assert not members(child.pid)
 raw=(S/(name+'.json')).read_bytes();row={'case':name,'argv':argv,'exit':code,'elapsed_s':time.monotonic()-t,'censored':censored,'binary_sha256':sha(binary),'stdout_sha256':hashlib.sha256(raw).hexdigest(),'stdout_bytes':len(raw),'remaining_owned':members(child.pid),'inputs':inputs};rows.append(row);(S/'receipts.json').write_text(json.dumps(rows,indent=2)+'\n');assert code==0 and not censored;obj=json.loads(raw);assert obj['analysis_outcome']['analysis_complete'] and obj['findings'];assert {str(p.relative_to(P)):sha(p) for p in root.rglob('*') if p.is_file()}==inputs
 print(name,obj['summary'],flush=True)
