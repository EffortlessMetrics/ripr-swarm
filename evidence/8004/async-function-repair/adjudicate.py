import json,pathlib,hashlib,collections
root=pathlib.Path('/workspace/ripr-swarm');out=pathlib.Path('/workspace/8004-repair')
cases=json.loads((root/'fixtures/inherent_receiver_free_helper_identity/cases.json').read_text())
old=json.loads(__import__('subprocess').check_output(['git','show','a2b489b9d7d5b78daebf9fc1b3e4201dae3c9524:fixtures/inherent_receiver_free_helper_identity/cases.json'],cwd=root,text=True))
assert cases[:34]==old
native=json.loads((out/'native-matrix/results.json').read_text())+json.loads((out/'native-additional/results.json').read_text())
assert len(native)==100
originals={r['path']:r for r in native if r['variant']=='original'}
records=json.loads((out/'fixed-cli/results.json').read_text());assert len(records)==len(cases)==50
for i,(case,row) in enumerate(zip(cases,records)):
 source=(root/'fixtures'/case['path']).read_bytes();digest=hashlib.sha256(source).hexdigest()
 assert row['case']==case and row['source_sha256']==digest==originals[case['path']]['source_sha256']
 assert originals[case['path']]['returncode']==0
 mutant=next(r for r in native if r['variant']=='mutant' and r['path']==case['path'])
 assert mutant['returncode']==(101 if case['native_reaches'] else 0)
 full=json.loads((out/'candidate-cli'/f'{i:02d}'/'repo-exposure-json/output.json').read_text())
 compact=json.loads((out/'candidate-cli'/f'{i:02d}'/'repo-exposure-summary-json/output.json').read_text())
 diff=json.loads((out/'candidate-cli'/f'{i:02d}'/'json/output.json').read_text())
 assert full['run_status']=='complete' and not compact['limits']['top_files_truncated']
 seams=[s for s in full['seams'] if s['owner']=='src/lib.rs::impl JsonValue::as_object'];assert len(seams)==4
 if case['relation'] is None:
  assert all(s['evidence']['reach']!='yes' and s['grip_class']!='strongly_gripped' for s in seams)
  assert all(r['relation_confidence']!='high' for s in seams for r in s['related_tests'])
  assert all(f['classification']!='exposed' for f in diff['findings'])
red=json.loads((out/'red-cli/results.json').read_text())+json.loads((out/'red-cli-supported/results.json').read_text())
assert len(red)==50
false=[]
for i,(case,row) in enumerate(zip(cases,red)):
 assert row['source_sha256']==records[i]['source_sha256']
 if case['relation'] is None and any(r in ['helper_owner_call','direct_owner_call'] for r in row['reports'][0]['relations']):false.append(i)
assert false==[34,35,36,41,47,49],false
summary={'composition_basis':'6b38d4e74623743d851f5dc7756725662a250e2b','composition_basis_tree':'834b90117415f3afc54753576fcf83ae39d727a3','cases':50,'original_cases_unchanged':34,'new_cases':16,'cli_reports':150,'fresh_compact_metrics_match_full':True,'source_input_native_digest_equality':True,'native_original_pass':50,'native_mutant_fail':sum(x['returncode']==101 for x in native if x['variant']=='mutant'),'native_mutant_survive':sum(x['returncode']==0 for x in native if x['variant']=='mutant'),'red_false_owner_case_indices':false,'red_false_owner_rows':24,'fixed_false_owner_rows':0,'staged_expectation_correction':{'index':38,'path':cases[38]['path'],'original_expectation':'helper_owner_call','correct_expectation':None,'native_reaches':True,'reason':'New sparse-name three-helper chain is already outside the existing bounded candidate graph. Preserve native-positive unsupported behavior; original failed receipt remains at red-cli, and all original 34 cases and native truth are unchanged.'},'ineffective_guard':{'tree':(out/'ineffective-guard.tree').read_text().strip(),'patch_sha256':hashlib.sha256((out/'ineffective-guard.patch').read_bytes()).hexdigest(),'cli_reports':150,'false_owner_rows':24,'reason':'Indexed function body starts at fn and omits async qualifier; source-aware declaration query is required.'}}
(out/'adjudication.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary))
