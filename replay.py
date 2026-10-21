#!/usr/bin/env python3
"""Real Git / public CLI discriminator. No product implementation changes."""
import hashlib, json, pathlib, subprocess, sys
ROOT = pathlib.Path(__file__).resolve().parent
BIN = pathlib.Path(sys.argv[1]).resolve()
CASES = [('text_only', 'behavior', False), ('binary_only', None, True), ('mixed_behavior', 'behavior', True), ('comment_only', 'comment', False), ('mixed_comment', 'comment', True), ('empty', None, False)]
def run(args, cwd=None):
    return subprocess.run([str(a) for a in args], cwd=cwd, capture_output=True, timeout=120)
def git(root, *args):
    p = run(['git', *args], root)
    assert p.returncode == 0, (args, p.stderr)
    return p.stdout
rows=[]
for name, text_kind, binary in CASES:
    case=ROOT/name
    case.mkdir(exist_ok=False)
    repo=case/'repo'
    (repo/'src').mkdir(parents=True)
    (repo/'Cargo.toml').write_text('[package]\nname="mixed_proof"\nversion="0.1.0"\nedition="2024"\n')
    before='pub fn threshold(x: i32) -> bool { x > 10 }\n'
    (repo/'src/lib.rs').write_text(before)
    old_binary=b'\x00'+bytes(range(256))*8
    (repo/'payload.dat').write_bytes(old_binary)
    git(repo, 'init', '-q')
    git(repo, 'config', 'user.name', 'RIPR proof fixture')
    git(repo, 'config', 'user.email', 'proof@example.invalid')
    git(repo, 'add', '.')
    git(repo, 'commit', '-qm', 'base')
    base=git(repo, 'rev-parse', 'HEAD').decode().strip()
    if text_kind == 'behavior':
        (repo/'src/lib.rs').write_text(before.replace(' > ', ' >= '))
    if text_kind == 'comment':
        (repo/'src/lib.rs').write_text(before+'// documentation only\n')
    if binary:
        (repo/'payload.dat').write_bytes(old_binary+b'\x00changed\n')
    git(repo, 'add', '.')
    git(repo, 'commit', '--allow-empty', '-qm', name)
    patch=git(repo, 'diff', '--no-ext-diff', '--no-textconv', '--no-color', '--unified=0', base, 'HEAD')
    (case/'input.patch').write_bytes(patch)
    assert (b'Binary files a/payload.dat and b/payload.dat differ' in patch) == binary
    assert (b'@@' in patch) == (text_kind is not None)
    if binary:
        assert (repo/'payload.dat').read_bytes() != old_binary
    commands={
        'check-json':[BIN,'check','--root','.', '--diff',case/'input.patch','--format','json','--quiet'],
        'check-github':[BIN,'check','--root','.', '--diff',case/'input.patch','--format','github','--quiet'],
        'check-human':[BIN,'check','--root','.', '--diff',case/'input.patch','--format','human','--quiet'],
        'diff-json':[BIN,'diff','--root','.', '--base',base,'--head','HEAD','--json'],
    }
    results={}
    for label,args in commands.items():
        p=run(args,repo)
        (case/(label+'.stdout')).write_bytes(p.stdout)
        (case/(label+'.stderr')).write_bytes(p.stderr)
        result={'argv':[str(a) for a in args], 'cwd':str(repo), 'exit':p.returncode}
        if label.endswith('json'):
            report=json.loads(p.stdout)
            ao=report['analysis_outcome']
            outcome=ao.get('outcome',ao)
            result.update(kind=outcome['kind'], complete=ao['analysis_complete'], counts=outcome['counts'],limitations=outcome['limitations'])
        results[label]=result
    rows.append({'case':name, 'binary_changed':binary, 'text_kind':text_kind, 'patch_sha256':hashlib.sha256(patch).hexdigest(),'results':results})
identity={'binary':str(BIN),'binary_sha256':hashlib.sha256(BIN.read_bytes()).hexdigest(),'version':run([BIN,'--version']).stdout.decode().strip()}
(ROOT/'results.json').write_text(json.dumps({'identity':identity,'cases':rows},indent=2)+'\n')
for row in rows:
    r=row['results']['check-json']
    print(row['case'], r['exit'],r['kind'],r['complete'],r['counts'])
# Standalone negative and supported positive must hold before challenging mixed sections.
by={row['case']:row for row in rows}
assert by['binary_only']['results']['check-json']['kind']=='unsupported_input'
assert by['text_only']['results']['check-json']['kind']=='complete_with_findings'
assert by['comment_only']['results']['check-json']['kind']=='no_behavioral_candidates'
assert by['empty']['results']['check-json']['kind']=='no_scope'
failures=[]
for name in ['mixed_behavior','mixed_comment']:
    for label in ['check-json','diff-json']:
        r=by[name]['results'][label]
        if r['complete'] or not r['limitations']:
            failures.append(name+'/'+label+': binary changes omitted from complete outcome')
(ROOT/'assertions.json').write_text(json.dumps({'control_assertions':'pass','mixed_honesty_failures':failures},indent=2)+'\n')
for f in failures: print('FAIL:',f)
sys.exit(bool(failures))
