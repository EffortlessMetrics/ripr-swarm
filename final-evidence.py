from pathlib import Path
import subprocess,os,json,hashlib,datetime
P=Path('/workspace/scratch/8265-reconstruction');R=Path('/workspace/macro-site-parity')
parent='5e94663edd90e21d91e7365597a38028a9605148';head='9fc6a47f29b5ad581a527244681d4c4bf0d4714b';branch='refs/heads/codex/8265-macro-site-parity-evidence'
def git(*args,input=None,env=None,cwd=R):
    return subprocess.check_output(['git',*args],cwd=cwd,input=input,env=env).strip()
assert git('rev-parse','HEAD').decode()==head
assert not git('status','--porcelain')
assert git('ls-remote','origin',branch).decode().split()[0]==parent
for label in ['check-fast-r2','precommit-r2','corpus-tests','fixture-final','final-cli-build']:
    r=json.loads((P/(label+'.json')).read_text()); assert r['exit']==0 and r['abort'] is None,(label,r)
files=[]
for f in P.rglob('*'):
    if not f.is_file():continue
    rel=f.relative_to(P)
    if any(x.startswith('cache-') or x in ['__pycache__','remote-readback.git'] for x in rel.parts):continue
    if f.name in ['baseline-ripr','candidate-ripr','manifest.json','pr-body-draft.md','pr-body.md'] or f.suffix in ['.gz','.index','.pyc']:continue
    if f.name.startswith('final-evidence-') or f.name.startswith('remote-readback'):continue
    files.append(str(rel))
sha=lambda b:hashlib.sha256(b).hexdigest()
manifest={'head':head,'tree':git('rev-parse','HEAD^{tree}').decode(),'basis':'006eaf853fce5713a2e9044ba9c8d6a9a873f3d0','reconstruction':True,'old_candidate_lost':'fc18360e9a50eb94b194fa38ba2f89e34d19fa82','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'native_binaries_and_archives':'locally retained, excluded from this packet; sources/raw receipts/inputs recoverable','payloads':{name:{'bytes':(P/name).stat().st_size,'sha256':sha((P/name).read_bytes())} for name in sorted(files)}}
(P/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
idx=P/'final-evidence.index';assert not idx.exists();env={**os.environ,'GIT_INDEX_FILE':str(idx)}
git('read-tree','--empty',env=env)
for name in [*sorted(files),'manifest.json']:
    blob=git('hash-object','-w',str(P/name),env=env).decode();git('update-index','--add','--cacheinfo','100644',blob,name,env=env)
tree=git('write-tree',env=env).decode();commit=git('commit-tree',tree,'-p',parent,input=b'8265 reconstructed candidate fresh proof and independent review\n',env=env).decode()
result={'commit':commit,'tree':tree,'parent':parent,'payloads':len(files),'payload_bytes':sum(v['bytes'] for v in manifest['payloads'].values()),'manifest_sha256':sha((P/'manifest.json').read_bytes())};(P/'final-evidence-identity.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
