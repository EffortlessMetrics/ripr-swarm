#!/usr/bin/env python3
"""Linux fixture controls; no Nextest/workspace build or network."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import textwrap

REPO = Path(__file__).resolve().parents[2]
HELPER = REPO / "tools/ci/nextest-discovery.py"

def review_controls(root):
    import json
    import os
    from pathlib import Path
    import signal
    import subprocess
    import sys
    import tempfile
    import time

    cases = Path(tempfile.mkdtemp(prefix='review-results.', dir=root))

    def run(name, command, budget='2'):
        d = cases / name
        d.mkdir()
        start = time.monotonic()
        p = subprocess.run([sys.executable, str(root/'discover.py'), 'full', str(d/'list-all.txt'), budget,
                            'bash', '-c', command], capture_output=True)
        return d, p, time.monotonic()-start

    for value in [124, 137]:
        d, p, elapsed = run(f'child-{value}', f'printf partial; exit {value}')
        receipt = json.loads((d/'discovery-full.status').read_text())
        assert p.returncode == value and receipt['state'] == 'child-failed'
        assert receipt['child_exit'] == value and not (d/'list-all.txt').exists()
        print(f'child-{value}: child-failed, not timeout PASS')
    d,p,_ = run('deadline', 'printf partial; sleep 10', '.2')
    assert p.returncode == 124 and json.loads((d/'discovery-full.status').read_text())['state']=='timeout'
    assert not (d/'list-all.txt').exists()
    print('supervisor deadline: timeout PASS')
    for sig in [signal.SIGTERM, signal.SIGINT]:
        d=cases/f'cancel-{sig}'; d.mkdir()
        p=subprocess.Popen([sys.executable,str(root/'discover.py'),'full',str(d/'list-all.txt'),'20',
            'bash','-c',f'echo $$ > "{d}/pid"; trap "" TERM INT; printf partial; (sleep .7; echo escaped > "{d}/escaped") & sleep 20'])
        end=time.monotonic()+3
        while not (d/'pid').exists() and time.monotonic()<end: time.sleep(.01)
        assert (d/'pid').exists()
        start=time.monotonic(); p.send_signal(sig); code=p.wait(timeout=2)
        assert code==128+sig and time.monotonic()-start<2
        time.sleep(.8)
        assert not (d/'escaped').exists()
        assert not (d/'list-all.txt').exists()
        assert json.loads((d/'discovery-full.status').read_text())['state']=='cancelled'
        print(f'cancel-{sig}: prompt child-group teardown, no canonical PASS')
    # Force status-store failure after command success (diagnostics already written).
    d=cases/'store-failure'
    d,p,_=run('store-failure', f'mkdir "{d}/discovery-full.status"; printf "valid\\n"')
    assert p.returncode==74 and not (d/'list-all.txt').exists()
    assert not (d/'discovery-full.status').is_file()
    print('receipt write failure after diagnostics: no canonical PASS')
    # Force diagnostic-store failure, not just status failure.
    d=cases/'diagnostic-failure'
    d,p,_=run('diagnostic-failure', f'mkdir "{d}/discovery-full.metadata.json"; printf "valid\\n"')
    assert p.returncode==74 and not (d/'list-all.txt').exists()
    assert not (d/'discovery-full.status').exists()
    print('diagnostic write failure: no canonical/status PASS')
    d,p,_=run('bounded-diagnostics', "printf 'valid\\n'; python3 -c 'import sys; sys.stderr.write(chr(27)+\"[31m\\n::error::injection\\n\"+\"x\"*1000000)' ")
    assert p.returncode==0
    raw=(d/'discovery-full.metadata.json').read_bytes()
    assert len(raw)<70000 and b'\x1b' not in raw and b'\n::error::' not in raw
    assert json.loads(raw)['stderr_bytes'] > 1000000
    assert set(json.loads(raw)) == {'phase', 'stderr_bytes', 'stderr_bucket'}
    assert not list(d.glob('*.raw')) and not list(d.glob('*.partial')) and not list(d.glob('.discovery-*'))
    print('bounded metadata-only diagnostics and fresh stage cleanup PASS')
    print(f'fixtures={cases}')


def final_controls(root):
    import json
    import os
    from pathlib import Path
    import subprocess
    import sys
    import tempfile
    import time
    cases=Path(tempfile.mkdtemp(prefix='final-results.',dir=root))
    # Inject real TERM at the exact publication boundary, without any production hook.
    d=cases/'after-rename'; d.mkdir()
    code='''
    import importlib.util, os, signal, sys
    spec=importlib.util.spec_from_file_location("discovery",sys.argv[1]); m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
    canonical=sys.argv[2]; real=m.os.replace
    def replace(a,b):
     real(a,b)
     if str(b)==canonical: os.kill(os.getpid(),signal.SIGTERM)
    m.os.replace=replace
    sys.argv=[sys.argv[1],"full",canonical,"2","bash","-c","printf 'valid\\n'"]
    sys.exit(m.main())
    '''
    p=subprocess.run([sys.executable,'-c',textwrap.dedent(code),str(root/'discover.py'),str(d/'list-all.txt')],capture_output=True)
    assert p.returncode==143, (p.returncode,p.stderr)
    assert not (d/'list-all.txt').exists()
    assert json.loads((d/'discovery-full.status').read_text())['state']=='cancelled'
    print('TERM immediately after canonical rename: cancelled status/nonzero/no canonical PASS')
    d=cases/'leftover';d.mkdir()
    p=subprocess.run([sys.executable,str(root/'discover.py'),'full',str(d/'list-all.txt'),'3','bash','-c',
     f'(exec >/dev/null 2>&1; sleep .7; echo escaped > "{d}/escaped") & printf "valid\\n"'],capture_output=True)
    assert p.returncode==71,(p.returncode,p.stderr)
    assert json.loads((d/'discovery-full.status').read_text())['state']=='leftover-processes'
    assert not (d/'list-all.txt').exists()
    time.sleep(.9)
    assert not (d/'escaped').exists()
    print('normal child exit with closed-pipe descendant: fail/teardown/no canonical PASS')
    d=cases/'secret';d.mkdir();secret='SAMPLE_SECRET_MUST_NOT_EXPORT_123'
    p=subprocess.run([sys.executable,str(root/'discover.py'),'full',str(d/'list-all.txt'),'2','bash','-c',
     f'printf "valid\\n"; printf "{secret}" >&2'],capture_output=True)
    assert p.returncode==0
    assert secret.encode() not in p.stdout+p.stderr
    for path in d.rglob('*'):
     if path.is_file(): assert secret.encode() not in path.read_bytes()
    assert json.loads((d/'discovery-full.metadata.json').read_text())=={'phase':'full','stderr_bytes':len(secret),'stderr_bucket':'present'}
    assert json.loads((d/'discovery-full.status').read_text())['state']=='complete'
    assert (d/'list-all.txt').read_text()=='valid\n'
    print('sample stderr secret: metadata-only export and consumer-ready success PASS')
    print(f'fixtures={cases}')


BASIC = r'''#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
RESULTS=$(mktemp -d "$PWD/results-revised.XXXXXX")
printf 'fixtures=%s\n' "$RESULTS"
run() {
 local name=$1 partition=$2 full=$3 expected=$4
 local dir="$RESULTS/$name"
 mkdir -p "$dir"
 # Stale canonical files must not survive a failing invocation.
 printf 'stale\n' > "$dir/list-1.txt"; printf 'stale\n' > "$dir/list-all.txt"
 set +e
 bash -e -c '
   bash ./discover.sh partition-1 "$1/list-1.txt" 1 bash -c "$2"
   bash ./discover.sh full "$1/list-all.txt" 1 bash -c "$3"
   touch "$1/tests-started"
 ' _ "$dir" "$partition" "$full" > "$dir/outer.stdout" 2> "$dir/outer.stderr"
 code=$?
 set -e
 [[ $code == "$expected" ]]
 if [[ $expected == 0 ]]; then
   [[ -f "$dir/tests-started" ]]; [[ $(cat "$dir/list-all.txt") == $'a\nb\nb\nc' ]]
 else
   [[ ! -e "$dir/tests-started" ]]
   if [[ $partition == "$good" ]]; then [[ ! -e "$dir/list-all.txt" ]]; else [[ ! -e "$dir/list-1.txt" ]]; fi
 fi
 [[ ! -e "$dir/shard-1.status" && ! -e "$dir/junit.xml" ]]
 printf '%s exit=%s tests_started=%s PASS\n' "$name" "$code" "$([[ -f "$dir/tests-started" ]] && echo yes || echo no)"
}
good="printf 'b\\na\\nb\\n'; printf '\\033[31mdiagnostic\\n::error::not-a-command\\n' >&2"
run success "$good" "$good; printf 'c\\n'" 0
run partition_nonzero "printf partial; echo bad >&2; exit 7" "$good" 7
run full_nonzero "$good" "printf partial; echo bad >&2; exit 7" 7
run full_timeout "$good" "printf partial; echo waiting >&2; sleep 10" 124
run partition_timeout "printf partial; sleep 10" "$good" 124
run full_empty "$good" ':' 65
run full_whitespace "$good" "printf ' \\n\\t\\n'" 65
run partition_empty ':' "$good" 65
# Full set is genuinely independent, never a union/copy of partition.
printf 'independent full invocation and duplicate preservation: PASS\n'
'''

def basic_controls(root):
    (root / "controls.sh").write_text(BASIC)
    (root / "discover.sh").write_text('#!/bin/sh\nexec "' + sys.executable + '" "$(dirname "$0")/discover.py" "$@"\n')
    subprocess.run(["bash", str(root / "controls.sh")], check=True, timeout=20)

def workflow_controls(root):
    workflow = (REPO / '.github/workflows/rust-test-shards.yml').read_text()
    step = workflow.split('id: rust-test-shard\n', 1)[1].split('        run: |\n', 1)[1].split('\n      # A failed', 1)[0]
    step = textwrap.dedent(step).replace('${{ matrix.shard }}', '1')
    assert 'full target/nextest/ci/list-all.txt 480 cargo nextest list --workspace' in step
    assert 'partition-1 target/nextest/ci/list-1.txt 480 cargo nextest list' in step
    upload = workflow.split('- name: Upload shard results', 1)[1].split('  rust-test-shards-verify:', 1)[0]
    assert 'target/nextest/ci/discovery-*.metadata.json' in upload
    assert 'target/nextest/ci/discovery-*.status' in upload
    assert '.stderr' not in upload and '.partial' not in upload
    for mode, expected in [('ok', 0), ('partition-fail', 7), ('full-fail', 7),
                           ('full-timeout', 124), ('full-empty', 65)]:
        case = root / mode
        (case / 'bin').mkdir(parents=True)
        (case / 'tools/ci').mkdir(parents=True)
        shutil.copyfile(HELPER, case / 'tools/ci/nextest-discovery.py')
        scripts = {
            'git': '#!/bin/sh\nprintf "fixture-identity\\n"\n',
            'rustc': '#!/bin/sh\nprintf "fixture-rust-version\\n"\n',
            # Fixture-only budget contraction; execute the actual helper unmodified.
            'python3': '#!' + sys.executable + '\nimport os,sys\na=sys.argv[1:]\na[3]="0.2"\nos.execv(sys.executable,[sys.executable]+a)\n',
            'cargo': '#!' + sys.executable + '\n' + textwrap.dedent('''\
                import os,sys,time
                from pathlib import Path
                a=sys.argv[1:]; mode=os.environ['MODE']
                if '--version' in a: print('fixture-version'); sys.exit(0)
                if a[:2] == ['nextest','list']:
                    full='--partition' not in a
                    if (full and mode=='full-empty'): sys.exit(0)
                    print('a',flush=True)
                    if full: print('independent-full',flush=True)
                    if (full and mode=='full-fail') or (not full and mode=='partition-fail'): sys.exit(7)
                    if full and mode=='full-timeout': time.sleep(5)
                    sys.exit(0)
                if a[:2] == ['nextest','run']:
                    Path('tests-started').touch()
                    Path('target/nextest/ci/junit.xml').write_text('<testsuites tests="1"></testsuites>')
                    sys.exit(0)
                sys.exit(99)
            ''')
        }
        for name, source in scripts.items():
            path = case / 'bin' / name
            path.write_text(source)
            path.chmod(0o755)
        env = dict(os.environ, PATH=str(case/'bin')+os.pathsep+os.environ['PATH'], MODE=mode,
                   GITHUB_REPOSITORY='fixture/repo', GITHUB_RUN_ID='1', GITHUB_RUN_ATTEMPT='1')
        p = subprocess.run(['bash','--noprofile','--norc','-eo','pipefail','-c',step],
                           cwd=case, env=env, capture_output=True, timeout=10)
        assert p.returncode == expected, (mode,p.returncode,p.stderr)
        ci = case/'target/nextest/ci'
        if expected:
            assert not (case/'tests-started').exists()
            assert not (ci/'shard-1.status').exists() and not (ci/'junit.xml').exists()
            failed = 'partition-1' if mode.startswith('partition') else 'full'
            assert not (ci/('list-1.txt' if failed=='partition-1' else 'list-all.txt')).exists()
            assert json.loads((ci/f'discovery-{failed}.status').read_text())['exit'] != 0
        else:
            assert (case/'tests-started').exists()
            assert (ci/'list-all.txt').read_text()=='a\nindependent-full\n'
            assert (ci/'list-1.txt').read_text()=='a\n'
            for phase in ['partition-1','full']:
                assert json.loads((ci/f'discovery-{phase}.status').read_text())['state']=='complete'
                assert set(json.loads((ci/f'discovery-{phase}.metadata.json').read_text()))=={'phase','stderr_bytes','stderr_bucket'}
        assert not list(ci.glob('*.raw')) and not list(ci.glob('*.partial'))
        print(f'actual workflow {mode}: PASS')


def spawn_handoff_controls(root):
    import signal
    import time
    for sig in (signal.SIGTERM, signal.SIGINT):
        d = root / f'spawn-handoff-{sig}'
        d.mkdir()
        code = r'''
import importlib.util, os, signal, sys
spec = importlib.util.spec_from_file_location('discovery', sys.argv[1])
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
canonical, marker, first = sys.argv[2], sys.argv[3], int(sys.argv[4])
real = m.subprocess.Popen
children = []
def spawn(*args, **kwargs):
    child = real(*args, **kwargs)
    children.append(child)
    real_wait = child.wait
    def wait(*args, **kwargs):
        # Additional synchronous signals while cleanup owns the unreaped child.
        os.kill(os.getpid(), signal.SIGTERM)
        os.kill(os.getpid(), signal.SIGINT)
        return real_wait(*args, **kwargs)
    child.wait = wait
    os.kill(os.getpid(), first)  # Before main receives/assigns this handle.
    return child
m.subprocess.Popen = spawn
sys.argv = [sys.argv[1], 'full', canonical, '3', 'bash', '-c',
            'trap "" TERM INT; sleep .7; echo escaped > "$1"', '_', marker]
try:
    result = m.main()
    assert children and children[0].returncode is not None, 'direct child not reaped'
    sys.exit(result)
finally:
    for child in children:
        if child.poll() is None:
            os.killpg(child.pid, signal.SIGKILL)
            # Bypass the injected wait for failure-only fixture cleanup.
            real_wait = type(child).wait
            real_wait(child)
'''
        p = subprocess.run([sys.executable, '-c', textwrap.dedent(code),
                            str(root/'discover.py'), str(d/'list-all.txt'),
                            str(d/'escaped'), str(sig)], capture_output=True, timeout=5)
        assert p.returncode == 128+sig, (sig, p.returncode, p.stderr)
        assert not (d/'list-all.txt').exists()
        receipt = json.loads((d/'discovery-full.status').read_text())
        assert receipt['state'] == 'cancelled' and receipt['exit'] == 128+sig
        time.sleep(.8)
        assert not (d/'escaped').exists()
        print(f'spawn handoff {sig} with repeated teardown signals: PASS')


if __name__ == '__main__':
    with tempfile.TemporaryDirectory(prefix='nextest-discovery-controls-') as temp:
        root=Path(temp)
        shutil.copyfile(HELPER,root/'discover.py')
        basic_controls(root)
        review_controls(root)
        final_controls(root)
        workflow_controls(root)
        spawn_handoff_controls(root)
    print('26 discovery/workflow controls PASS')
