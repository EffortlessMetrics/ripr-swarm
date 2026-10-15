import pathlib,subprocess,json,hashlib,os,sys,difflib
root=pathlib.Path('/workspace/review8004')
binary=pathlib.Path(sys.argv[1]);label=sys.argv[2]
out=pathlib.Path('/workspace/review8004-evidence')/label;out.mkdir(exist_ok=True)
env=dict(os.environ,CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',PATH='/workspace/.cargo/bin:/usr/bin:/bin')
cases=json.loads((root/'fixtures/inherent_receiver_free_helper_identity/cases.json').read_text())+json.loads((out.parent/'extras/cases.json').read_text())
results=[];violations=[]
for i,c in list(enumerate(cases))[int(sys.argv[3]) if len(sys.argv)>3 else 0:]:
 source=(root/'fixtures'/c['path']).read_text()
 row={'case':c,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'reports':[]}
 full=None
 for fmt in ['repo-exposure-json','repo-exposure-summary-json','json']:
  d=out/f'{i:02d}'/fmt;(d/'src').mkdir(parents=True,exist_ok=True)
  (d/'Cargo.toml').write_text('[package]\nname="review-control"\nversion="0.0.0"\nedition="2024"\n[workspace]\n')
  (d/'src/lib.rs').write_text(source)
  patch=''.join(difflib.unified_diff(source.replace('Self::Object(value) => Some(*value),','Self::Object(value) => None,').splitlines(True),source.splitlines(True),fromfile='a/src/lib.rs',tofile='b/src/lib.rs'))
  (d/'diff.patch').write_text('diff --git a/src/lib.rs b/src/lib.rs\n'+patch)
  argv=[str(binary),'check','--root',str(d),'--mode','ready','--format',fmt]
  if fmt=='json': argv+=['--diff',str(d/'diff.patch')]
  run=subprocess.run(argv,env=env,capture_output=True,text=True)
  (d/'output.json').write_text(run.stdout);(d/'stderr.log').write_text(run.stderr)
  assert run.returncode==0,(label,i,fmt,run.returncode,run.stderr[:300])
  data=json.loads(run.stdout)
  report={'format':fmt,'argv':argv,'exit':run.returncode}
  if fmt=='repo-exposure-json':
   seams=[s for s in data['seams'] if s['owner']=='src/lib.rs::impl JsonValue::as_object']
   assert len(seams)==4,(i,'missing owner seams',len(seams))
   assert data['run_status']=='complete'
   relations=[]
   for s in seams:
    rows=s['related_tests'];assert len(rows)==1 and rows[0]['name']==c['name'],(i,'wrong subject',rows)
    rel=rows[0]['relation_reason'];relations.append(rel)
    expected=c['relation']
    if (expected and rel!=expected) or (not expected and rel in ['direct_owner_call','helper_owner_call']): violations.append([i,fmt,rel,expected])
   report['relations']=relations;report['metrics']=data['metrics'];full=data
  elif fmt=='repo-exposure-summary-json':
   m=data['metrics'];assert m['raw_seams']==full['metrics']['seams_total']
   for k,v in m['grip_class'].items(): assert full['metrics'][k]==v,(i,k,v,full['metrics'][k])
   report['metrics']=m
  else:
   assert len(data['findings'])==2,(i,'diff probes',len(data['findings']))
   relations=[]
   for f in data['findings']:
    rows=f['related_tests'];assert rows and all(r['name']==c['name'] for r in rows),(i,'diff subjects',rows)
    for r in rows:
     rel=r['relation_reason'];relations.append(rel)
     # Diff does not claim helper traversal; direct positives must remain direct.
     if c['relation']=='direct_owner_call' and rel!='direct_owner_call':violations.append([i,fmt,rel,c['relation']])
     if c['relation'] is None and rel in ['direct_owner_call','helper_owner_call']:violations.append([i,fmt,rel,None])
   report['relations']=relations
  row['reports'].append(report)
 results.append(row)
 (out/'results.json').write_text(json.dumps(results,indent=2)+'\n')
 (out/'violations.json').write_text(json.dumps(violations,indent=2)+'\n')
 print(i,c['path'],[r.get('relations') for r in row['reports']],flush=True)
print(json.dumps({'cases':len(cases),'reports':sum(len(r['reports']) for r in results),'violations':violations}))
assert not violations
