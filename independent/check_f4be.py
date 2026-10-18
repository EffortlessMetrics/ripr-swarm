import pathlib,subprocess,json,os,hashlib
lab=pathlib.Path('/workspace/5500-review');binary='/workspace/5500-whole-tail-evidence/ripr-final-f4be8084a'; records=[]
for case in sorted(p for p in lab.iterdir() if p.is_dir() and (p/'input.diff').exists() and p.name != 'glob_scalar'):
 env=os.environ.copy();env.update(RIPR_CACHE_DIR=str(lab/'cache'/case.name/'f4be'),RAYON_NUM_THREADS='2')
 args=[binary,'check','--root',str(case),'--diff',str(case/'input.diff'),'--mode','ready','--format','json']
 run=subprocess.run(args,env=env,capture_output=True);(case/'f4be.json').write_bytes(run.stdout);(case/'f4be.stderr').write_bytes(run.stderr)
 out=json.loads(run.stdout);fs=[f for f in out['findings'] if f['probe']['expression'].startswith('match ') and '=>' not in f['probe']['expression']]
 records.append(dict(case=case.name,exit=run.returncode,headers=[dict(id=f['id'],classification=f['classification'],expression=f['probe']['expression'],evidence=f['evidence']) for f in fs]))
receipt=dict(head='f4be8084a1cbc7cd9d66c7e10ec4b9eb46b7a5bc',binary=binary,binary_sha256=hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest(),records=records)
(lab/'f4be-cli-controls-receipt.json').write_text(json.dumps(receipt,indent=2))
print(json.dumps(dict(binary_sha256=receipt['binary_sha256'],results=[dict(case=r['case'],exit=r['exit'],headers=[f['classification'] for f in r['headers']]) for r in records]),indent=2))
