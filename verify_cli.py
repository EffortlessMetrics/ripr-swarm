import json, pathlib, subprocess, sys, hashlib, os
root=pathlib.Path(__file__).resolve().parent
data=json.loads((root/'corpus.json').read_text())
binary=pathlib.Path(sys.argv[1]).resolve()
label=sys.argv[2]
assert len(data['cases']) == 11
assert len({case['id'] for case in data['cases']}) == 11
results=[]
for case in data['cases']:
    work=root/'cases'/case['id']
    (work/'src').mkdir(parents=True,exist_ok=True)
    (work/'tests').mkdir(exist_ok=True)
    (work/'pyproject.toml').write_text('[project]\nname = "oracle-corpus"\nversion = "0.0.0"\n')
    (work/'ripr.toml').write_text('[languages]\nenabled = ["python"]\n')
    (work/'src/shop.py').write_text(data['source_new'])
    (work/'tests/test_shop.py').write_text(case['test'])
    (work/'diff.patch').write_text('diff --git a/src/shop.py b/src/shop.py\n--- a/src/shop.py\n+++ b/src/shop.py\n@@ -1,4 +1,4 @@\n def fee(amount):\n     if amount < 0:\n         raise ValueError(\'negative\')\n-    return amount * 2\n+    return amount * 3\n')
    argv=[str(binary),'check','--root',str(work),'--diff',str(work/'diff.patch'),'--mode','fast','--format','json','--quiet']
    run=subprocess.run(argv,capture_output=True,timeout=30,env=dict(os.environ,RAYON_NUM_THREADS='1'))
    (work/(label+'.stdout.json')).write_bytes(run.stdout)
    (work/(label+'.stderr.txt')).write_bytes(run.stderr)
    assert run.returncode==0,(case['id'],run.returncode,run.stderr)
    doc=json.loads(run.stdout)
    assert doc['analysis_outcome']['analysis_complete'] is True, (case['id'], 'partial analysis')
    assert doc['summary']['probes'] == 1 and doc['summary']['findings'] == 1, (case['id'], doc['summary'])
    subjects=[f for f in doc['findings'] if f['probe']['file']=='src/shop.py' and f['probe']['line']==4 and f['probe']['family']=='return_value' and f['probe']['owner']=='python:src/shop.py::fee']
    assert len(subjects)==1,(case['id'],subjects,doc['summary'])
    finding=subjects[0]
    results.append(dict(case=case['id'],expected=case['expected_class'],actual=finding['classification'],oracle_kind=finding['oracle_kind'],oracle_strength=finding['oracle_strength'],alignment=finding['oracle_alignment'],argv=argv,exit=run.returncode))
receipt=dict(binary=str(binary),binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),results=results)
(root/(label+'-receipt.json')).write_text(json.dumps(receipt,indent=2)+'\n')
for row in results:
    print(row['case'],row['expected'],row['actual'])
raise SystemExit(any(r['expected']!=r['actual'] for r in results))
