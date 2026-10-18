from pathlib import Path
import json, hashlib, sys
P=Path('/workspace/scratch/8265-reconstruction')
variant=sys.argv[1] if len(sys.argv)>1 else 'candidate'
rows=[]
for name in ['clean','same-path-rank','scoped-public-only','scoped-rest','trusted-subset']:
    before=json.loads((P/'baseline-controls'/f'{name}.json').read_text())
    after=json.loads((P/(variant+'-controls')/f'{name}.json').read_text())
    assert before['findings'] and after['findings']
    assert before['analysis_outcome']['analysis_complete'] and after['analysis_outcome']['analysis_complete']
    if name in ['clean','same-path-rank','scoped-rest']:
        assert before==after, name
    elif name=='scoped-public-only':
        assert {f['classification'] for f in before['findings']}=={'reachable_unrevealed'}
        assert {f['classification'] for f in after['findings']}=={'static_unknown'}
        assert len(before['findings'])==len(after['findings'])==2
        for f in after['findings']:
            assert not f.get('canonical_gap_id')
            raw=json.dumps(f)
            assert 'rust_assertion_context_unresolved' in raw and 'gap_evidence_unresolved' in raw
            assert any('glob-imports' in e and 'src/globs.rs' in e for e in f['evidence'])
    else:
        assert before['summary']==after['summary']
        for f in before['findings']:
            assert any('glob-imports' in e and 'src/z.rs' in e for e in f['evidence'])
        for f in after['findings']:
            assert any('stringify' in e and 'src/a.rs' in e for e in f['evidence'])
    rows.append({'case':name,'whole_json_equal':before==after,'baseline_summary':before['summary'],'candidate_summary':after['summary']})
(P/('cli-comparison-'+variant+'.json')).write_text(json.dumps(rows,indent=2)+'\n')
print(json.dumps(rows,indent=2))
