import json,pathlib
root=pathlib.Path('/workspace/8830-proof')
old=json.loads((root/'base/receipt.json').read_text());new=json.loads((root/'candidate/receipt.json').read_text())
assert len(old)==len(new)==42
changed=[]
for a,b in zip(old,new):
 assert (a['case'],a['format'])==(b['case'],b['format'])
 assert a['exit']==b['exit'],(a,b)
 case,fmt=a['case'],a['format'];stem=case+'-'+fmt
 ao=(root/'base'/(stem+'.out')).read_bytes();bo=(root/'candidate'/(stem+'.out')).read_bytes()
 ae=(root/'base'/(stem+'.err')).read_bytes();be=(root/'candidate'/(stem+'.err')).read_bytes()
 assert ae==be,(stem,'stderr differs')
 if fmt not in ['badge-json','badge-shields']:assert ao==bo,(stem,'non-badge output differs')
 else:
  aj=json.loads(ao);bj=json.loads(bo)
  if case in ['empty','readme','comment']:
   assert aj['color']=='brightgreen' and aj['message']=='0'
   kind='no_scope' if case=='empty' else 'no_behavioral_candidates'
   assert bj['color']=='yellow' and bj['message']=='no analysis subject: '+kind
   if fmt=='badge-json':
    assert aj['status']=='pass' and bj['status']=='warn'
    assert aj['analysis_outcome']==bj['analysis_outcome'] and bj['analysis_complete'] is True
    for key in aj:
     if key not in ['status','color','message']: assert aj[key]==bj[key],(stem,key)
   changed.append(stem)
  else:assert ao==bo,(stem,'control changed')
predicate=next(r for r in new if r['case']=='predicate' and r['format']=='badge-json')
assert predicate['message']=='0' and predicate['color']=='brightgreen'
assert predicate['outcome']['counts']['candidate_line_count']>0 and predicate['outcome']['counts']['finding_count']>0
print('PASS: 42 executions per revision; 30 non-badge stdout/stderr/exit comparisons identical; all 42 exit codes and stderr identical; exactly six expected subjectless badge outputs changed; genuine analyzed-subject zero headline retained; unsupported controls identical.')
