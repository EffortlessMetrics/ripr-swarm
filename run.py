import pathlib,os,json,subprocess,time,datetime,signal,shutil,sys
P=pathlib.Path('/workspace/scratch/8265-reconstruction');R=pathlib.Path('/workspace/macro-site-parity');A=P/'admission.json'
if not A.exists():A.write_text(json.dumps({'initial_free':shutil.disk_usage(P).free,'reserve':10*1024**3,'peak_additional':2*1024**3,'worktree':str(R),'target':'/workspace/ripr-swarm/target','retention':'source remotely preserved; new receipts and distinct RED/GREEN witnesses retained; shared target untouched except ordinary Cargo writes; no destructive cleanup','base':'006eaf853fce5713a2e9044ba9c8d6a9a873f3d0','reconstruction':True},indent=2)+'\n')
ad=json.loads(A.read_text());env=os.environ.copy();env.update(PATH='/workspace/.cargo/bin:'+env['PATH'],CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',RUSTUP_TOOLCHAIN='1.95.0',CARGO_TARGET_DIR='/workspace/ripr-swarm/target',CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='2',RUST_TEST_THREADS='2')
def run(label,args):
 assert not (P/(label+'.json')).exists();head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip();started=datetime.datetime.now(datetime.timezone.utc).isoformat();t=time.monotonic();abort=None
 (P/(label+'-env.json')).write_text(json.dumps({k:env.get(k) for k in ['PATH','CARGO_HOME','RUSTUP_HOME','RUSTUP_TOOLCHAIN','CARGO_TARGET_DIR','CARGO_INCREMENTAL','CARGO_BUILD_JOBS','RUST_TEST_THREADS','RUSTFLAGS','RUSTC_WRAPPER']},indent=2)+'\n')
 with (P/(label+'.stdout')).open('w') as out,(P/(label+'.stderr')).open('w') as err:
  child=subprocess.Popen(args,cwd=R,env=env,stdout=out,stderr=err,start_new_session=True)
  while child.poll() is None:
   free=shutil.disk_usage(P).free
   if free<ad['reserve'] or ad['initial_free']-free>ad['peak_additional']:
    abort='storage admission exhausted';os.killpg(child.pid,signal.SIGTERM);break
   time.sleep(2)
  try:code=child.wait(timeout=15)
  except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);code=child.wait()
 receipt={'argv':args,'head':head,'tree':subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=R,text=True).strip(),'started_utc':started,'finished_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'elapsed_s':time.monotonic()-t,'exit':code,'abort':abort,'process_group':child.pid,'free_after':shutil.disk_usage(P).free};(P/(label+'.json')).write_text(json.dumps(receipt,indent=2)+'\n');print(label,code,abort,flush=True);return code
if sys.argv[1]=='baseline':
 for label,args in [('base-doctor',['cargo','xtask','worktree','doctor']),('base-check-fast',['cargo','xtask','check-fast']),('base-check',['cargo','check','--locked','-p','ripr','--all-targets'])]:
  code=run(label,args)
  if code:sys.exit(code)
else:sys.exit(run(sys.argv[1],sys.argv[2:]))
