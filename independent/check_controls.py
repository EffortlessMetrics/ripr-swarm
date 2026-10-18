import pathlib, subprocess,json,os,hashlib
lab=pathlib.Path('/workspace/5500-review'); records=[]
for case in sorted(p for p in lab.iterdir() if p.is_dir() and (p/'input.diff').exists()):
    for label,binary in [('candidate','/workspace/5500-whole-tail-evidence/ripr-candidate'),('predecessor','/workspace/5500-whole-tail-evidence/ripr-predecessor')]:
        env=os.environ.copy(); env.update(RIPR_CACHE_DIR=str(lab/'cache'/case.name/label),RAYON_NUM_THREADS='2')
        args=[binary,'check','--root',str(case),'--diff',str(case/'input.diff'),'--mode','ready','--format','json']
        run=subprocess.run(args,env=env,capture_output=True)
        (case/(label+'.json')).write_bytes(run.stdout); (case/(label+'.stderr')).write_bytes(run.stderr)
        out=json.loads(run.stdout)
        fs=[f for f in out['findings'] if f['probe']['expression'].startswith('match ') and '=>' not in f['probe']['expression']]
        records.append(dict(case=case.name,binary=label,exit=run.returncode,headers=[{'id':f['id'],'class':f['classification'],'expression':f['probe']['expression'],'evidence':f.get('evidence',[])} for f in fs]))
(lab/'cli-controls-receipt.json').write_text(json.dumps(records,indent=2))
print(json.dumps(records,indent=2))
