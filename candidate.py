import os,pathlib,subprocess,json,time,shutil,hashlib
P=pathlib.Path('/workspace/scratch/probe-shape-reuse');R=pathlib.Path('/workspace/probe-shape-reuse')
env=os.environ.copy();env.update(PATH='/workspace/.cargo/bin:'+env['PATH'],CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',CARGO_TARGET_DIR='/workspace/ripr-swarm/target',CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='2',RUST_TEST_THREADS='2')
receipts=[]
for label,args in [('candidate-units',['test','--locked','-p','ripr','--lib','analysis::syntax::ra::','--','--nocapture']),('candidate-build',['build','--locked','-p','ripr','--bin','ripr','--message-format=json']),('candidate-check-fast',['xtask','check-fast']),('candidate-precommit',['xtask','precommit'])]:
 started=time.monotonic()
 with (P/(label+'.stdout')).open('w') as out,(P/(label+'.stderr')).open('w') as err:code=subprocess.call(['cargo',*args],cwd=R,env=env,stdout=out,stderr=err)
 receipts.append(dict(label=label,head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=R).decode().strip(),command=['cargo',*args],exit=code,elapsed_s=time.monotonic()-started))
 (P/'candidate-receipts.json').write_text(json.dumps(receipts,indent=2)+'\n');print(label,code,flush=True)
 if code:raise SystemExit(code)
 if label=='candidate-build':
  rows=[json.loads(l) for l in (P/(label+'.stdout')).read_text().splitlines()];a=[r for r in rows if r.get('reason')=='compiler-artifact' and r.get('executable') and r['target']['name']=='ripr'][-1]
  shutil.copy2(a['executable'],P/'candidate-ripr')
  (P/'candidate-binary.json').write_text(json.dumps({'source':receipts[-1]['head'],'tree':subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=R).decode().strip(),'artifact':a,'sha256':hashlib.sha256((P/'candidate-ripr').read_bytes()).hexdigest(),'version':subprocess.check_output([str(P/'candidate-ripr'),'--version']).decode().strip()},indent=2))
