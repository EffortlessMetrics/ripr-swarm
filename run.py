import pathlib, shutil, subprocess, os, json, hashlib
P=pathlib.Path('/workspace/scratch/cli-output-audit'); S=pathlib.Path('/workspace/cli-audit-current'); B='/workspace/ripr-swarm/target/debug/ripr'
def case(name,fixture,extra='',env=None):
 r=P/'cases'/name; shutil.copytree(S/'fixtures'/fixture/'input',r,dirs_exist_ok=True)
 patch=(S/'fixtures'/fixture/'diff.patch').read_text()+extra
 (r/'change.patch').write_text(patch)
 return (name,r,env or {})
cases=[]
for name,f in [('rust','boundary_gap'),('python','python_boundary_gap'),('typescript','typescript_boundary_gap'),('python-disabled','python_disabled'),('typescript-disabled','typescript_disabled'),('typescript-parse','typescript_parse_error_unsupported_syntax')]:
 cases.append(case(name,f))
for lang,f in [('rust','boundary_gap'),('python','python_boundary_gap'),('typescript','typescript_boundary_gap')]:
 cases.append(case(lang+'-malformed',f,'diff --git a/src/absent.rs b/src/absent.rs\n--- a/src/absent.rs\n+++ b/src/absent.rs\n@@ -0,0 +1,1 @@\n'))
 cases.append(case(lang+'-render-limited',f,env={'RIPR_CHECK_FINDINGS_BYTES':'1'}))
 cases.append(case(lang+'-budget-invalid',f,env={'RIPR_PARTIAL_DIFF_FILE_BUDGET':'nope'}))
cases.append(case('rust-missing','boundary_gap','diff --git a/src/absent.rs b/src/absent.rs\n--- a/src/absent.rs\n+++ b/src/absent.rs\n@@ -0,0 +1,1 @@\n+pub fn absent() -> u8 { 1 }\n'))
cases.append(case('mixed-go','boundary_gap','diff --git a/main.go b/main.go\n--- a/main.go\n+++ b/main.go\n@@ -1 +1 @@\n-package a\n+package b\n'))
cases.append(case('rust-partial','boundary_gap','diff --git a/src/second.rs b/src/second.rs\n--- a/src/second.rs\n+++ b/src/second.rs\n@@ -1 +1 @@\n-pub fn second() -> u8 { 0 }\n+pub fn second() -> u8 { 1 }\n', {'RIPR_PARTIAL_DIFF_FILE_BUDGET':'1'}))
(cases[-1][1]/'src/second.rs').write_text('pub fn second() -> u8 { 1 }\n')
rows=[]
for name,r,env in cases:
 for fmt in ['json','badge-json','human']:
  cmd=[B,'check','--root',str(r),'--diff',str(r/'change.patch'),'--mode','fast','--format',fmt,'--quiet']
  out=subprocess.run(cmd,env=dict(os.environ,**env),capture_output=True,timeout=30)
  d=P/'results'/name; d.mkdir(parents=True,exist_ok=True)
  (d/(fmt+'.stdout')).write_bytes(out.stdout); (d/(fmt+'.stderr')).write_bytes(out.stderr)
  row={'case':name,'format':fmt,'argv':cmd,'env':env,'exit':out.returncode,'stdout_sha256':hashlib.sha256(out.stdout).hexdigest(),'stderr':out.stderr.decode()}
  if fmt!='human':
   try:
    v=json.loads(out.stdout); row.update({k:v.get(k) for k in ['analysis_outcome','run_status','run_limitations','runtime_status','downstream_consumable','status','message','warnings'] if k in v}); row['findings']=len(v.get('findings',[]))
   except Exception as e: row['json_error']=str(e)
  rows.append(row)
(P/'receipt.json').write_text(json.dumps(rows,indent=2))
for row in rows:
 a=row.get('analysis_outcome') or {}; print(row['case'],row['format'],row['exit'],a.get('analysis_complete'),a.get('outcome',{}).get('kind'),row.get('run_status'),row.get('status'),row.get('message'),len(row.get('run_limitations',[])))
