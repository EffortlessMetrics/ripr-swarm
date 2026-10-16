import pathlib,subprocess,os,time,json,shutil,hashlib
P=pathlib.Path('/workspace/scratch/module-scope-walk');R=pathlib.Path('/workspace/module-scope-walk');env=os.environ.copy();env.update(PATH='/workspace/.cargo/bin:'+env['PATH'],CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',CARGO_TARGET_DIR='/workspace/ripr-swarm/target',CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='2',RUST_TEST_THREADS='2')
(P/'env.json').write_text(json.dumps({k:env[k] for k in ['PATH','CARGO_HOME','RUSTUP_HOME','CARGO_TARGET_DIR','CARGO_INCREMENTAL','CARGO_BUILD_JOBS','RUST_TEST_THREADS']},indent=2))
r=[]
for label,args in [('doctor',['xtask','worktree','doctor']),('baseline-check-fast',['xtask','check-fast']),('baseline-check',['check','--locked','-p','ripr','--all-targets']),('baseline-build',['build','--locked','-p','ripr','--bin','ripr','--message-format=json'])]:
 t=time.monotonic()
 with (P/(label+'.stdout')).open('w') as out,(P/(label+'.stderr')).open('w') as err: code=subprocess.call(['cargo',*args],cwd=R,env=env,stdout=out,stderr=err)
 r.append({'label':label,'argv':['cargo',*args],'exit':code,'elapsed_s':time.monotonic()-t,'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()});(P/'baseline-receipts.json').write_text(json.dumps(r,indent=2)+'\n');print(label,code,flush=True)
 if code:raise SystemExit(code)
 if label=='baseline-check-fast':
  assert subprocess.check_output(['git','merge-base','origin/main','HEAD'],cwd=R,text=True).strip()=='3477bf95a9ead7f35ec099a4e6cf17af96708003'
  assert not subprocess.check_output(['git','diff','--name-only','origin/main...HEAD'],cwd=R)
  for name in ['check-fast.md','check-fast-selector.md','worktree-doctor.md']:
   src=R/'target/ripr/reports'/name
   if src.exists():shutil.copy2(src,P/('baseline-'+name))
 if label=='baseline-build':
  rows=[json.loads(l) for l in (P/(label+'.stdout')).read_text().splitlines()];a=[x for x in rows if x.get('reason')=='compiler-artifact' and x.get('executable') and x['target']['name']=='ripr'][-1];dst=P/'baseline-ripr';shutil.copy2(a['executable'],dst);h=hashlib.file_digest(dst.open('rb'),'sha256').hexdigest();(P/'baseline-binary.json').write_text(json.dumps({'source':r[-1]['head'],'artifact':a,'sha256':h,'version':subprocess.check_output([str(dst),'--version'],text=True)},indent=2))
