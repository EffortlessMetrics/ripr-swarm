import json, collections, statistics as st
import sys
rows=[json.loads(l) for l in open(sys.argv[1] if len(sys.argv)>1 else '../receipts/cohort-2026-11-04.jsonl')]
bad=[r for r in rows if 'error' in r or not r.get('valid')]
print(len(rows),'runs; invalid/error:',len(bad), [(r['task'],r['arm'],r['model'],r.get('score_error','')[:80] if r.get('score_error') else r.get('error')) for r in bad][:5])
g=collections.defaultdict(list)
for r in rows:
    if r.get('valid'): g[(r['model'],r['arm'])].append(r)
for k,v in sorted(g.items()):
    esc=[1-r['killed']/r['mutants'] for r in v]
    print(k,'n',len(v),'escape mean %.3f'%st.mean(esc),'sd %.3f'%(st.pstdev(esc)),'runs w/ escape',sum(e>0 for e in esc),
      'turns %.1f'%st.mean(r['turns'] for r in v),'tokens %.0f'%st.mean(r['tokens'] for r in v),'cost %.3f'%st.mean(r['cost_usd'] for r in v),'wall %.0f'%st.mean(r['agent_wall_s'] for r in v),'ripr calls %.1f'%st.mean(r['ripr_calls'] for r in v))
print([ (r['task'],r['arm'],r['model'],r['survived']) for r in rows if r.get('valid') and r['survived']])
