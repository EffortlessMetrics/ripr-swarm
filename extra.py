import pathlib,subprocess,json,os
P=pathlib.Path('/workspace/scratch/cli-output-audit');B='/workspace/ripr-swarm/target/debug/ripr'; rows=[]
for name in ['rust-malformed','python-disabled','typescript-parse','rust-partial']:
 r=P/'cases'/name
 for fmt in ['sarif','github']:
  cmd=[B,'check','--root',str(r),'--diff',str(r/'change.patch'),'--mode','fast','--format',fmt,'--quiet']
  env={'RIPR_PARTIAL_DIFF_FILE_BUDGET':'1'} if name=='rust-partial' else {}
  o=subprocess.run(cmd,env=dict(os.environ,**env),capture_output=True,timeout=30);d=P/'results'/name
  (d/(fmt+'.stdout')).write_bytes(o.stdout);(d/(fmt+'.stderr')).write_bytes(o.stderr)
  rows.append({'case':name,'format':fmt,'exit':o.returncode,'argv':cmd,'env':env})
(P/'extra-receipt.json').write_text(json.dumps(rows,indent=2))
for x in rows:print(x['case'],x['format'],x['exit'])
