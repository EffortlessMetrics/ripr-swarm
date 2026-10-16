import os,pathlib,subprocess,json,time,shutil,hashlib
P=pathlib.Path('/workspace/scratch/probe-shape-reuse');R=pathlib.Path('/workspace/probe-shape-reuse')
env=os.environ.copy();env.update(PATH='/workspace/.cargo/bin:'+env['PATH'],CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',CARGO_TARGET_DIR='/workspace/ripr-swarm/target',CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='2')
receipts=[]
for label,args in [('baseline-check-fast',['xtask','check-fast']),('baseline-check',['check','--locked','-p','ripr','--all-targets']),('baseline-build',['build','--locked','-p','ripr','--bin','ripr','--message-format=json'])]:
 started=time.monotonic()
 with (P/(label+'.stdout')).open('w') as out,(P/(label+'.stderr')).open('w') as err:code=subprocess.call(['cargo',*args],cwd=R,env=env,stdout=out,stderr=err)
 receipts.append(dict(label=label,command=['cargo',*args],exit=code,elapsed_s=time.monotonic()-started))
 (P/'baseline-receipts.json').write_text(json.dumps(receipts,indent=2)+'\n')
 print(label,code,flush=True)
 if code:raise SystemExit(code)
rows=[json.loads(l) for l in (P/'baseline-build.stdout').read_text().splitlines()]
a=[r for r in rows if r.get('reason')=='compiler-artifact' and r.get('executable') and r['target']['name']=='ripr'][-1]
shutil.copy2(a['executable'],P/'baseline-ripr')
(P/'baseline-binary.json').write_text(json.dumps({'source':'14afcde1c5e7777ecb5697bdbfb80394dd9e5fb2','artifact':a,'sha256':hashlib.sha256((P/'baseline-ripr').read_bytes()).hexdigest()},indent=2))
