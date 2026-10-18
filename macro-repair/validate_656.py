import pathlib,subprocess,json,shutil,hashlib,sys,os
root=pathlib.Path('/workspace/5500-macro-repair');repo=pathlib.Path('/workspace/ripr-swarm');head='656d6d7effb51a888c4b627e2a0cd44c3bbd37ef';tree='998fd59f62e93a71cd8e20d3b1080f48bd1257a7';rows=[]
steps=[('affected-final',['cargo','test','--locked','-p','ripr','--test','field_match_selector','--test','native_path_roots']),('check-fast-final',['cargo','xtask','check-fast']),('precommit-final',['cargo','xtask','precommit']),('dogfood-final',['cargo','xtask','dogfood']),('honesty-final',['cargo','xtask','check-evidence-promotion-honesty'])]
for name,cmd in steps:
 assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()==head
 (root/'validation-state.json').write_text(json.dumps({'head':head,'tree':tree,'active':name,'completed':rows},indent=2)+'\n')
 with (root/(name+'.log')).open('w') as log:r=subprocess.run(cmd,cwd=repo,stdout=log,stderr=subprocess.STDOUT)
 rows.append({'stage':name,'command':cmd,'native_exit':r.returncode});print(name+' native_exit='+str(r.returncode),flush=True)
 (root/'validation-state.json').write_text(json.dumps({'head':head,'tree':tree,'active':None,'completed':rows},indent=2)+'\n')
 if r.returncode:sys.exit(r.returncode)
 if name=='focused-final':
  binary=root/'ripr-final-14554b025';shutil.copy2('/workspace/5500-target/debug/ripr',binary);version=subprocess.check_output([str(binary),'--version'],text=True).strip();assert head in version and 'dirty' not in version
  receipt={'head':head,'tree':tree,'path':str(binary),'version':version,'sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'bytes':binary.stat().st_size,'env':{k:os.environ[k] for k in ['CARGO_BUILD_JOBS','CARGO_INCREMENTAL','CARGO_PROFILE_DEV_DEBUG','CARGO_PROFILE_TEST_DEBUG','RUST_TEST_THREADS','RAYON_NUM_THREADS']}}
  (root/'final-cli-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print('final_cli_ready '+receipt['sha256'],flush=True)
 reports=repo/'target/ripr/reports'
 files={'check-fast-final':['check-fast.md','check-fast-selector.md'],'precommit-final':['precommit-v2.json','precommit-v2.md','goldens.md','golden-drift.json','golden-drift.md','traceability.md'],'dogfood-final':['dogfood.json','dogfood.md'],'honesty-final':['evidence-promotion-honesty.md']}.get(name,[])
 for filename in files:shutil.copy2(reports/filename,root/('final-'+filename))
(root/'validation-terminal.json').write_text(json.dumps({'head':head,'tree':tree,'all_native_exit_zero':True,'stages':rows,'claim':'Local bounded proof only; no hosted qualification or consumer improvement inferred.'},indent=2)+'\n')
