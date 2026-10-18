from pathlib import Path
import json,hashlib,shutil,subprocess
P=Path('/workspace/scratch/8265-reconstruction')
receipt=json.loads((P/'candidate-cli-build.json').read_text()); assert receipt['exit']==0 and not receipt['abort']
rows=[]
for line in (P/'candidate-cli-build.stdout').read_text().splitlines():
    try: rows.append(json.loads(line))
    except ValueError: pass
a=[r for r in rows if r.get('reason')=='compiler-artifact' and r.get('executable') and r['target']['name']=='ripr' and r['target']['kind']==['bin']][-1]
src=Path(a['executable']); dst=P/'candidate-ripr'; assert not dst.exists()
baseline=json.loads((P/'baseline-binary.json').read_text())
assert a['profile']==baseline['artifact']['profile'] and a['features']==baseline['artifact']['features']
shutil.copy2(src,dst)
sha=lambda p:hashlib.file_digest(p.open('rb'),'sha256').hexdigest()
assert sha(src)==sha(dst)
version=subprocess.check_output([str(dst),'--version'],text=True).strip(); assert receipt['head'] in version
result={'source_receipt':receipt,'artifact':a,'version':version,'binary_sha256':sha(dst),'binary_bytes':dst.stat().st_size,'copy':str(dst),'baseline_profile_features_match':True}
(P/'candidate-binary.json').write_text(json.dumps(result,indent=2)+'\n'); print(version,sha(dst))
