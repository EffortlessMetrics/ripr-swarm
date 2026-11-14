#!/usr/bin/env python3
"""Exercise offline provider recovery with an already installed wheel payload.

The caller supplies a disposable fixture, private scratch directory and the
absolute installed binary. Cargo fetch is explicit setup; analysis stays offline.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import re
import subprocess
import sys

binary, source, work, evidence = map(Path, sys.argv[1:])
assert binary.is_absolute() and binary.is_file() and os.access(binary, os.X_OK)
assert source.is_dir() and not work.exists() and not evidence.exists()
work.mkdir(parents=True)
evidence.mkdir(parents=True)
case = work / 'fixture'
shutil.copytree(source / 'input', case)
shutil.copyfile(source / 'diff.patch', case / 'diff.patch')
inputs = [case / p for p in ('Cargo.toml', 'Cargo.lock', 'src/lib.rs', 'tests/boundary.rs', 'diff.patch')]

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def identities():
    return {str(p.relative_to(case)): digest(p) for p in inputs}

baseline = identities()
home = work / 'cargo-home'
home.mkdir()
env = os.environ.copy()
for key in list(env):
    if key.upper().startswith('PROPTEST_'):
        del env[key]
env.update(CARGO_HOME=str(home), CARGO_TARGET_DIR=str(work / 'target'),
           RIPR_CACHE_DIR=str(work / 'ripr-cache'), RUSTUP_AUTO_INSTALL='0', CARGO_TERM_COLOR='never')
record = {'binary_sha256': digest(binary), 'fixture': baseline, 'commands': [], 'status': 'incomplete'}

def run(label, argv, expected=0, environment=None):
    result = subprocess.run(argv, cwd=case, env=environment or env, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, timeout=180, check=False)
    (evidence / (label + '.stdout')).write_bytes(result.stdout)
    (evidence / (label + '.stderr')).write_bytes(result.stderr)
    record['commands'].append({'label': label, 'exit': result.returncode,
                               'stdout_sha256': hashlib.sha256(result.stdout).hexdigest(),
                               'stderr_sha256': hashlib.sha256(result.stderr).hexdigest()})
    assert len(result.stdout) + len(result.stderr) < 4 * 1024 * 1024, label
    assert result.returncode == expected, (label, result.returncode, result.stderr.decode(errors='replace'))
    return result.stdout, result.stderr

def finding(output):
    rows = json.loads(output)['findings']
    assert len(rows) == 1 and rows[0]['probe']['expression'] == 'amount >= threshold', rows
    return rows[0]

try:
    run('fetch', ['cargo', 'fetch', '--locked', '--manifest-path', 'Cargo.toml'])
    packages = []
    for section in (case / 'Cargo.lock').read_text().split('[[package]]')[1:]:
        fields = dict(re.findall(r'^(name|version|source|checksum) = "([^"]+)"$', section, re.M))
        if fields.get('source') == 'registry+https://github.com/rust-lang/crates.io-index':
            assert all(key in fields for key in ('name', 'version', 'checksum')), fields
            packages.append(fields)
    assert packages
    for p in packages:
        archives = list((home / 'registry/cache').glob('*/' + p['name'] + '-' + p['version'] + '.crate'))
        assert len(archives) == 1 and digest(archives[0]) == p['checksum'], p
    record['locked_archives'] = len(packages)
    # Request Cargo offline mode and set refusing proxies; this does not prove no attempted fetch.
    env.update(CARGO_NET_OFFLINE='true', HTTP_PROXY='http://127.0.0.1:9',
               HTTPS_PROXY='http://127.0.0.1:9', ALL_PROXY='http://127.0.0.1:9', NO_PROXY='')
    metadata = ['cargo', 'metadata', '--offline', '--locked', '--format-version', '1', '--manifest-path', 'Cargo.toml']
    check = [str(binary), 'check', '--root', str(case), '--diff', str(case / 'diff.patch'),
             '--mode', 'fast', '--format', 'json', '--quiet']
    run('healthy-metadata', metadata)
    healthy = finding(run('healthy', check)[0])
    assert healthy['classification'] == 'exposed' and healthy['related_tests_total'] == 1, healthy
    assert finding(run('healthy-repeat', check)[0]) == healthy
    targets = [p for p in packages if p['name'] == 'zerocopy']
    assert len(targets) == 1
    label = 'zerocopy-' + targets[0]['version']
    archives = list((home / 'registry/cache').glob('*/' + label + '.crate'))
    sources = list((home / 'registry/src').glob('*/' + label))
    assert len(archives) == len(sources) == 1
    archive, unpacked = archives[0], sources[0]
    assert not archive.is_symlink() and not unpacked.is_symlink()
    checksum = digest(archive)
    held = work / 'held'
    held.mkdir()
    archive.rename(held / archive.name)
    unpacked.rename(held / unpacked.name)
    try:
        _, stderr = run('missing-metadata', metadata, expected=101)
        assert b'zerocopy' in stderr and b'--offline' in stderr
        refused = finding(run('missing', check)[0])
        assert refused['classification'] == 'static_unknown' and refused['related_tests_total'] == 0, refused
        assert refused['related_tests'] == refused['observed_values'] == refused['activation']['observed_values'] == []
        assert refused['static_limit_kind'] == 'rust_macro_reach_unresolved'
        assert refused['oracle_kind'] == 'unknown' and refused['oracle_strength'] == 'none'
        assert all(refused['ripr'][stage]['state'] == 'unknown' for stage in ('reach', 'infect', 'propagate'))
        command = 'cargo metadata --offline --locked --format-version 1 --manifest-path Cargo.toml'
        action = refused['recommended_next_step']
        assert command in action and 'zerocopy' not in action
        assert any('offline provider verification;' in line and 'exited unsuccessfully' in line
                   for line in refused['evidence'])
        assert finding(run('missing-repeat', check)[0]) == refused
        human = check.copy()
        human[human.index('json')] = 'human-full'
        rendered = run('missing-human', human)[0].decode()
        next_step = rendered.split('\nNext step\n', 1)[1].split('\nDrill in:', 1)[0]
        assert ' '.join(next_step.split()) == ' '.join(action.removesuffix('.').split())
        assert not archive.exists() and not unpacked.exists()
        assert identities() == baseline and digest(held / archive.name) == checksum
    finally:
        if (held / archive.name).exists():
            (held / archive.name).rename(archive)
        if (held / unpacked.name).exists():
            (held / unpacked.name).rename(unpacked)
    run('restored-metadata', metadata)
    assert finding(run('restored', check)[0]) == healthy
    assert finding(run('restored-repeat', check)[0]) == healthy
    assert identities() == baseline and digest(archive) == checksum
    record.update(status='pass', withheld=label, withheld_sha256=checksum)
finally:
    (evidence / 'record.json').write_text(json.dumps(record, indent=2) + '\n')
