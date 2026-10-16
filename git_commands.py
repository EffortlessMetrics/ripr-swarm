import pathlib,shutil,subprocess,json,os
P=pathlib.Path('/workspace/scratch/cli-output-audit');B='/workspace/ripr-swarm/target/debug/ripr';rows=[]
for lang,src in [('rust','src/lib.rs'),('python','src/discount.py'),('typescript','src/discount.ts')]:
 r=P/'git-cases'/lang;shutil.copytree(P/'cases'/lang,r,dirs_exist_ok=True)
 def git(*args):
  o=subprocess.run(['git',*args],cwd=r,capture_output=True,timeout=15);assert o.returncode==0,(args,o.stderr);return o.stdout.decode().strip()
 git('init','-q');git('add','.');git('-c','user.name=CLI audit fixture','-c','user.email=cli-audit@example.invalid','commit','-qm','fixture base');base=git('rev-parse','HEAD')
 for state in ['no-scope','conflict']:
  if state=='conflict':
   f=r/src; f.write_text('<<<<<<< ours\n'+f.read_text()+'=======\n>>>>>>> theirs\n');git('add',src);git('-c','user.name=CLI audit fixture','-c','user.email=cli-audit@example.invalid','commit','-qm','conflict subject')
  for command in ['check','diff']:
   for fmt in ['json','human']:
    cmd=[B,command,'--root','.','--base',base]+(['--json'] if fmt=='json' else [])
    o=subprocess.run(cmd,cwd=r,capture_output=True,timeout=30);d=P/'results'/(lang+'-'+command+'-'+state);d.mkdir(exist_ok=True);(d/(fmt+'.stdout')).write_bytes(o.stdout);(d/(fmt+'.stderr')).write_bytes(o.stderr)
    v=json.loads(o.stdout) if fmt=='json' else {};a=v.get('analysis_outcome') or {};row={'case':lang+'-'+command+'-'+state,'format':fmt,'exit':o.returncode,'argv':cmd,'cwd':str(r),'base':base,'head':git('rev-parse','HEAD'),'outcome':a.get('outcome',{}).get('kind'),'analysis_complete':a.get('analysis_complete')};rows.append(row);print(row['case'],fmt,row['exit'],row['outcome'])
(P/'git-command-receipt.json').write_text(json.dumps(rows,indent=2))
