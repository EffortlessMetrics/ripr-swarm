# Independent review experiment

```python
from pathlib import Path
import subprocess,os,json,hashlib
p=Path('/workspace/8201-evidence/exclusion94/parent-refusal');first=p/'first';second=p/'second';selected=p/'selected';selected.unlink();selected.symlink_to(first,target_is_directory=True)
def inventory():return {str(f.relative_to(second/'result')):hashlib.sha256(f.read_bytes()).hexdigest() for f in (second/'result').rglob('*') if f.is_file()}
before=inventory();assert before
binary=Path('/workspace/ripr-swarm/target/debug/ripr');assert binary.exists()
env=os.environ.copy();env.update(CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup',PATH='/workspace/.cargo/bin:'+env['PATH'])
args=[str(binary),'check','--root',str(p/'root'),'--diff',str(p/'input.diff'),'--output-dir',str(selected/'result')]
proc=subprocess.Popen(args,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE);lines=[];trigger=False
for line in iter(proc.stderr.readline,b''):
 lines.append(line)
 if b'ripr progress: analyzing ' in line and not trigger:selected.unlink();selected.symlink_to(second,target_is_directory=True);trigger=True
stdout=proc.stdout.read();proc.wait(timeout=30);stderr=b''.join(lines);assert trigger and proc.returncode==2 and inventory()==before
(p/'existing.stdout').write_bytes(stdout);(p/'existing.stderr').write_bytes(stderr);(p/'existing-receipt.json').write_text(json.dumps(dict(args=args,version=subprocess.check_output([str(binary),'--version'],env=env,text=True).strip(),exit=proc.returncode,trigger=trigger,existing_destination_preserved=True,files=before),indent=2)+'\n');print('existing retargeted destination preserved')

```
