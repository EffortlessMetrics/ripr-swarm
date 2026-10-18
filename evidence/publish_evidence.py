"""Copy allowlisted text evidence; binaries and mutable caches remain local."""
import pathlib, shutil, hashlib, json, subprocess
destination = pathlib.Path('/workspace/5500-evidence-publish')
destination.mkdir(exist_ok=True)
suffixes = {'.json', '.txt', '.log', '.stderr', '.diff', '.md', '.rs', '.toml', '.patch', '.py'}
entries = []
for label, source in [('evidence', pathlib.Path('/workspace/5500-whole-tail-evidence')), ('independent', pathlib.Path('/workspace/5500-review'))]:
    for path in sorted(source.rglob('*')):
        relative = path.relative_to(source)
        if any(part.startswith('cache') or part in ('.git', 'target', 'fixture-draft') for part in relative.parts):
            continue
        if not path.is_file() or path.suffix not in suffixes:
            continue
        target = destination / label / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, target)
        data = target.read_bytes()
        entries.append({'path': str(target.relative_to(destination)), 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
head = subprocess.check_output(['git', '-C', '/workspace/ripr-swarm', 'rev-parse', 'HEAD']).decode().strip()
tree = subprocess.check_output(['git', '-C', '/workspace/ripr-swarm', 'rev-parse', 'HEAD^{tree}']).decode().strip()
manifest = {'schema': 'ripr-5500-whole-tail-evidence-1', 'source_head': head, 'source_tree': tree, 'consumer_head': '278253a03b00d1aa777eeba7633954122163890f', 'consumer_base': '715a36434a56be3a9abeb5545012d56e2cb9cb60', 'entries': entries, 'retention': 'Text evidence published. Distinct executable witnesses, mutable caches/targets, and labs remain retained locally; no destructive cleanup.', 'claim_boundary': 'Scoped source and local comparison evidence; no hosted/installed/merge/release qualification.'}
(destination / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps({'head': head, 'tree': tree, 'text_files': len(entries), 'bytes': sum(e['bytes'] for e in entries)}, indent=2))
