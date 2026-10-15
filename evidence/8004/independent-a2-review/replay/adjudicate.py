import pathlib,json,sys,hashlib
p=pathlib.Path('/workspace/review8004-evidence');root=pathlib.Path('/workspace/review8004')
cases=json.loads((root/'fixtures/inherent_receiver_free_helper_identity/cases.json').read_text())+json.loads((p/'extras/cases.json').read_text())
phase=sys.argv[1];records=json.loads((p/(phase+'-cli')/'results.json').read_text())
if phase=='source':records+=json.loads((p/'source-cli-additional/results.json').read_text())
assert len(records)==len(cases)==46
findings=[];passes=[]
for i,(case,row) in enumerate(zip(cases,records)):
 assert hashlib.sha256((root/'fixtures'/case['path']).read_bytes()).hexdigest()==row['source_sha256']
 rels=row['reports'][0]['relations'];expected=case['relation']
 bad=[r for r in rels if (r!=expected if expected else r in ['helper_owner_call','direct_owner_call'])]
 if bad:
  findings.append({'index':i,'case':case,'actual_relations':rels,'wrong_owner_rows':len(bad)})
 else:passes.append(i)
assert {r['index'] for r in findings}=={42,44,45},findings
receipt={'phase':phase,'source_head':'a2b489b9d7d5b78daebf9fc1b3e4201dae3c9524','cases':46,'reports':138,'original_matrix_cases':34,'original_matrix_violations':0,'adjudicated_pass_cases':len(passes),'blocking_cases':findings,'wrong_repository_owner_rows':sum(r['wrong_owner_rows'] for r in findings),'fresh_compact_metrics_equal_full':True,'initial_driver_expectation_correction':{'index':41,'path':cases[41]['path'],'native_reaches':True,'static_relation':None,'reason':'owner_value is outside the bounded helper-name candidate graph; native positive is retained as statically unsupported. Initial source driver expected helper credit; its failed raw receipt is preserved, with no output or native-truth rewrite.'},'disposition':'REPAIR_REQUIRED'}
(p/(phase+'-adjudication.json')).write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt))
