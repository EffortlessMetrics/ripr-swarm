from pathlib import Path
import subprocess,json,hashlib,datetime
P=Path('/workspace/scratch/8265-reconstruction');identity=json.loads((P/'final-evidence-identity.json').read_text());commit=identity['commit'];bare=P/'remote-readback.git';assert not bare.exists()
subprocess.run(['git','init','--bare',str(bare)],capture_output=True,check=True)
def git(*args):return subprocess.check_output(['git','--git-dir='+str(bare),*args])
subprocess.run(['git','--git-dir='+str(bare),'fetch','--no-tags','--depth=1','https://github.com/EffortlessMetrics/ripr-swarm.git','refs/heads/codex/8265-macro-site-parity-evidence'],capture_output=True,check=True)
assert git('rev-parse','FETCH_HEAD').decode().strip()==commit
manifest_bytes=git('show',commit+':manifest.json');manifest=json.loads(manifest_bytes);assert hashlib.sha256(manifest_bytes).hexdigest()==identity['manifest_sha256']
files=git('ls-tree','-rz','--name-only',commit).decode().split('\0');files=[f for f in files if f];assert sorted(files)==sorted([*manifest['payloads'],'manifest.json'])
for path,row in manifest['payloads'].items():
    data=git('show',commit+':'+path);assert len(data)==row['bytes'] and hashlib.sha256(data).hexdigest()==row['sha256'],path
result={'commit':commit,'tree':git('rev-parse',commit+'^{tree}').decode().strip(),'manifest_sha256':hashlib.sha256(manifest_bytes).hexdigest(),'payloads_verified':len(manifest['payloads']),'complete_inventory':True,'method':'fresh isolated bare Git repository fetched remote evidence branch; every blob byte/size/digest and entire tree inventory verified','utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
(P/'remote-readback.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
