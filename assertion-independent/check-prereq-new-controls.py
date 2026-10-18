from pathlib import Path
import json,os,subprocess,hashlib
lab=Path('/workspace/5500-macro-review');base=Path('/workspace/5500-assertion-prerequisite');binary=base/'ripr-5b097c8d'
assert hashlib.sha256(binary.read_bytes()).hexdigest()=='dbc4b5701c5d269c471d53d5467e2749f971a8d154cca4bf8427a5d02b6459e6'
assert '5b097c8d18285c2c1511800b1451e9a58734b8cc' in subprocess.check_output([str(binary),'--version'],text=True)
custom=lab/'prereq-custom-noop';(custom/'src').mkdir(parents=True,exist_ok=True)
for name in ['lib.rs','proof.rs']:(custom/'src'/name).write_bytes((base/'native/custom/original/src'/name).read_bytes())
(custom/'Cargo.toml').write_text('[package]\nname="prereq_custom_noop"\nversion="0.0.0"\nedition="2024"\n[workspace]\n')
(custom/'input.diff').write_bytes((base/'return-local/input.diff').read_bytes())
cases=[('return-local',base/'return-local','return_value','600','exposed'),('return-qualified',base/'return-qualified','return_value','600','exposed')]
cases += [('boundary-'+x.name,x,'return_value','600','static_unknown') for x in sorted((base/'boundaries').iterdir())]
cases += [('whole-local',base/'local','match_arm',None,'weakly_exposed'),('whole-qualified',base/'qualified','match_arm',None,'exposed'),('custom-noop',custom,'return_value','600','reachable_unrevealed')]
rs=[]
for name,root,family,expr,want in cases:
 cmd=[str(binary),'check','--root',str(root),'--diff',str(root/'input.diff'),'--mode','ready','--format','json'];env=os.environ.copy();env.update(RIPR_CACHE_DIR=str(lab/'cache-prereq5b'/name),RAYON_NUM_THREADS='2')
 run=subprocess.run(cmd,env=env,capture_output=True);(lab/f'prereq5b-{name}.json').write_bytes(run.stdout);(lab/f'prereq5b-{name}.stderr').write_bytes(run.stderr);assert run.returncode==0,(name,run.returncode)
 d=json.loads(run.stdout);fs=[f for f in d['findings'] if f['probe']['family']==family and (f['probe']['expression']==expr if expr is not None else f['probe']['expression'].startswith('match '))];assert len(fs)==1,(name,len(fs));f=fs[0];assert f['classification']==want,(name,f['classification'],want)
 rs.append(dict(case=name,command=cmd,native_exit=0,id=f['id'],classification=f['classification'],whole_tail_receipt=any('whole_tail_match_return_observation' in x for x in f['evidence'])))
result=dict(head='5b097c8d18285c2c1511800b1451e9a58734b8cc',sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),records=rs)
(lab/'prerequisite5b-new-controls-receipt.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(rs,indent=2))
