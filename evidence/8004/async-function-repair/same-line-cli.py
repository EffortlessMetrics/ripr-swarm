import pathlib,subprocess,json,os,sys,difflib
root=pathlib.Path('/workspace/ripr-swarm');out=pathlib.Path('/workspace/8004-repair')/sys.argv[2];out.mkdir(exist_ok=True)
source=(root/'fixtures/inherent_receiver_free_helper_identity/controls/same-line-callers.rs').read_text()
env=dict(os.environ,CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',PATH='/workspace/.cargo/bin:/usr/bin:/bin')
rows=[];full=None
for fmt in ['repo-exposure-json','repo-exposure-summary-json','json']:
 d=out/fmt;(d/'src').mkdir(parents=True,exist_ok=True);(d/'src/lib.rs').write_text(source);(d/'Cargo.toml').write_text('[package]\nname="same-line-review"\nversion="0.0.0"\nedition="2024"\n[workspace]\n')
 (d/'diff.patch').write_text('diff --git a/src/lib.rs b/src/lib.rs\n'+''.join(difflib.unified_diff(source.replace('Self::Object(value) => Some(*value),','Self::Object(value) => None,').splitlines(True),source.splitlines(True),fromfile='a/src/lib.rs',tofile='b/src/lib.rs')))
 argv=[sys.argv[1],'check','--root',str(d),'--mode','ready','--format',fmt]+(['--diff',str(d/'diff.patch')] if fmt=='json' else [])
 r=subprocess.run(argv,env=env,capture_output=True,text=True);(d/'output.json').write_text(r.stdout);(d/'stderr.log').write_text(r.stderr);assert r.returncode==0
 data=json.loads(r.stdout)
 if fmt=='repo-exposure-json':
  full=data;seams=[s for s in data['seams'] if s['owner']=='src/lib.rs::impl JsonValue::as_object'];assert len(seams)==4
  for s in seams:
   tests={r['name']:r['relation_reason'] for r in s['related_tests']};assert set(tests)=={'actual','decoy'};assert tests['actual']=='helper_owner_call';assert tests['decoy'] not in ['helper_owner_call','direct_owner_call']
 elif fmt=='repo-exposure-summary-json':
  assert data['metrics']['raw_seams']==full['metrics']['seams_total']
  assert all(full['metrics'][k]==v for k,v in data['metrics']['grip_class'].items())
 else:
  assert len(data['findings'])==2
  for f in data['findings']:
   assert {r['name'] for r in f['related_tests']}=={'actual','decoy'}
 rows.append({'format':fmt,'argv':argv,'exit':r.returncode})
(out/'results.json').write_text(json.dumps(rows,indent=2)+'\n');print('3 reports; both subjects retained; genuine helper high, foreign sibling refused')
