import pathlib,subprocess,json,hashlib,shutil
src=pathlib.Path('/workspace/5500-veto-reconciliation');repo=pathlib.Path('/workspace/5500-evidence-publish');previous='4cafbc5255252bc1a7d8e9c3c3f0c34713a2552e';assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()==previous;assert not subprocess.check_output(['git','status','--porcelain'],cwd=repo,text=True);entries=[]
for f in sorted(src.rglob('*')):
 if not f.is_file():continue
 rel=f.relative_to(src)
 if any(x in ('cache','target','.git','__pycache__') for x in rel.parts[:-1]):continue
 if f.suffix not in ('.md','.txt','.json','.py','.rs','.toml','.diff','.log','.stderr'):continue
 dest=repo/'veto-reconciliation'/rel;dest.parent.mkdir(parents=True,exist_ok=True);b=f.read_bytes();b.decode('utf-8');shutil.copy2(f,dest);entries.append({'path':str(dest.relative_to(repo)),'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()})
r={'frozen_source_head':'c4a8f7828e15426c4d872718f66bd4bac3ab0daf','source_tree':'58e19fdafaa9a83509a50c8402b13211a34eefdc','prior_evidence_commit':previous,'claim':'Proof-onlydecisiveprerequisite reconciliation; nosourcepatch/builtornewconsumerreplay. Exactc4consumer0 retainedwith107sourceidentitycheck; all52unchanged. PrimitiveScalarIdentityactualconsumeredgeunprovedunder no-broad-serde. Root#5500ownsresidual; no rivalpatch.','files':entries};manifest=repo/'veto-reconciliation-manifest.json';manifest.write_text(json.dumps(r,indent=2)+'\n')
for x in entries:
 b=(repo/x['path']).read_bytes();assert len(b)==x['bytes'] and hashlib.sha256(b).hexdigest()==x['sha256']
print(json.dumps({'files':len(entries),'bytes':sum(x['bytes'] for x in entries),'manifest_sha256':hashlib.sha256(manifest.read_bytes()).hexdigest()}))
