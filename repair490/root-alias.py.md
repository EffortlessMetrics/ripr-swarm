# Independent review experiment

```python
from pathlib import Path
import subprocess,os,json,hashlib
p=Path('/workspace/8201-evidence/repair490/root-alias');p.mkdir();r=p/'physical';(r/'src').mkdir(parents=True)
(r/'Cargo.toml').write_text('[package]\nname="alias-fixed-review"\nversion="0.0.0"\nedition="2024"\n[workspace]\n');(r/'ripr.toml').write_text('[languages]\nenabled=["rust"]\n');(r/'src/lib.rs').write_text('pub fn positive(v:i32)->bool { v > 0 }\n');(p/'input.diff').write_text('--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-pub fn positive(v:i32)->bool { v >= 0 }\n+pub fn positive(v:i32)->bool { v > 0 }\n');alias=p/'alias';alias.symlink_to(r,target_is_directory=True)
env=os.environ.copy();env.update(CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',PATH='/workspace/.cargo/bin:'+env['PATH']);binary='/workspace/ripr-swarm/target/debug/ripr';rows=[]
def run(label,root,args):
 cmd=[binary,'check','--root',str(root),'--diff',str(p/'input.diff'),'--quiet']+args;o=subprocess.run(cmd,env=env,capture_output=True,timeout=30);(p/(label+'.stdout')).write_bytes(o.stdout);(p/(label+'.stderr')).write_bytes(o.stderr);rows.append(dict(label=label,args=cmd,exit=o.returncode,stderr=o.stderr.decode()));return o
for label,root in [('physical',r),('alias',alias)]:
 out=run(label,root,['--output-dir',str(p/(label+'-output'))]);assert out.returncode==0,out.stderr
 m=json.loads((p/(label+'-output')/'manifest.json').read_text());assert m['complete_set'] and m['command_success']
 for x in m['members']:
  b=(p/(label+'-output')/x['file']).read_bytes();assert len(b)==x['bytes'] and hashlib.sha256(b).hexdigest()==x['sha256']
for fmt,file in [('badge-json','badge.json'),('json','detail.json'),('human','human.txt')]:
 o=run('native-'+fmt,alias,['--format',fmt]);assert o.returncode==0 and o.stdout==(p/'alias-output'/file).read_bytes()
(p/'outside.rs').write_text('pub fn escaped() {}');(r/'src/link.rs').symlink_to(p/'outside.rs');o=run('inroot-symlink',alias,['--output-dir',str(p/'inroot-symlink-output')]);assert o.returncode==2 and b'regular input files' in o.stderr
m=json.loads((p/'inroot-symlink-output/manifest.json').read_text());assert not m['complete_set'] and not m['command_success']
(p/'receipt.json').write_text(json.dumps(dict(status='passed',binary_sha256=hashlib.sha256(Path(binary).read_bytes()).hexdigest(),version=subprocess.check_output([binary,'--version'],env=env,text=True).strip(),records=rows),indent=2)+'\n');print('root-alias repair: six processes passed')

```
