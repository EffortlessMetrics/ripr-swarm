import pathlib,json,subprocess,os,difflib,shutil,hashlib
P=pathlib.Path('/workspace/scratch/8210-proof'); A=pathlib.Path('/workspace/scratch/cli-output-audit/cases');rows=[]
cases=[(n,{}) for n in ['rust','python','typescript','python-disabled','typescript-disabled','rust-malformed','python-malformed','typescript-malformed']]
cases += [(n,{'RIPR_PARTIAL_DIFF_FILE_BUDGET':'1'}) for n in ['mixed-partial','mixed-partial-ts']]
cases += [('render-bound',{'RIPR_CHECK_FINDINGS_BYTES':'1'})]
for name,env in cases:
 r=A/name
 for fmt in ['json','human','badge-json']:
  streams=[]
  for variant in ['base','final']:
   cmd=[str(pathlib.Path(os.environ['FINAL_BINARY']) if variant == 'final' else P/'base-ripr'),'check','--root',str(r),'--diff',str(r/'change.patch'),'--mode','fast','--format',fmt,'--quiet'];o=subprocess.run(cmd,env=dict(os.environ,**env),capture_output=True,timeout=30);d=P/'native'/name;d.mkdir(parents=True,exist_ok=True);(d/(variant+'-'+fmt+'.stdout')).write_bytes(o.stdout);(d/(variant+'-'+fmt+'.stderr')).write_bytes(o.stderr);streams.append(o)
   rows.append({'case':name,'variant':variant,'format':fmt,'argv':cmd,'env':env,'exit':o.returncode,'stdout_sha256':hashlib.sha256(o.stdout).hexdigest()})
  before,after=streams;assert before.returncode==after.returncode,(name,fmt,'exit changed');assert before.stderr==after.stderr,(name,fmt,'stderr changed')
  if name.startswith('mixed-partial') and fmt=='json':
   b=json.loads(before.stdout);a=json.loads(after.stdout);assert b['preview_languages'][0]['analyzed'] is True;assert 'preview_languages' not in a;del b['preview_languages'];assert b==a,(name,'unrelated JSON changed');assert a['analysis_outcome']['analysis_complete'] is False;assert a['analysis_scope']['downstream_consumable'] is False
  elif name.startswith('mixed-partial') and fmt=='human':
   label='Python' if name=='mixed-partial' else 'TypeScript';line=f'\nNote: 1 {label} file analyzed under preview support — preview evidence is advisory and may be incomplete. Treat its findings as advisory, not Rust-grade.\n'.encode();assert before.stdout.count(line)==1;assert before.stdout.replace(line,b'')==after.stdout,(name,'unrelated human changed')
  else: assert before.stdout==after.stdout,(name,fmt,'unaffected bytes changed')
# JavaScript-family native check of the same shared routing path.
r=P/'js-case';shutil.copytree(A/'mixed-partial-ts',r,dirs_exist_ok=True,ignore=shutil.ignore_patterns('target','*.ts'));source=(A/'mixed-partial-ts/src/discount.ts').read_text().replace(': number','');(r/'src/discount.js').write_text(source);patch=(A/'mixed-partial-ts/change.patch').read_text().replace('discount.ts','discount.js').replace(': number','');(r/'change.patch').write_text(patch)
for budget in ['1','3']:
 for fmt in ['json','human']:
  cmd=[os.environ['FINAL_BINARY'],'check','--root',str(r),'--diff',str(r/'change.patch'),'--mode','fast','--format',fmt,'--quiet'];o=subprocess.run(cmd,env=dict(os.environ,RIPR_PARTIAL_DIFF_FILE_BUDGET=budget),capture_output=True,timeout=30);d=P/'native'/('javascript-budget'+budget);d.mkdir(parents=True,exist_ok=True);(d/(fmt+'.stdout')).write_bytes(o.stdout);(d/(fmt+'.stderr')).write_bytes(o.stderr);assert o.returncode==0
  if fmt=='json':
   v=json.loads(o.stdout)
   if budget=='1': assert 'preview_languages' not in v and v['analysis_outcome']['analysis_complete'] is False
   else: assert v['preview_languages'][0]['analyzed'] is True and v['preview_languages'][0]['file_count']==1
  elif budget=='1': assert b'JavaScript file analyzed under preview support' not in o.stdout
  else: assert b'1 JavaScript file analyzed under preview support' in o.stdout
  rows.append({'case':'javascript','format':fmt,'budget':budget,'argv':cmd,'exit':o.returncode})
(P/'native-compare-receipt.json').write_text(json.dumps({'rows':rows,'native_invocations':len(rows),'comparisons':33,'unaffected_comparisons_byte_equal':29,'scoped_JSON_human_pairs':4,'javascript_controls':4},indent=2));print(f'{len(rows)} native invocations: 29 unaffected byte-equal pairs; 4 scoped advisory-only JSON/human deltas; 4 JavaScript controls passed')
