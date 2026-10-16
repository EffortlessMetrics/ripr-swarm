# Independent review experiment

```python
from pathlib import Path
import subprocess,os,json,hashlib
q=Path('/workspace/8201-evidence/repair490');p=q/('excluded-native-'+os.environ.get('EXCLUSION','git'));p.mkdir();r=p/'root';(r/'src').mkdir(parents=True);excluded='.git' if os.environ.get('EXCLUSION','git')=='git' else 'target';(r/excluded).mkdir()
(r/'Cargo.toml').write_text('[package]\nname="excluded-alias-review"\nversion="0.0.0"\nedition="2024"\n[workspace]\n')
(r/'ripr.toml').write_text('[analysis]\nmode="ready"\n[languages]\nenabled=["rust"]\n')
(r/'src/lib.rs').write_text('include!("../'+excluded+'/linked.rs");\n')
(r/'src/impl.rs').write_text('pub fn positive(v:i32)->bool { v > 0 }\n')
(r/excluded/'linked.rs').symlink_to(r/'src/impl.rs')
(p/'input.diff').write_text('--- a/src/impl.rs\n+++ b/src/impl.rs\n@@ -1 +1 @@\n-pub fn positive(v:i32)->bool { v >= 0 }\n+pub fn positive(v:i32)->bool { v > 0 }\n')
env=os.environ.copy();env.update(CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',PATH='/workspace/.cargo/bin:'+env['PATH'])
rows=[]
for label,binary in [('original6a','/workspace/8201-evidence/ripr'),('repair490','/workspace/ripr-swarm/target/debug/ripr')]:
 cmd=[binary,'check','--root',str(r),'--diff',str(p/'input.diff'),'--quiet','--output-dir',str(p/label)]
 result=subprocess.run(cmd,env=env,capture_output=True,timeout=30);(p/(label+'.stdout')).write_bytes(result.stdout);(p/(label+'.stderr')).write_bytes(result.stderr)
 row=dict(label=label,binary_sha256=hashlib.sha256(Path(binary).read_bytes()).hexdigest(),version=subprocess.check_output([binary,'--version'],env=env,text=True).strip(),args=cmd,exit=result.returncode,stderr=result.stderr.decode())
 if (p/label/'manifest.json').exists():
  m=json.loads((p/label/'manifest.json').read_text());row.update(complete_set=m['complete_set'],command_success=m['command_success'])
 rows.append(row)
(p/'receipt.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows,indent=2))

```
