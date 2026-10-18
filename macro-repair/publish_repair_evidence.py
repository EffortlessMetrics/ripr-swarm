"""Append allowlisted repair evidence; preserve historical immutable custody."""
from pathlib import Path
import hashlib,json,shutil,subprocess
p=Path('/workspace/5500-evidence-publish');entries=[]
assert hashlib.sha256((p/'manifest.json').read_bytes()).hexdigest()=='f6030e39be1687d22ea8a9736c3818b408fbb12c7707a83b1c2809ed8d301dd8'
allowed={'.json','.txt','.md','.log','.stderr','.rs','.toml','.diff','.patch','.py'}
for label,src in [('macro-repair',Path('/workspace/5500-macro-repair')),('macro-independent',Path('/workspace/5500-macro-review'))]:
 for f in sorted(src.rglob('*')):
  rel=f.relative_to(src)
  if any(x.startswith('cache') or x in {'.git','target','__pycache__','.ripr'} or x.endswith('-target') for x in rel.parts[:-1]): continue
  if not f.is_file() or f.suffix not in allowed or f.name in {'canonical-final-draft.md','coordination-final-draft.md'}:continue
  dest=p/label/rel;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(f,dest);data=dest.read_bytes();entries.append(dict(path=str(dest.relative_to(p)),bytes=len(data),sha256=hashlib.sha256(data).hexdigest()))
head=subprocess.check_output(['git','-C','/workspace/ripr-swarm','rev-parse','HEAD'],text=True).strip();assert head=='656d6d7effb51a888c4b627e2a0cd44c3bbd37ef'
manifest=dict(schema='ripr-5500-macro-repair-evidence-1',source_head=head,source_tree='998fd59f62e93a71cd8e20d3b1080f48bd1257a7',consumer_head='278253a03b00d1aa777eeba7633954122163890f',consumer_base='715a36434a56be3a9abeb5545012d56e2cb9cb60',historical_evidence_commit='690975cff31ae56c7d8e28f93aa308c85d497bc4',historical_manifest_sha256='f6030e39be1687d22ea8a9736c3818b408fbb12c7707a83b1c2809ed8d301dd8',entries=entries,retention='All text copied; binaries and mutable caches/targets/labs retained locally. No destructive cleanup.',claim_boundary='Exact local source proof and unchanged consumer inventory. No installed/hosted/Ready/merge/release claim.')
(p/'macro-repair-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');shutil.copy2('/workspace/5500-macro-repair/REPAIR-SUMMARY.md',p/'MACRO-REPAIR-SUMMARY.md')
print(json.dumps(dict(files=len(entries),bytes=sum(x['bytes'] for x in entries),manifest_sha256=hashlib.sha256((p/'macro-repair-manifest.json').read_bytes()).hexdigest()),indent=2))
