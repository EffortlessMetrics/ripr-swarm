from pathlib import Path
import json,hashlib,re
p=Path('/workspace/5500-assertion-source');q=p/'crates/ripr/src/analysis/seam_cache.rs';s=q.read_text()
s=s.replace('pub(crate) const CACHE_SCHEMA_VERSION: &str = "1.84";', '/// `1.85`: source-visible resolved local-module glob assertion binding (#5500).\npub(crate) const CACHE_SCHEMA_VERSION: &str = "1.85";')
for name in ['SHARDED_CLASSIFIED_SEAM_CACHE_SCHEMA_VERSION','COMPACT_CLASSIFIED_SEAM_CACHE_SCHEMA_VERSION']:
 s=s.replace('const '+name+': &str = "0.90";', '/// `0.91`: same local-module macro binding as full `1.85`.\nconst '+name+': &str = "0.91";')
s=s.replace('assert_eq!(CACHE_SCHEMA_VERSION, "1.84")','assert_eq!(CACHE_SCHEMA_VERSION, "1.85")').replace('assert_eq!(SHARDED_CLASSIFIED_SEAM_CACHE_SCHEMA_VERSION, "0.90")','assert_eq!(SHARDED_CLASSIFIED_SEAM_CACHE_SCHEMA_VERSION, "0.91")').replace('assert_eq!(COMPACT_CLASSIFIED_SEAM_CACHE_SCHEMA_VERSION, "0.90")','assert_eq!(COMPACT_CLASSIFIED_SEAM_CACHE_SCHEMA_VERSION, "0.91")')
# Keep prior tests, add immediate predecessor copies for all three envelopes.
s=s.replace('("sharded_cache_schema_version", "0.88"),','("sharded_cache_schema_version", "0.88"),\n                            ("schema_version", if compact { "0.90" } else { "1.84" }),\n                            ("sharded_cache_schema_version", "0.90"),',1)
s=s.replace('("schema_version", if compact { "0.88" } else { "1.82" }),\n                        ]','("schema_version", if compact { "0.88" } else { "1.82" }),\n                            ("schema_version", if compact { "0.90" } else { "1.84" }),\n                        ]',1);q.write_text(s)
for rel in ['docs/OUTPUT_SCHEMA.md','xtask/src/tests/rust_corpus_projection.rs']:
 q=p/rel;q.write_text(q.read_text().replace('"1.84"','"1.85"'))
folder=p/'fixtures/boundary_gap/expected/first-useful-action/unchanged-after-attempt';hashes={}
for name in ['before.repo-exposure.json','after.repo-exposure.json']:
 q=folder/name;s=q.read_text().replace('"cache_schema_version": "1.84"','"cache_schema_version": "1.85"');m=re.search(r'"content_sha256": "(sha256:[a-f0-9]{64})"',s);assert m
 blank=s[:m.start(1)]+'sha256:'+'0'*64+s[m.end(1):];digest='sha256:'+hashlib.sha256(blank.encode()).hexdigest();s=s[:m.start(1)]+digest+s[m.end(1):];q.write_text(s);hashes[name]=dict(content=digest,raw='sha256:'+hashlib.sha256(s.encode()).hexdigest())
q=folder/'agent-verify.json';d=json.loads(q.read_text());d['inputs']['before_content_sha256']=hashes['before.repo-exposure.json']['content'];d['inputs']['after_content_sha256']=hashes['after.repo-exposure.json']['content'];q.write_text(json.dumps(d,indent=2,sort_keys=True)+'\n')
q=folder/'agent-receipt.json';d=json.loads(q.read_text())
for side in ['before','after']:d['provenance'][side+'_artifact']['sha256']=hashes[side+'.repo-exposure.json']['raw']
d['provenance']['verify_artifact']['sha256']='sha256:'+hashlib.sha256((folder/'agent-verify.json').read_bytes()).hexdigest();q.write_text(json.dumps(d,indent=2,sort_keys=True)+'\n')
Path('/workspace/5500-assertion-prerequisite/cache-artifact-receipt.json').write_text(json.dumps(hashes,indent=2)+'\n')
