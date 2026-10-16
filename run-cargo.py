import sys,os,pathlib,subprocess,time,json,shutil
P=pathlib.Path('/workspace/scratch/module-scope-walk');R=pathlib.Path('/workspace/module-scope-walk');label=sys.argv[1];args=sys.argv[2:];env=os.environ.copy();env.update(json.loads((P/'env.json').read_text()));
if args and args[0]=='test':env['CARGO_INCREMENTAL']='1'
start=time.monotonic();assert not (P/(label+'.receipt.json')).exists()
with (P/(label+'.stdout')).open('w') as out,(P/(label+'.stderr')).open('w') as err: code=subprocess.call(['cargo',*args],cwd=R,env=env,stdout=out,stderr=err)
index=P/(label+'.index');subprocess.check_call(['git','read-tree','HEAD'],cwd=R,env={**env,'GIT_INDEX_FILE':str(index)});subprocess.check_call(['git','add','-A'],cwd=R,env={**env,'GIT_INDEX_FILE':str(index)});tree=subprocess.check_output(['git','write-tree'],cwd=R,env={**env,'GIT_INDEX_FILE':str(index)},text=True).strip()
(P/(label+'.receipt.json')).write_text(json.dumps({'argv':['cargo',*args],'exit':code,'elapsed_s':time.monotonic()-start,'CARGO_INCREMENTAL':env['CARGO_INCREMENTAL'],'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip(),'executed_worktree_tree':tree},indent=2)+'\n');print(label,code,flush=True);raise SystemExit(code)
