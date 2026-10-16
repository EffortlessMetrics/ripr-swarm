# Independent review experiment

```python
from pathlib import Path
import subprocess,os,json,hashlib
p=Path('/workspace/8201-evidence/exclusion94/parent-refusal');p.mkdir();r=p/'root';(r/'src').mkdir(parents=True)
(r/'Cargo.toml').write_text('[package]\nname="parent-refusal-review"\nversion="0.0.0"\nedition="2024"\n[workspace]\n');(r/'ripr.toml').write_text('[analysis]\nmode="ready"\n[languages]\nenabled=["rust"]\n')
lib='';diff=''
for i in range(16):
 lib+=f'pub mod m{i};\n';s=f'pub fn positive(v:i32)->bool {{ v > {i} }}\n';(r/f'src/m{i}.rs').write_text(s);diff+=f'--- a/src/m{i}.rs\n+++ b/src/m{i}.rs\n@@ -1 +1 @@\n-'+s.replace(' > ',' >= ')+'+'+s
(r/'src/lib.rs').write_text(lib);(p/'input.diff').write_text(diff)
first=p/'first';second=p/'second';first.mkdir();second.mkdir();selected=p/'selected';selected.symlink_to(first,target_is_directory=True)
env=os.environ.copy();env.update(CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',PATH='/workspace/.cargo/bin:'+env['PATH']);binary='/workspace/ripr-swarm/target/debug/ripr'
args=[binary,'check','--root',str(r),'--diff',str(p/'input.diff'),'--output-dir',str(selected/'result')]
proc=subprocess.Popen(args,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE);lines=[];trigger=False
for line in iter(proc.stderr.readline,b''):
 lines.append(line)
 if b'ripr progress: analyzing ' in line and not trigger:
  selected.unlink();selected.symlink_to(second,target_is_directory=True);trigger=True
stdout=proc.stdout.read();proc.wait(timeout=30);stderr=b''.join(lines);(p/'stdout').write_bytes(stdout);(p/'stderr').write_bytes(stderr)
assert trigger and proc.returncode==2 and b'destination parent changed' in stderr
assert not (first/'result').exists();manifest=json.loads((second/'result/manifest.json').read_bytes());assert not manifest['complete_set'] and not manifest['command_success']
for member in manifest['members']:
 b=(second/'result'/member['file']).read_bytes();assert len(b)==member['bytes'] and hashlib.sha256(b).hexdigest()==member['sha256']
row=dict(args=args,version=subprocess.check_output([binary,'--version'],env=env,text=True).strip(),exit=proc.returncode,trigger=trigger,first_destination_exists=False,retargeted_destination_has_refusal=True,complete_set=False,command_success=False,stdout_bytes=len(stdout),member_hashes_verified=True)
(p/'receipt.json').write_text(json.dumps(row,indent=2)+'\n');print(json.dumps(row,indent=2))

```
