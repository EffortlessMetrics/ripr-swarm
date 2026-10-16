import json,pathlib,sys,types,subprocess,os,hashlib,re
root=pathlib.Path(__file__).resolve().parent
data=json.loads((root/'8252-extra-corpus.json').read_text());assert len(data['cases'])==13 and len({c['id'] for c in data['cases']})==13
if sys.argv[1]=='native':
 results=[]
 for case in data['cases']:
  for version in ['old','new']:
   src=types.ModuleType('src');src.__path__=[]
   shop=types.ModuleType('src.shop');exec(data['source_'+version],shop.__dict__)
   other=types.ModuleType('src.other');exec(data['source_other'],other.__dict__)
   src.shop=shop;src.other=other;sys.modules.update({'src':src,'src.shop':shop,'src.other':other})
   ns={};exec(case['test'],ns);calls=[]
   for name,value in ns.items():
    if name.startswith('test_') and callable(value):calls.append(value)
    if name.startswith('Test') and isinstance(value,type):
     instance=value();calls.extend(getattr(instance,n) for n in vars(value) if n.startswith('test_'))
   assert len(calls)==case['expected_test_bodies'],(case['id'],len(calls))
   for call in calls:call()
   results.append(dict(case=case['id'],version=version,body_count=len(calls),passed=True))
 (root/'8252-extra-native.json').write_text(json.dumps(results,indent=2)+'\n')
 print(sum(r['body_count'] for r in results),'native old/new test bodies pass; none discriminate the production return')
else:
 binary=pathlib.Path(sys.argv[1]).resolve();results=[]
 version_run=subprocess.run([str(binary),'--version'],capture_output=True,text=True,timeout=30)
 assert version_run.returncode==0,(version_run.returncode,version_run.stderr)
 version=version_run.stdout.strip();source_match=re.search(r'\(([0-9a-f]{40})\)',version)
 for case in data['cases']:
  work=root/'8252-extra-cases'/case['id'];(work/'src').mkdir(parents=True,exist_ok=True);(work/'tests').mkdir(exist_ok=True)
  (work/'pyproject.toml').write_text('[project]\nname="review-oracle-corpus"\nversion="0.0.0"\n')
  (work/'ripr.toml').write_text('[languages]\nenabled=["python"]\n')
  (work/'src/shop.py').write_text(data['source_new']);(work/'src/other.py').write_text(data['source_other']);(work/'tests/test_shop.py').write_text(case['test'])
  (work/'diff.patch').write_text("diff --git a/src/shop.py b/src/shop.py\n--- a/src/shop.py\n+++ b/src/shop.py\n@@ -1,4 +1,4 @@\n def fee(amount):\n     if amount < 0:\n         raise ValueError('negative')\n-    return amount * 2\n+    return amount * 3\n")
  argv=[str(binary),'check','--root',str(work),'--diff',str(work/'diff.patch'),'--mode','fast','--format','json','--quiet']
  run=subprocess.run(argv,capture_output=True,timeout=30,env=dict(os.environ,RAYON_NUM_THREADS='1',RIPR_CACHE_DIR=str(work/'cache')))
  (work/'stdout.json').write_bytes(run.stdout);(work/'stderr.txt').write_bytes(run.stderr);assert run.returncode==0,(case['id'],run.returncode,run.stderr)
  d=json.loads(run.stdout);assert d['analysis_outcome']['analysis_complete'] is True
  assert d['summary']['probes']==1 and d['summary']['findings']==1,(case['id'],d['summary'])
  subjects=[f for f in d['findings'] if f['probe']['owner']=='python:src/shop.py::fee' and f['probe']['file']=='src/shop.py' and f['probe']['line']==4 and f['probe']['family']=='return_value'];assert len(subjects)==1
  f=subjects[0];results.append(dict(case=case['id'],expected=case['expected_class'],actual=f['classification'],alignment=f.get('oracle_alignment'),oracle_kind=f['oracle_kind'],oracle_strength=f['oracle_strength'],exit=run.returncode,argv=argv))
 receipt=dict(binary=str(binary),binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),version=version,head=source_match.group(1) if source_match else None,reviewed_head='1ed9c33f6ca1d4aa11a1aedfff2c1f694a81b62d',results=results)
 (root/'8252-extra-cli.json').write_text(json.dumps(receipt,indent=2)+'\n')
 for row in results:print(row['case'],row['expected'],row['actual'],row['alignment'])
 raise SystemExit(any(r['actual']!=r['expected'] for r in results))
