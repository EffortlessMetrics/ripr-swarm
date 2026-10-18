from pathlib import Path
import json,os,subprocess,hashlib
lab=Path('/workspace/5500-macro-review')
binary=Path('/workspace/5500-macro-repair/ripr-final-656d6d7ef')
old=json.loads(Path('/workspace/5500-review/f4be-cli-controls-receipt.json').read_text())['records']
cases={r['case']:(Path('/workspace/5500-review')/r['case'],r['headers'][0]['classification'],r['headers'][0]['id']) for r in old}
cases.update({
 'rootmacro':(Path('/workspace/5500-macro-repair/repro'),'weakly_exposed','probe:src_lib.rs:match_arm:54eaf025'),
 'moduleMAX':(lab/'expected-identity','weakly_exposed','probe:src_lib.rs:match_arm:54eaf025'),
 'localproc':(lab/'expected-proc-local','weakly_exposed','probe:src_lib.rs:match_arm:54eaf025'),
 'localmacro':(Path('/workspace/5500-macro-repair/expected-local'),'weakly_exposed','probe:src_lib.rs:match_arm:54eaf025'),
 'registered_scalar':(Path('/workspace/ripr-swarm/fixtures/whole_tail_match_macro_scalar/input'),'weakly_exposed','probe:src_lib.rs:match_arm:54eaf025'),
 'registered_MAX':(Path('/workspace/ripr-swarm/fixtures/whole_tail_match_macro_expected/input'),'weakly_exposed','probe:src_lib.rs:match_arm:d59d892c'),
})
cases.update({
 'ancestor_original':(lab/'ancestor-macro-use-local-tests','weakly_exposed','probe:src_owned.rs:match_arm:b7574575'),
 'ancestor_root_tests':(lab/'ancestor-macro-use','weakly_exposed','probe:src_owned.rs:match_arm:b7574575'),
 'ancestor_primitive_positive':(lab/'ancestor-positive-local-tests','exposed','probe:src_owned.rs:match_arm:b7574575'),
})
records=[]
for name,(root,want,identity) in cases.items():
 diff=root/'input.diff'
 if name.startswith('registered_'): diff=root.parent/'diff.patch'
 env=os.environ.copy();env.update(RIPR_CACHE_DIR=str(lab/'cache-final656final'/name),RAYON_NUM_THREADS='2')
 cmd=[str(binary),'check','--root',str(root),'--diff',str(diff),'--mode','ready','--format','json']
 run=subprocess.run(cmd,env=env,capture_output=True)
 (lab/f'656final-{name}.json').write_bytes(run.stdout);(lab/f'656final-{name}.stderr').write_bytes(run.stderr)
 assert run.returncode==0,(name,run.returncode)
 d=json.loads(run.stdout);hs=[f for f in d['findings'] if f['probe']['family']=='match_arm' and f['probe']['expression'].startswith('match ') and '=>' not in f['probe']['expression']]
 assert len(hs)==1,(name,len(hs));h=hs[0]
 assert h['classification']==want and h['id']==identity,(name,h['classification'],want,h['id'],identity)
 receipt=any('whole_tail_match_return_observation' in s for s in h['evidence'])
 assert receipt==(want=='exposed'),(name,receipt,want)
 records.append(dict(case=name,command=cmd,native_exit=run.returncode,id=h['id'],classification=h['classification'],whole_tail_receipt=receipt))
result=dict(head='656d6d7effb51a888c4b627e2a0cd44c3bbd37ef',binary=str(binary),sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),prior_control_count=len(old),unique_controls=len(records),records=records)
(lab/'final656final-cli-review-receipt.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({'prior_controls':len(old),'unique_controls':len(records),'results':[{k:r[k] for k in ['case','classification','whole_tail_receipt']} for r in records]},indent=2))
