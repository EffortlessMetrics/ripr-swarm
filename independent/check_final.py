import pathlib,subprocess,json,os,hashlib
lab=pathlib.Path('/workspace/5500-review');binary='/workspace/5500-whole-tail-evidence/ripr-final-c96331b71'; records=[]
for case in sorted(p for p in lab.iterdir() if p.is_dir() and (p/'input.diff').exists() and p.name != 'glob_scalar'):
 env=os.environ.copy();env.update(RIPR_CACHE_DIR=str(lab/'cache'/case.name/'c963'),RAYON_NUM_THREADS='2')
 args=[binary,'check','--root',str(case),'--diff',str(case/'input.diff'),'--mode','ready','--format','json']
 run=subprocess.run(args,env=env,capture_output=True);(case/'c963.json').write_bytes(run.stdout);(case/'c963.stderr').write_bytes(run.stderr)
 out=json.loads(run.stdout);fs=[f for f in out['findings'] if f['probe']['expression'].startswith('match ') and '=>' not in f['probe']['expression']]
 records.append(dict(case=case.name,exit=run.returncode,headers=[dict(id=f['id'],classification=f['classification'],expression=f['probe']['expression'],evidence=f['evidence']) for f in fs]))
receipt=dict(head='c96331b7113c7626609d57405236e85ef4a0a703',binary=binary,binary_sha256=hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest(),records=records)
(lab/'c963-cli-controls-receipt.json').write_text(json.dumps(receipt,indent=2))
print(json.dumps(dict(binary_sha256=receipt['binary_sha256'],results=[dict(case=r['case'],exit=r['exit'],headers=[f['classification'] for f in r['headers']]) for r in records]),indent=2))
