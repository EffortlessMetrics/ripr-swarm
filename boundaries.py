import pathlib,shutil,json,subprocess,os,hashlib
P=pathlib.Path('/workspace/scratch/cli-output-audit');S=pathlib.Path('/workspace/cli-audit-current');B='/workspace/ripr-swarm/target/debug/ripr';rows=[]
def fixture(name,base):
 r=P/'cases'/name;shutil.copytree(S/'fixtures'/base/'input',r,dirs_exist_ok=True);(r/'change.patch').write_bytes((S/'fixtures'/base/'diff.patch').read_bytes());return r
cases=[]
r=fixture('python-parse','python_unsupported_syntax_limit');cases.append(('python-parse',r,{},[]))
r=fixture('rust-hard-limit','boundary_gap');cases.append(('rust-hard-limit',r,{'RIPR_MAX_DIFF_CHANGED_RUST_LINES':'1'},[]))
r=fixture('empty','boundary_gap');(r/'change.patch').write_text('');cases.append(('empty',r,{},[]))
r=fixture('garbage','boundary_gap');(r/'change.patch').write_text('this is not a unified diff\n');cases.append(('garbage',r,{},[]))
r=fixture('perl-unavailable','perl_cpan_alpha');(r/'ripr.toml').write_text('[languages]\nenabled = ["rust", "perl"]\n');cases.append(('perl-unavailable',r,{},[]))
r=fixture('rust-disabled','boundary_gap');(r/'ripr.toml').write_text('[languages]\nenabled = ["python"]\n');cases.append(('rust-disabled',r,{},[]))
r=fixture('render-bound','boundary_gap');(r/'src/lib.rs').write_text('pub fn record_mark(values: &mut Vec<u32>) {\n    values.push(1);\n}\n#[cfg(test)]\nmod tests {\nuse super::record_mark;\n#[test]\nfn case_00() {\nlet mut values = Vec::new();\nrecord_mark(&mut values);\nassert_eq!(values, vec![1]);\n}\n}\n');(r/'tests/pricing.rs').write_text('');(r/'change.patch').write_text('diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn record_mark(values: &mut Vec<u32>) {\n-    values.push(0);\n+    values.push(1);\n }\n');cases.append(('render-bound',r,{'RIPR_CHECK_FINDINGS_BYTES':'1'},[]));cases.append(('render-full',r,{'RIPR_CHECK_FINDINGS_BYTES':'0'},[]))
r=fixture('mixed-partial','boundary_gap');shutil.copytree(S/'fixtures/python_boundary_gap/input/src',r/'src',dirs_exist_ok=True);shutil.copytree(S/'fixtures/python_boundary_gap/input/tests',r/'tests',dirs_exist_ok=True);(r/'src/second.rs').write_text('pub fn second() -> u8 { 1 }\n');(r/'ripr.toml').write_text('[languages]\nenabled = ["rust", "python"]\n');(r/'change.patch').write_bytes((P/'cases/rust-partial/change.patch').read_bytes()+(S/'fixtures/python_boundary_gap/diff.patch').read_bytes());cases.append(('mixed-partial',r,{'RIPR_PARTIAL_DIFF_FILE_BUDGET':'1'},[]))
for name,r,env,extra in cases:
 for fmt in ['json','badge-json','human']:
  cmd=[B,'check','--root',str(r),'--diff',str(r/'change.patch'),'--mode','fast','--format',fmt,'--quiet']+extra
  o=subprocess.run(cmd,env=dict(os.environ,**env),capture_output=True,timeout=30);d=P/'results'/name;d.mkdir(parents=True,exist_ok=True);(d/(fmt+'.stdout')).write_bytes(o.stdout);(d/(fmt+'.stderr')).write_bytes(o.stderr)
  v=json.loads(o.stdout) if fmt!='human' else {};a=v.get('analysis_outcome') or {};kind=a.get('outcome',a).get('kind');rows.append({'case':name,'format':fmt,'exit':o.returncode,'argv':cmd,'env':env,'stdout_sha256':hashlib.sha256(o.stdout).hexdigest()});print(name,fmt,o.returncode,v.get('analysis_complete',a.get('analysis_complete')),kind,v.get('analysis_scope'),v.get('run_limitations'),v.get('message'))
(P/'boundary-receipt.json').write_text(json.dumps(rows,indent=2))
