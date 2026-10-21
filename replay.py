import pathlib, subprocess, json, hashlib, sys
root=pathlib.Path('/workspace/8830-proof')
repo=root/'consumer'
def run(args,cwd=repo):
 p=subprocess.run(args,cwd=cwd,capture_output=True,text=True)
 if p.returncode: raise RuntimeError((args,p.returncode,p.stderr))
 return p.stdout.strip()
if not repo.exists():
 (repo/'src').mkdir(parents=True)
 run(['git','init','-q'])
 run(['git','config','user.name','Badge proof'])
 run(['git','config','user.email','badge-proof@example.invalid'])
 (repo/'Cargo.toml').write_text('[package]\nname="badge-proof"\nversion="0.1.0"\nedition="2024"\n')
 (repo/'src/lib.rs').write_text('pub fn accepts(n: i32) -> bool {\n    n > 2\n}\n\n#[test]\nfn boundary() {\n    assert!(!accepts(2));\n    assert!(accepts(3));\n}\n')
 (repo/'README.md').write_text('Badge proof\n')
 (repo/'.gitignore').write_text('target/\n')
 run(['git','add','.']);run(['git','commit','-qm','base'])
 (root/'base.txt').write_text(run(['git','rev-parse','HEAD']))
base=(root/'base.txt').read_text().strip()
binary=sys.argv[1]; label=sys.argv[2]
out=root/label;out.mkdir(exist_ok=True)
results=[]
source=(repo/'src/lib.rs').read_text()
for case in ['empty','readme','comment','predicate','mode','malformed']:
 run(['git','restore','.'])
 if case=='readme': (repo/'README.md').write_text('Badge proof, edited\n')
 if case=='comment': (repo/'src/lib.rs').write_text('// explanation\n'+source)
 if case=='predicate': (repo/'src/lib.rs').write_text(source.replace('n > 2','n >= 2'))
 if case=='mode': (repo/'src/lib.rs').chmod(0o755)
 patch=root/(case+'.diff')
 patch.write_text(run(['git','diff','--no-ext-diff',base])+'\n' if case!='empty' else '')
 if case=='malformed':patch.write_text('--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,1 +1,2 @@\n+broken\n')
 for fmt in ['badge-json','badge-shields','json','human','human-full','sarif','github']:
  args=[binary,'check','--root',str(repo),'--diff',str(patch),'--format',fmt]
  p=subprocess.run(args,cwd=repo,capture_output=True,text=True)
  stem=out/(case+'-'+fmt)
  stem.with_suffix('.out').write_text(p.stdout);stem.with_suffix('.err').write_text(p.stderr)
  row=dict(case=case,format=fmt,exit=p.returncode,command=args,stdout_sha256=hashlib.sha256(p.stdout.encode()).hexdigest())
  if fmt=='badge-json':
   j=json.loads(p.stdout);row.update(status=j['status'],color=j['color'],message=j['message'],outcome=j.get('analysis_outcome'),complete=j.get('analysis_complete'))
  results.append(row)
 run(['git','restore','.'])
(out/'receipt.json').write_text(json.dumps(results,indent=2)+'\n')
for row in results:
 if row['format']=='badge-json':print(json.dumps(row))
