import json,subprocess,pathlib
path='fixtures/evidence-promotion-honesty-corpus/corpus.json'
def read(ref):return json.loads(subprocess.check_output(['git','show',ref+':'+path],text=True))
b=read('a142074061128a85e76c1ac494b9939f3a58428b');c=read('a2b489b9d7d5b78daebf9fc1b3e4201dae3c9524');m=read('98c8f172b')
result={}
for k in set(b)|set(c)|set(m):
 if k=='cases':continue
 bv,cv,mv=b.get(k),c.get(k),m.get(k)
 assert cv==mv or cv==bv or mv==bv,('metadata conflict',k)
 result[k]=mv if cv==bv else cv
maps=[{r['id']:r for r in x['cases']} for x in (b,c,m)]
assert all(len(t)==len(x['cases']) for t,x in zip(maps,(b,c,m)))
rows={};divergent=[]
for ident in set().union(*maps):
 bv,cv,mv=[t.get(ident) for t in maps]
 if not(cv==mv or cv==bv or mv==bv):divergent.append(ident);continue
 chosen=mv if cv==bv else cv
 if chosen is not None:rows[ident]=chosen
assert not divergent,divergent
order=list(dict.fromkeys([r['id'] for x in (c,m) for r in x['cases']]))
result['cases']=[rows[k] for k in order if k in rows]
# No whole input row may disappear unless an explicit upstream edit superseded it.
for ident,row in maps[1].items():assert rows[ident]==row or row==maps[0].get(ident),ident
for ident,row in maps[2].items():assert rows[ident]==row or row==maps[0].get(ident),ident
pathlib.Path(path).write_text(json.dumps(result,indent=2)+'\n')
receipt=dict(base='a142074061128a85e76c1ac494b9939f3a58428b',candidate='a2b489b9d7d5b78daebf9fc1b3e4201dae3c9524',main=subprocess.check_output(['git','rev-parse','98c8f172b'],text=True).strip(),counts=[len(x['cases']) for x in (b,c,m)]+[len(rows)],divergent=divergent,metadata_preserved=True,complete_keyed_rows=True)
pathlib.Path('/workspace/8004-repair/corpus-integration.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
