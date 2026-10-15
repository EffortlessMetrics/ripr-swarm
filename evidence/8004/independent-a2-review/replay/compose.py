import subprocess,json,pathlib
r=pathlib.Path('/workspace/review8004-composition')
refs={'base':'a142074061128a85e76c1ac494b9939f3a58428b','source':'a2b489b9d7d5b78daebf9fc1b3e4201dae3c9524','main':'ef12e0258a63e2476fcf9509d867152d6b36df80'}
path='fixtures/evidence-promotion-honesty-corpus/corpus.json'
data={k:json.loads(subprocess.check_output(['git','show',v+':'+path],cwd=r)) for k,v in refs.items()}
maps={k:{x['id']:x for x in d['cases']} for k,d in data.items()}
for k,d in data.items(): assert len(maps[k])==len(d['cases'])
metadata=[{k:v for k,v in d.items() if k!='cases'} for d in data.values()]
assert all(x==metadata[0] for x in metadata)
for k in ['source','main']:
    assert all(maps[k].get(i)==row for i,row in maps['base'].items())
overlap=set(maps['source'])&set(maps['main'])
assert all(maps['source'][i]==maps['main'][i] for i in overlap)
result=data['source']
added=[x for x in data['main']['cases'] if x['id'] not in maps['source']]
result['cases']+=added
(r/path).write_text(json.dumps(result,indent=2)+'\n')
receipt={'refs':refs,'counts':{k:len(m) for k,m in maps.items()},'result_count':len(result['cases']),'main_added':[x['id'] for x in added],'divergent':[],'all_complete_rows_and_metadata_preserved':True}
pathlib.Path('/workspace/review8004-evidence/composition-corpus.json').write_text(json.dumps(receipt,indent=2)+'\n')
subprocess.run(['git','add',path],cwd=r,check=True)
tree=subprocess.check_output(['git','write-tree'],cwd=r,text=True).strip()
receipt['tree']=tree
pathlib.Path('/workspace/review8004-evidence/composition-corpus.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
