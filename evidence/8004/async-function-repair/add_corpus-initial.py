from pathlib import Path
import json,copy,shutil,hashlib
repo=Path('/workspace/ripr-swarm');out=Path('/workspace/8004-repair');p=repo/'fixtures/evidence-promotion-honesty-corpus/corpus.json'
original=json.loads(p.read_text());data=copy.deepcopy(original);assert len(data['cases'])==317
repo_template=next(x for x in data['cases'] if x['id']=='rust_repo_inherent_receiver_async_operand_helper')
diff_template=next(x for x in data['cases'] if x['id']=='rust_diff_inherent_receiver_async_operand_helper')
added=[]
for index,label in [(34,'operand'),(35,'entry'),(36,'last-hop'),(41,'method-entry'),(43,'activated-unsupported'),(47,'intermediate'),(49,'method-last-hop')]:
 raw=out/'fixed-cli'/f'{index:02d}'/'repo-exposure-json/output.json';report=json.loads(raw.read_text())
 seams=[s for s in report['seams'] if s['owner']=='src/lib.rs::impl JsonValue::as_object'];assert len(seams)==4
 assert all(s['grip_class']=='opaque' and s['evidence']['reach']=='weak' for s in seams)
 row=copy.deepcopy(repo_template);row['id']='rust_repo_inherent_async_fn_'+label.replace('-','_')
 row['source_report']='fixtures/evidence-promotion-honesty-corpus/reports/rust-inherent-async-fn-'+label+'.json'
 row['vector']='unpolled_async_fn_helper_is_not_owner_execution' if index!=43 else 'explicit_polling_keeps_bounded_helper_activation_unsupported'
 target=repo/row['source_report'];assert not target.exists();shutil.copyfile(raw,target)
 added.append(row);data['cases'].append(row)
row=copy.deepcopy(diff_template);row['id']='rust_diff_inherent_async_fn_operand';row['source_fixture']='fixtures/inherent_receiver_async_function';row['vector']='unpolled_async_fn_helper_is_not_owner_execution';added.append(row);data['cases'].append(row)
assert len(data['cases'])==325 and len({r['id'] for r in data['cases']})==325
assert data['cases'][:317]==original['cases']
assert {k:v for k,v in data.items() if k!='cases'}=={k:v for k,v in original.items() if k!='cases'}
p.write_text(json.dumps(data,indent=2)+'\n')
(out/'corpus-addition.json').write_text(json.dumps({'input_count':317,'final_count':325,'all_complete_input_rows_preserved':True,'all_metadata_preserved':True,'added_ids':[r['id'] for r in added]},indent=2)+'\n')
p=repo/'.ripr/traceability.toml';s=p.read_text();needle='  "crates/ripr/src/analysis/test_grip_evidence/related_tests/context.rs",';assert needle in s;s=s.replace(needle,needle+'\n  "crates/ripr/src/analysis/classify/owner_pin.rs::function_declaration_is_async",',1);p.write_text(s)
