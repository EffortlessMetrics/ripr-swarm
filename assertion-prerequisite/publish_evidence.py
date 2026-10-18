import pathlib,json,hashlib,shutil,subprocess
src=pathlib.Path('/workspace/5500-assertion-prerequisite');repo=pathlib.Path('/workspace/5500-evidence-publish');review=pathlib.Path('/workspace/5500-macro-review');allowed={'.json','.txt','.md','.log','.stderr','.rs','.toml','.diff','.patch','.py'}
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()=='a3183503470d8db8afcb4045cd707ec79acb42d6'
assert not subprocess.check_output(['git','status','--porcelain'],cwd=repo,text=True)
entries=[]
def copy(f,d):
 d.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(f,d);b=d.read_bytes();b.decode('utf-8');entries.append({'path':str(d.relative_to(repo)),'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()})
for f in sorted(src.rglob('*')):
 if not f.is_file():continue
 rel=f.relative_to(src)
 if any(x.startswith('cache') or x in ('target','.git','__pycache__') for x in rel.parts[:-1]):continue
 if f.suffix not in allowed and f.name!='Cargo.lock':continue
 copy(f,repo/'assertion-prerequisite'/rel)
for f in sorted(review.rglob('*')):
 if not f.is_file():continue
 rel=f.relative_to(review)
 if any(x.startswith('cache') or x in ('target','.git','__pycache__') for x in rel.parts[:-1]):continue
 if f.suffix not in allowed:continue
 if not ('24a' in str(rel) or 'c4a' in str(rel) or f.name.startswith('check-')):continue
 copy(f,repo/'assertion-independent'/rel)
manifest={'source_head':'c4a8f7828e15426c4d872718f66bd4bac3ab0daf','source_tree':'58e19fdafaa9a83509a50c8402b13211a34eefdc','frozen_producer_head':'656d6d7effb51a888c4b627e2a0cd44c3bbd37ef','prior_evidence_commit':'a3183503470d8db8afcb4045cd707ec79acb42d6','original_source_custody':'16ff7479d2a78fd9558b4ff9f5ad6c803c2857d7 independent complete validation issuecomment6248595293 remains untouched','scope':'Append-only prerequisite text witnesses. Binary identities retained by SHA256; executable/rlib/so/targets/caches excluded. Failed/setup/oracle/checkpoint logs preserved with explicit scope; implement.py is historical authoring scratch and must not be replayed against final source.','files':entries}
(repo/'assertion-prerequisite-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
for x in entries:
 b=(repo/x['path']).read_bytes();assert len(b)==x['bytes'] and hashlib.sha256(b).hexdigest()==x['sha256']
print(json.dumps({'files':len(entries),'bytes':sum(x['bytes'] for x in entries),'manifest_sha256':hashlib.sha256((repo/'assertion-prerequisite-manifest.json').read_bytes()).hexdigest()}))
