"""Native controls for the verbatim canonical base-recovery shell (Linux CI)."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(sys.argv[1]).resolve()
OUT.mkdir(parents=True, exist_ok=False)
ENV = {k: v for k, v in os.environ.items() if not k.upper().startswith('GIT_')}
ENV.update(GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL='/dev/null',
           GIT_AUTHOR_NAME='Recovery control', GIT_AUTHOR_EMAIL='test@example.invalid',
           GIT_COMMITTER_NAME='Recovery control', GIT_COMMITTER_EMAIL='test@example.invalid')
LABELS = ['Prepare independent user-text base', 'Recover pinned base compiler arity if needed',
          'Recover pinned base missed-txt match if needed']
FILES = ['crates/ripr/src/app/test_stub.rs', 'crates/ripr/src/output/format.rs']
OLD = ['41ce81981802365b31e5182b7a7a51540ff8d7be', '5c223a5e714540e314600f24f6f07373d136c475']
FIXED = ['e934320980555de2d431b2aa8482a6059e9cf7f7', 'cf05a4c4a649b292fe0cae25e43f2ce3d28d79b6']
source = Path(os.environ.get('RECOVERY_WORKFLOW', ROOT / '.github/workflows/rust-gates.yml')).read_text()
scripts = []
for label in LABELS:
    block = source.split('      - name: ' + label + '\n', 1)[1].split('\n      - ', 1)[0]
    lines = block.split('        run: |\n', 1)[1].splitlines()
    script = []
    for line in lines:
        if line and not line.startswith('          '):
            break
        script.append(line[10:] if line else '')
    scripts.append('\n'.join(script).rstrip() + '\n')
for i, script in enumerate(scripts):
    (OUT / f'production-{i}.sh').write_text(script)
rows = []


def git(tree, *args):
    return subprocess.check_output(['git', '-C', str(tree), *args], env=ENV, stderr=subprocess.PIPE).decode().strip()


def scenario(name, mode='clean', stage=1, baseline='old'):
    directory = OUT / name
    subject = directory / 'subject'
    subject.mkdir(parents=True)
    runner = directory / 'runner'
    runner.mkdir()
    git(subject, 'init', '-q')
    for path, fixture, expected in zip(FILES, ['test_stub.rs.txt', 'format.rs.txt'], OLD):
        dest = subject / path
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes((ROOT / 'tools/repo-policy/tests/fixtures/base_recovery' / fixture).read_bytes())
        assert git(subject, 'hash-object', path) == expected
        if baseline == 'fixed':
            dest.write_bytes((OUT / 'clean/runner/user-text-base' / path).read_bytes())
        elif baseline == 'other':
            with dest.open('a') as file:
                file.write('\n// other committed input\n')
    (subject / 'unrelated.txt').write_text('tracked\n')
    (subject / '.gitignore').write_text('ignored.txt\n')
    helper = subject / '.github/scripts/check-base-recovery-state.py'
    helper.parent.mkdir(parents=True)
    shutil.copyfile(ROOT / '.github/scripts/check-base-recovery-state.py', helper)
    git(subject, 'add', '.')
    git(subject, '-c', 'core.hooksPath=/dev/null', 'commit', '-qm', 'base')
    git(subject, '-c', 'core.hooksPath=/dev/null', 'commit', '--allow-empty', '-qm', 'subject')
    env = dict(ENV, RUNNER_TEMP=str(runner), GITHUB_WORKSPACE=str(subject))
    row = dict(case=name, exits=[])
    tree = runner / 'user-text-base'

    def execute(index):
        result = subprocess.run(['bash', '--noprofile', '--norc', str(OUT / f'production-{index}.sh')],
                                cwd=subject, env=env, capture_output=True, timeout=20)
        row['exits'].append(dict(step=index, exit=result.returncode, stderr=result.stderr.decode()))
        return result.returncode

    assert execute(0) == 0
    before = [git(tree, 'hash-object', path) for path in FILES]
    if stage == 2:
        assert execute(1) == 0
    if mode == 'staged':
        (tree / 'unrelated.txt').write_text('different\n')
        git(tree, 'add', 'unrelated.txt')
    elif mode == 'intent-to-add':
        (tree / 'new.txt').write_text('intent\n')
        git(tree, 'add', '-N', 'new.txt')
    elif mode == 'staged-delete':
        git(tree, 'rm', 'unrelated.txt')
    elif mode == 'staged-rename':
        git(tree, 'mv', 'unrelated.txt', 'renamed.txt')
    elif mode in ['untracked', 'ignored', 'newline']:
        filename = {'untracked': 'new.txt', 'ignored': 'ignored.txt', 'newline': 'odd\nname.txt'}[mode]
        (tree / filename).write_text('unexpected\n')
    elif mode in ['assume', 'skip']:
        git(tree, 'update-index', '--assume-unchanged' if mode == 'assume' else '--skip-worktree', 'unrelated.txt')
        (tree / 'unrelated.txt').write_text('hidden\n')
    elif mode == 'unstaged':
        (tree / 'unrelated.txt').write_text('unexpected\n')
    elif mode == 'stub-mutation':
        with (tree / FILES[0]).open('a') as file:
            file.write('\n// wrong repair\n')
    elif mode == 'chmod-repair':
        (tree / FILES[0]).chmod(0o755)
    elif mode == 'staged-repair':
        git(tree, 'add', FILES[0])
    elif mode == 'reverse':
        assert execute(2) == 0
    if mode == 'clean':
        for index in range(stage, 3):
            assert execute(index) == 0, row
        after = [git(tree, 'hash-object', path) for path in FILES]
        assert after == (before if baseline == 'other' else FIXED)
    else:
        assert execute(stage) != 0, f'accepted hostile input: {row}'
    row['status'] = git(tree, 'status', '--porcelain=v1', '--untracked-files=all', '--ignored=matching')
    rows.append(row)
    (OUT / 'receipt.json').write_text(json.dumps(rows, indent=2) + '\n')


scenario('clean')
scenario('fixed', baseline='fixed')
scenario('other', baseline='other')
for phase in [1, 2]:
    for mutation in ['staged', 'intent-to-add', 'staged-delete', 'staged-rename', 'untracked', 'ignored', 'newline',
                     'assume', 'skip', 'unstaged', 'stub-mutation']:
        scenario(f'{phase}-{mutation}', mutation, phase)
scenario('staged-repair', 'staged-repair', 2)
scenario('chmod-repair', 'chmod-repair', 2)
scenario('reverse', 'reverse')
for baseline in ['fixed', 'other']:
    scenario(f'{baseline}-staged', 'staged', baseline=baseline)
    scenario(f'{baseline}-untracked', 'untracked', baseline=baseline)
print(f'{len(rows)} native recovery controls passed')
