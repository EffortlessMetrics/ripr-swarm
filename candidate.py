import pathlib,subprocess,json,hashlib,shutil
P=pathlib.Path('/workspace/scratch/module-scope-walk');R=pathlib.Path('/workspace/module-scope-walk')
for label,args in [('candidate-parser-tests',['test','--locked','-p','ripr','--lib','analysis::syntax::ra::','--','--nocapture']),('candidate-helper-tests',['test','--locked','-p','ripr','--lib','analysis::facts::test_helpers::','--','--nocapture']),('candidate-build',['build','--locked','-p','ripr','--bin','ripr','--message-format=json'])]:
 assert shutil.disk_usage(P).free>10*1024**3,'storage reserve admission failed'
 subprocess.run(['python',str(P/'run-cargo.py'),label,*args],check=True)
 if label=='candidate-parser-tests':subprocess.run(['python',str(P/'freeze-test.py'),label],check=True)
 if label=='candidate-build':
  rows=[]
  for l in (P/(label+'.stdout')).read_text().splitlines():
   try:rows.append(json.loads(l))
   except json.JSONDecodeError:pass
  a=[x for x in rows if x.get('reason')=='compiler-artifact' and x.get('executable') and x['target']['name']=='ripr'][-1];dst=P/'candidate-ripr';assert not dst.exists();shutil.copy2(a['executable'],dst);r=json.loads((P/(label+'.receipt.json')).read_text());assert not subprocess.check_output(['git','status','--porcelain'],cwd=R)
  (P/'candidate-binary.json').write_text(json.dumps({'source':r['head'],'tree':r['executed_worktree_tree'],'artifact':a,'sha256':hashlib.file_digest(dst.open('rb'),'sha256').hexdigest(),'version':subprocess.check_output([str(dst),'--version'],text=True)},indent=2)+'\n')
