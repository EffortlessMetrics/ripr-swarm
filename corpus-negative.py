from pathlib import Path
import json,subprocess,hashlib
P=Path('/workspace/scratch/8265-reconstruction');R=Path('/workspace/macro-site-parity');F=R/'fixtures/macro_site_scoped_rest_limit/expected'
assert json.loads((P/'candidate-cli-build.json').read_text())['exit']==0
files={name:(F/name).read_bytes() for name in ['check.json','human.txt','human-full.txt']}
sha=lambda b:hashlib.sha256(b).hexdigest()
def run(label):
    return subprocess.run(['python',str(P/'run.py'),label,'/workspace/ripr-swarm/target/debug/xtask','check-evidence-promotion-honesty'],cwd=R).returncode
try:
    o=json.loads(files['check.json']);assert len(o['findings'])==2
    for finding in o['findings']:finding['classification']='reachable_unrevealed'
    o['summary']['reachable_unrevealed']=2;o['summary']['static_unknown']=0
    (F/'check.json').write_text(json.dumps(o,indent=2)+'\n')
    for name in ['human.txt','human-full.txt']:(F/name).write_bytes(files[name].replace(b'static_unknown',b'reachable_unrevealed'))
    bad={name:sha((F/name).read_bytes()) for name in files}
    (P/'corpus-negative.patch').write_bytes(subprocess.check_output(['git','diff','--binary'],cwd=R))
    for name in files:(P/('wrong-'+name)).write_bytes((F/name).read_bytes())
    code=run('corpus-wrong')
    stdout=(P/'corpus-wrong.stdout').read_text();stderr=(P/'corpus-wrong.stderr').read_text()
    assert code!=0 and 'rust_macro_site_scoped_rest_limit' in stdout+stderr and 'expected_class' in stdout+stderr
finally:
    for name,data in files.items():(F/name).write_bytes(data)
assert run('corpus-restored')==0
assert not subprocess.check_output(['git','status','--porcelain'],cwd=R)
native=Path('/workspace/ripr-swarm/target/debug/xtask')
result={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip(),'driver_sha256':sha(native.read_bytes()),'wrong_sha256':bad,'good_sha256':{name:sha(data) for name,data in files.items()},'wrong_exit':code,'restored_exit':0,'wrong_case':'coherent two-finding reachable_unrevealed classification/summary/human mutation rejected by unchanged expected_class static_unknown corpus assertion'}
(P/'corpus-negative.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
