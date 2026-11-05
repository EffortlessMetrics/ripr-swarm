from pathlib import Path
import subprocess,json,hashlib,os,time
repo=Path('/workspace/ripr-011-current-main');proof=Path('/workspace/011-integration-proof');binary=Path('/workspace/12010-proof/target/debug/ripr');expected='3d82fd6d0b9ae02b0f204c1d5fc84ebd2e4aa045'
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()==expected
assert not subprocess.check_output(['git','status','--porcelain'],cwd=repo,text=True).strip()
env=os.environ.copy();env.update(PATH='/workspace/011-integration-proof/qualification-bin:/workspace/.cargo/bin:'+env['PATH'],CARGO_HOME='/workspace/.cargo',RUSTUP_HOME='/workspace/.rustup')
rows=[]
def run(args):
 command=[str(binary),*args];start=time.monotonic();p=subprocess.run(command,cwd=repo,env=env,capture_output=True,timeout=180);row={'command':command,'exit':p.returncode,'seconds':round(time.monotonic()-start,3),'stdout_sha256':hashlib.sha256(p.stdout).hexdigest(),'stderr':p.stderr.decode(errors='replace')};rows.append(row)
 assert p.returncode==0,(args,p.returncode,row['stderr'],p.stdout.decode(errors='replace')[:1000]);return p.stdout
corpus=json.loads((repo/'fixtures/evidence-promotion-honesty-corpus/corpus.json').read_text());audit=json.loads((proof/'caf-d11f-json-composition-audit.json').read_text())
base=Path('fixtures/boundary_gap/expected/first-useful-action/unchanged-after-attempt');raw=run(['check','--root','fixtures/boundary_gap/input','--format','repo-exposure-json']);value=json.loads(raw);assert Path(value['artifact']['repository']['root'])==(repo/'fixtures/boundary_gap/input').resolve()
head='8a00693e680c8ad5172d5191bbb08484cbd5e300';value['artifact']['repository']['root']='.';value['artifact']['repository']['head']=head;value['artifact']['analysis']['worktree']='dirty';value['artifact']['snapshot_identity']='snapshot:'+value['artifact']['analysis']['input_identity']+';revision:'+head;value['artifact'].pop('generated_at',None)
for key in ['analyzer_version','toolchain_hash','workspace_manifests_hash']:assert key in value['analysis_inputs'];value['analysis_inputs'][key]='normalized-for-portable-fixture'
placeholder='sha256:'+'0'*64;value['artifact']['content_sha256']=placeholder;rendered=json.dumps(value,indent=2,sort_keys=True,ensure_ascii=False)+'\n';digest='sha256:'+hashlib.sha256(rendered.encode()).hexdigest();rendered=rendered.replace(placeholder,digest)
assert value['seams'][0]['seam_id']=='67fc764ba37d77bd';assert value['seams'][0]['grip_class']=='weakly_gripped'
for name in ['before.repo-exposure.json','after.repo-exposure.json']:(repo/base/name).write_text(rendered)
verify=run(['agent','verify','--root','.','--before',str(base/'before.repo-exposure.json'),'--after',str(base/'after.repo-exposure.json'),'--json']);verified=json.loads(verify);assert verified['summary']['unchanged']==1 and not verified['changed_seams'];(repo/base/'agent-verify.json').write_bytes(verify)
receipt=run(['agent','receipt','--root','.','--verify-json',str(base/'agent-verify.json'),'--seam-id','67fc764ba37d77bd','--json']);value=json.loads(receipt);assert value['seam']['change']=='unchanged';assert value['seam']['after']=='weakly_gripped'
provenance=value['provenance'];provenance['generated_at']='<generated_at>';provenance['ripr_version']='<ripr_version>'
for field in ['before_artifact','after_artifact','verify_artifact']:
 artifact=provenance[field];assert artifact['sha256']=='sha256:'+hashlib.sha256((repo/artifact['path']).read_bytes()).hexdigest(),artifact
if isinstance(value.get('analysis_outcome_error'),str):
 diagnostic=value['analysis_outcome_error'].strip().rstrip('.')
 if diagnostic:
  for parent,key in [(value.get('summary',{}),'next_recommendation'),(value.get('summary',{}).get('next_action',{}),'recommended_action')]:
   if isinstance(parent.get(key),str):parent[key]=parent[key].replace(diagnostic,'<analysis_outcome_error>')
 value['analysis_outcome_error']='<analysis_outcome_error>'
if 'analysis_outcome' in value:value['analysis_outcome']=None
(repo/base/'agent-receipt.json').write_text(json.dumps(value,indent=2,sort_keys=True,ensure_ascii=False)+'\n')
manifest={'source_candidate':expected,'producer_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'producer_version':subprocess.check_output([str(binary),'--version'],text=True).strip(),'commands':rows,'normalization':'Only portable boundary fixture root/revision/worktree, generated_at, three declared machine identity values and derived content commitment; receipt volatile identity/diagnostic/outcome normalized with actual artifact digests retained. Raw honesty reports are unmodified producer bytes.','result':'four boundary chain artifacts refreshed;21 raw report captures retained from6c producer; expected corpus assertions unchanged; qualification still pending'};(proof/'importer-portable-fixture-regeneration-receipt.json').write_text(json.dumps(manifest,indent=2)+'\n');print(json.dumps({'commands':len(rows),'result':manifest['result']}),flush=True)
