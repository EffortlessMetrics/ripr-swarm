import pathlib,subprocess,os,json,hashlib,datetime
P=pathlib.Path('/workspace/scratch/8265-reconstruction');R=pathlib.Path('/workspace/macro-site-parity');B='006eaf853fce5713a2e9044ba9c8d6a9a873f3d0';H='8a8ade0fdeda1eee0972c3e53900f826ac84df48';assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()==H
(P/'test-source.patch').write_bytes(subprocess.check_output(['git','diff',B,H],cwd=R));(P/'checkpoint-identity.json').write_text(json.dumps({'head':H,'base':B,'reconstruction':True,'stage':'fresh test-only checkpoint; successful RED6fail/2pass; proposed repair scripts not yet executed','old_candidate_unrecoverable':'fc18360e9a50eb94b194fa38ba2f89e34d19fa82','timestamp':datetime.datetime.now(datetime.timezone.utc).isoformat()},indent=2)+'\n')
files=[]
for p in P.rglob('*'):
 if not p.is_file():continue
 rel=p.relative_to(P)
 if any(x in ['target','__pycache__'] for x in rel.parts) or p.suffix in ['.gz','.pyc','.index']:continue
 if p.name.startswith('baseline-cli-build') or p.name.startswith('checkpoint-evidence-') or p.name=='checkpoint-manifest.json':continue
 files.append(str(rel))
m={f:{'bytes':(P/f).stat().st_size,'sha256':hashlib.file_digest((P/f).open('rb'),'sha256').hexdigest()} for f in sorted(files)};(P/'checkpoint-manifest.json').write_text(json.dumps(m,indent=2)+'\n');idx=P/'checkpoint-evidence.index';assert not idx.exists();env={**os.environ,'GIT_INDEX_FILE':str(idx)}
def g(*args,input=None):return subprocess.check_output(['git',*args],cwd=R,env=env,input=input,text=input is None).strip()
g('read-tree','--empty')
for f in [*sorted(files),'checkpoint-manifest.json']:
 blob=g('hash-object','-w',str(P/f));g('update-index','--add','--cacheinfo','100644',blob,f)
tree=g('write-tree');c=g('commit-tree',tree,input=b'8265 reconstruction first recoverable RED and authoring checkpoint\n').decode();(P/'checkpoint-evidence-commit.txt').write_text(c+'\n');print(json.dumps({'commit':c,'tree':tree,'payloads':len(m),'manifest_sha256':hashlib.file_digest((P/'checkpoint-manifest.json').open('rb'),'sha256').hexdigest()}))
