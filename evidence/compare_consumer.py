import json, hashlib, collections, pathlib
root = pathlib.Path(__file__).parent
old = json.loads((root / 'consumer-predecessor.json').read_text())
new = json.loads((root / 'consumer-candidate.json').read_text())
def indexed(output):
    rows = output['findings']
    assert rows and len({r['id'] for r in rows}) == len(rows)
    assert output['analysis_outcome']['analysis_complete'] is True
    return {r['id']: r for r in rows}
a, b = indexed(old), indexed(new)
target = 'probe:src_proof_budget.rs:match_arm:67654dc6'
assert target in a and target in b
rows = []
for key in sorted(a.keys() | b.keys()):
    before, after = a.get(key), b.get(key)
    changed_fields = [field for field in sorted((before or {}).keys() | (after or {}).keys()) if (before or {}).get(field) != (after or {}).get(field)]
    rows.append({'id': key, 'before': (before or {}).get('classification'), 'after': (after or {}).get('classification'), 'status': 'introduced' if before is None else 'removed' if after is None else 'changed' if changed_fields else 'unchanged', 'changed_fields': changed_fields, 'probe': (after or before)['probe'], 'source_currentness': (after or before).get('source_currentness')})
receipt = {
    'consumer_head': '278253a03b00d1aa777eeba7633954122163890f',
    'consumer_base': '715a36434a56be3a9abeb5545012d56e2cb9cb60',
    'diff_bytes': (root / 'consumer.diff').stat().st_size,
    'diff_sha256': hashlib.sha256((root / 'consumer.diff').read_bytes()).hexdigest(),
    'outputs': {name: {'sha256': hashlib.sha256((root / ('consumer-' + name + '.json')).read_bytes()).hexdigest(), 'count': len(data['findings']), 'classes': dict(collections.Counter(r['classification'] for r in data['findings'])), 'current_classes': dict(collections.Counter(r['classification'] for r in data['findings'] if r.get('source_currentness') == 'candidate_current')), 'deleted_classes': dict(collections.Counter(r['classification'] for r in data['findings'] if r.get('source_currentness') == 'base_deleted')), 'analysis_outcome': data['analysis_outcome']} for name, data in [('predecessor', old), ('candidate', new)]},
    'target': next(r for r in rows if r['id'] == target),
    'inventory': dict(collections.Counter(r['status'] for r in rows)),
    'all_rows': rows,
    'other_top_level_changes': [k for k in sorted(old.keys() | new.keys()) if k != 'findings' and old.get(k) != new.get(k)],
    'claim_boundary': 'Exact frozen consumer comparison only; no hosted, installed, merge or full consumer clearance.'
}
(root / 'consumer-comparison.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps({k: v for k, v in receipt.items() if k not in ('all_rows', 'outputs')}, indent=2))
print(json.dumps(receipt['outputs'], indent=2))
