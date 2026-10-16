import pathlib,os,subprocess,json,hashlib,stat,time
P=pathlib.Path('/workspace/scratch/module-scope-walk');R=pathlib.Path('/workspace/module-scope-walk');head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip();tree=subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=R,text=True).strip();assert head=='8ac12c026f976489630a767185be756b4fdcecb2';assert tree=='b3b410906719b57307dee00f38d42bde01db03af';assert not subprocess.check_output(['git','status','--porcelain'],cwd=R)
identity={'head':head,'tree':tree,'base':'3477bf95a9ead7f35ec099a4e6cf17af96708003','branch':'codex/module-scope-walk-reuse','author_status':'local proof complete; independent review/hosted qualification pending','timestamp_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),'claim':'one-pass module scope collection; exact behavior preserved in exercised controls; no end-to-end speedup established','resources':'source and distinct frozen executables/test witnesses retained for review; no cleanup or release'};(P/'identity.json').write_text(json.dumps(identity,indent=2)+'\n')
excluded={'baseline-ripr','candidate-ripr','test-first-green-tests','wrong-cfg-red-tests','candidate-parser-tests-tests','collector-micro','workload.rmeta','pr-body-draft.md','pr-body.md','manifest.json','evidence-commit.txt'}
files=[]
for p in P.rglob('*'):
 if not p.is_file():continue
 rel=p.relative_to(P)
 if any(x.startswith(('bench-cache-','cache-','__pycache__')) or x=='cache' for x in rel.parts):continue
 if p.name in excluded or p.suffix in ['.index','.pyc','.rmeta']:continue
 files.append(str(rel))
files=sorted(set(files));manifest={f:{'bytes':(P/f).stat().st_size,'sha256':hashlib.file_digest((P/f).open('rb'),'sha256').hexdigest()} for f in files};(P/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');idx=P/'evidence.index';assert not idx.exists();env={**os.environ,'GIT_INDEX_FILE':str(idx)}
def git(*a,input=None):return subprocess.check_output(['git',*a],cwd=R,env=env,input=input,text=input is None).strip()
git('read-tree','--empty')
for f in [*files,'manifest.json']:
 blob=git('hash-object','-w',str(P/f));git('update-index','--add','--cacheinfo','100644',blob,f)
etree=git('write-tree');commit=git('commit-tree',etree,input=('Exact-head module scope traversal author evidence for '+head+'\n').encode()).decode();(P/'evidence-commit.txt').write_text(commit+'\n');print(json.dumps({'commit':commit,'tree':etree,'payloads':len(files),'total_bytes':sum(x['bytes'] for x in manifest.values()),'manifest_sha256':hashlib.file_digest((P/'manifest.json').open('rb'),'sha256').hexdigest()},indent=2))
