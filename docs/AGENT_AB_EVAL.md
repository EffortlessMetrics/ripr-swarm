# Agent A/B eval: does ripr make coding agents better?

Governing issue: #12912. Companion to [agent-as-user runs](AGENT_AS_USER_RUNS.md),
which measures agents *with* ripr only. This eval adds the control arm, so the
claim "ripr makes agents faster and better" has a number with a counterfactual.

## Question

For the same task, repo snapshot, agent model and brief, does an agent that has
`ripr` finish with **better tests**, in **fewer turns and tokens**, and with
**fewer dead ends**, than an agent without it?

## Design

| Element | Rule |
|---|---|
| Task | A first-party crate whose latest commit adds a function with a smoke-only test (`benchmarks/agent_ab/tasks/<name>/`; the harness, not the repo, creates each run's scaffold commit followed by the feature commit). The agent is told only the commit message's feature and "strengthen the tests so wrong versions would fail". First-party code only, so scoring needs no run of external code. |
| Arms | **A**: `ripr` on `PATH`, agent told it exists and has never seen its docs. **B**: control, no `ripr`. Later: **C**: `cargo mutants` instead of ripr (issue slice 3). |
| Brief | Identical text across arms except the one sentence naming ripr. |
| Isolation | Each run gets its own fresh copy and git repo (scaffold commit, then the feature commit). The answer key lives outside the run directory. Agents are told to stay in their directory. |
| Scorer | Hand-written mutants in `benchmarks/agent_ab/tasks/answer-keys.json`, scored by the reference scorer in the appendix (`score.py KEY TASK TREE`), one textual replacement each; a mutant is *caught* only when the nonempty original test inventory completes with a named libtest failure after the mutant compiles. Assertion failures and mutation-induced panics reported by a named test both count; Cargo/runner failures do not. Equivalent mutants are not allowed (a `>=`/`>` flip at a no-op truncate was removed during validation). |
| Positive control | Before any scored run, a tree with the known-good test in `benchmarks/agent_ab/controls/<task>/<task>.rs` must catch every mutant of its task, and the weak baseline must catch none. A scorer that fails either is broken, and no result is reported. |
| Production edits | A run that edits `src/` is scored but flagged; the brief forbids it. |

## Invoice evaluator input domain

The invoice brief includes the domain documented in its agent-visible `src/lib.rs`:
at most 100 lines, each `unit_cents <= 1_000_000` and `qty <= 10_000`,
`tax_bps <= 10_000`, and `BuyNGetOneFree(n)` with `n <= 1_000_000`.
All values are nonnegative. `PercentOff` accepts any `u32` (capped at 100);
`FlatOff` accepts any `u32` (saturating subtraction). The evaluator must keep
original and generated test inputs in this domain. Inputs outside it are not
part of this benchmark contract; the fixture does not validate them.

Within this domain, each gross is at most 10^10 cents and the summed subtotal
at most 10^12. Discounts cannot increase it. The largest basis-point numerator
is 10^16 + 5,000, and the taxed total is at most 2 * 10^12, all within `u64`.
The BOGO divisor is at most 1,000,001, within `u32`; its free-item product is
at most the gross. The capped percentage multiplication is at most 10,000.
Existing controls fit this domain. `invoice_domain_boundaries` adds a fourth
positive-control test (previously three); the weak baseline remains two tests.
The answer key remains seven invoice mutants and 21 overall. Historical pilot
receipts retain their original test inventory and are not rewritten as results
from this expanded positive control.

## Metrics per run

| Metric | Source | Direction |
|---|---|---|
| `mutants_caught / total` | scorer | higher |
| `baseline_test_pass` | scorer | must be true |
| `tool_uses`, `total_tokens`, `wall_seconds` | the harness's per-agent usage report | lower |
| `ripr_commands` (arm A) | agent self-report cross-checked with the transcript | context only |
| `dead_ends` | agent self-report, verified against its command list: a command or edit after which the result did not move | lower |
| `tests_added`, `test_lines_added` | `git diff` of the run tree | context |
| `caught_per_test_line` | derived | higher |

## What counts as a result

- One run is an anecdote. Report **A minus B per task with the run count**, and
  call anything under three runs per arm per task a pilot.
- Report both arms' failures. An arm that reaches full catch in fewer tokens is
  better; an arm that reaches full catch with ripr but not without is a larger
  win than one that merely saves tokens.
- Model, ripr build, task set and date are part of the receipt. Numbers do not
  transfer across models.
- Static ripr verdicts are never described as runtime mutation outcomes. The
  mutants are the independent authority.

## Known limits

- The answer key lives in this repository, which an agent could read if it left
  its directory. Runs are instructed not to; a transcript showing otherwise
  voids the run.
- Tokens come from the harness's usage report for the agent and include its
  own reasoning, so they compare arms, not absolute cost.
- Small first-party crates favour a competent agent without any tool. A
  measured effect here does not establish a lower bound for harder tasks: it
  may shrink or reverse, and those effects remain unmeasured. Harder targets
  (Python, TypeScript, upstream crates) are later slices.

## Receipts

See `metrics/agent-ab/`. Each receipt records the per-run rows, the
positive-control result and the aggregate A-minus-B delta. Per-task deltas can be
read from the rows. Metrics marked below as not yet recorded (`wall_seconds`,
`tests_added`, `test_lines_added`, `caught_per_test_line`) are listed in the
receipt's `fields_not_yet_recorded`.

## Appendix: reference scorer

Kept here, not as a repository file, because repository policy keeps non-Rust
programming files to approved surfaces. Porting it to `xtask` is slice 2 of #12912.
Unviable (does not compile), timed-out and invalid executions are reported separately
and never count as caught. A failing, missing, empty, ignored or incomplete original
test run stops scoring before any mutant is run. `comparison_valid: false` and
`caught: null` mean no valid comparison, not zero catches. A nonbehavioral mutant
outcome likewise invalidates the comparison while preserving per-mutant outcomes.
The weak-baseline and positive-control checks above remain prerequisites under
the same compiler and runner configuration; do not report agent results when
either control is invalid or fails its expected catch count.

This appendix reference scorer is Linux/POSIX-only and fails closed on other
platforms before launching Cargo. It owns a new process group for each Cargo
invocation; timeout kills that group, drains the pipes and reaps Cargo before
classification. Fixtures must not detach descendants from that group. This is
not a sandbox or a Windows process-tree cleanup claim.

This bounded scorer supports native first-party libtest fixtures with no doctests,
custom harnesses or benchmarks. It uses `--tests --no-fail-fast`, enumerates named
tests, and requires the same complete inventory and consistent terminal summaries
for each mutant. It trusts libtest output; it is not an adversarial-output parser.
Use Rust 1.95.0 as pinned in `rust-toolchain.toml`. TREE and its adjacent `.mut`,
`.mut-target` and `.base-target` paths must be disposable and evaluator-owned.
All 21 mutants in the key were checked to compile on 2026-11-05. Save as `score.py` and run `python3 -I score.py benchmarks/agent_ab/tasks/answer-keys.json TASK TREE`.

```python
import sys, subprocess, shutil, os, json, re, signal
from collections import Counter
# Native first-party libtest fixtures only; stdout is trusted harness output.
key,task,tree=sys.argv[1:4]
k=json.load(open(key))['tasks'][task]
tree=os.path.abspath(tree)
work=tree+'.mut'
tgt=tree+'.mut-target'
env=dict(os.environ,CARGO_TARGET_DIR=tgt)
def run_owned(args,*,cwd,env,timeout=600):
    p=subprocess.Popen(args,cwd=cwd,stdout=subprocess.PIPE,stderr=subprocess.PIPE,
                       env=env,text=True,start_new_session=True)
    try:
        out,err=p.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        # Cargo may have exited while a native test still holds its pipes open.
        try: os.killpg(p.pid,signal.SIGKILL)
        except ProcessLookupError: pass
        p.communicate()  # Drain inherited pipes and reap the owned group leader.
        raise
    return subprocess.CompletedProcess(args,p.returncode,out,err)
def run(d,t=None,expected=None):
    if not sys.platform.startswith('linux') or os.name!='posix':
        return 'invalid_execution', None
    e=dict(env,CARGO_TARGET_DIR=t or tgt)
    def cargo(args):
        return run_owned(['cargo','test','--tests','--offline','--locked']+args,
                         cwd=d,env=e)
    try:
        b=cargo(['--no-run','-q'])
        if b.returncode!=0: return 'unviable', None
        listing=cargo(['-q','--','--list','--format=terse'])
        names=Counter(re.findall(r'^(.+): test$',listing.stdout,re.M))
        if listing.returncode!=0 or not names or (expected is not None and names!=expected):
            return 'invalid_execution', None
        r=cargo(['-q','--no-fail-fast','--','--format=pretty','--test-threads=1'])
        rows=re.findall(r'^test (.+) \.\.\. (ok|FAILED|ignored)$',r.stdout,re.M)
        summaries=re.findall(r'^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;',r.stdout,re.M)
        passed=sum(status=='ok' for _,status in rows)
        failed=sum(status=='FAILED' for _,status in rows)
        complete=(Counter(name for name,_ in rows)==names and len(rows)==passed+failed
                  and bool(summaries) and sum(int(s[1]) for s in summaries)==passed
                  and sum(int(s[2]) for s in summaries)==failed
                  and all(s[3:]==('0','0','0') for s in summaries)
                  and all((s[0]=='FAILED')==(int(s[2])>0) for s in summaries))
        errors=[line for line in r.stderr.splitlines() if line.startswith('error:')]
        expected_errors=all(re.fullmatch(r'error: test failed, to rerun pass `[^`]+`|error: \d+ targets? failed:',line) for line in errors)
        if not complete or not expected_errors or r.returncode!=(101 if failed else 0):
            return 'invalid_execution', names
        return ('caught' if failed else 'MISSED'), names
    except subprocess.TimeoutExpired:
        return 'timeout', None
    except OSError:
        return 'invalid_execution', None
baseline,names=run(tree,tree+'.base-target')
res={'task':task,'baseline_test_pass':baseline=='MISSED','baseline_status':baseline,
     'test_count':sum(names.values()) if names else 0,'mutants':{}}
if baseline=='MISSED':
    for m in k['mutants']:
        if os.path.exists(work): shutil.rmtree(work)
        shutil.copytree(tree,work,ignore=shutil.ignore_patterns('target','*.mut-target'))
        p=os.path.join(work,k['file']); s=open(p).read()
        if s.count(m['from'])!=1:
            res['mutants'][m['id']]='NOMATCH'; continue
        open(p,'w').write(s.replace(m['from'],m['to']))
        res['mutants'][m['id']]=run(work,expected=names)[0]
shutil.rmtree(work,ignore_errors=True)
valid=baseline=='MISSED' and len(res['mutants'])==len(k['mutants']) and all(v in ('caught','MISSED') for v in res['mutants'].values())
res['comparison_valid']=valid
res['caught']=sum(v=='caught' for v in res['mutants'].values()) if valid else None
res['total']=len(k['mutants'])
print(json.dumps(res))
```

## Appendix: scorer regression controls

Save the following block beside `score.py` as `scorer-regressions.py`, then run
`python3 -I scorer-regressions.py`. All fourteen entrypoint cases and the process-group control must pass. These deterministic
instrument controls exercise the scorer entrypoint, including the requirement
that an invalid original executes no mutants. They are not native Rust evidence
and do not replace the weak/positive fixture checks above. Keep both blocks in
sync until the scorer and regression controls move into `xtask`.

```python
# Save beside score.py; run: python3 -I scorer-regressions.py
# Instrument controls only; these do not replace the native fixture controls.
import ast, contextlib, io, json, os, pathlib, runpy, select, signal, subprocess, sys, tempfile, time
scorer=pathlib.Path(__file__).with_name('score.py')
PASS='test witness ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n'
FAIL='test witness ... FAILED\n\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out;\n'
# case: (valid comparison, baseline passes, mutant outcome)
cases={
    'pass':(True,True,'MISSED'),
    'assertion':(True,True,'caught'),
    'panic':(True,True,'caught'),
    'runner':(False,True,'invalid_execution'),
    'unrelated_error':(False,True,'invalid_execution'),
    'incomplete':(False,True,'invalid_execution'),
    'compile':(False,True,'unviable'),
    'timeout':(False,True,'timeout'),
    'failed_original':(False,False,None),
    'zero_original':(False,False,None),
    'ignored_original':(False,False,None),
    'missing_cargo':(False,False,None),
    'unsupported_platform':(False,False,None),
    'timeout_group_gone':(False,True,'timeout'),
}
for case,(valid,baseline,outcome) in cases.items():
    with tempfile.TemporaryDirectory(prefix='agent-ab-score-') as tmp:
        root=pathlib.Path(tmp); tree=root/'tree'; tree.mkdir()
        (tree/'lib.rs').write_text('ORIGINAL')
        key=root/'key.json'
        key.write_text(json.dumps({'tasks':{'fixture':{'file':'lib.rs','mutants':[
            {'id':'one','from':'ORIGINAL','to':'MUTANT'}]}}}))
        calls=[]
        def fake(args,**kw):
            mutant=str(kw['cwd']).endswith('.mut')
            calls.append((mutant,args))
            code=0; out=''; err=''
            if case=='missing_cargo': raise FileNotFoundError('cargo')
            if '--no-run' in args:
                if mutant and case=='compile': code=1
            elif '--list' in args:
                out='' if case=='zero_original' else 'witness: test\n'
            else:
                out=PASS
                if case=='failed_original' or (mutant and case in ('assertion','panic','unrelated_error')):
                    code=101; out=FAIL
                    if case=='panic': out+='thread witness panicked: attempt to subtract with overflow\n'
                if mutant and case=='runner': code=42; out=''; err='runner unavailable\n'
                if mutant and case=='unrelated_error': err='error: unrelated Cargo failure\n'
                if mutant and case=='incomplete': out='test witness ... ok\n'
                if mutant and case in ('timeout','timeout_group_gone'): raise subprocess.TimeoutExpired(args,600)
                if case=='zero_original': out='test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n'
                if case=='ignored_original': out='test witness ... ignored\ntest result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out;\n'
            if not kw.get('text'): out=out.encode(); err=err.encode()
            return subprocess.CompletedProcess(args,code,out,err)
        killed=[]; drained=[]
        class FakeProcess:
            pid=123456789
            def __init__(self,args,**kw):
                assert kw['start_new_session'] is True
                assert kw['stdout']==kw['stderr']==subprocess.PIPE
                self.args,self.kw=args,kw
            def communicate(self,timeout=None):
                if timeout is None:
                    drained.append(self.pid); self.returncode=-signal.SIGKILL
                    return '', ''
                result=fake(self.args,**self.kw)
                self.returncode=result.returncode
                return result.stdout,result.stderr
        def killpg(pid,sig):
            assert pid==FakeProcess.pid and sig==signal.SIGKILL
            killed.append(pid)
            if case=='timeout_group_gone': raise ProcessLookupError()
        saved_popen,saved_killpg,saved_argv,saved_platform=subprocess.Popen,os.killpg,sys.argv,sys.platform
        try:
            subprocess.Popen,os.killpg=FakeProcess,killpg
            if case=='unsupported_platform': sys.platform='win32'
            sys.argv=[str(scorer),str(key),'fixture',str(tree)]
            output=io.StringIO()
            with contextlib.redirect_stdout(output): runpy.run_path(str(scorer),run_name='__main__')
            result=json.loads(output.getvalue())
        finally:
            subprocess.Popen,os.killpg,sys.argv,sys.platform=saved_popen,saved_killpg,saved_argv,saved_platform
        assert result['baseline_test_pass']==baseline, (case,result)
        assert result['comparison_valid']==valid, (case,result)
        assert result['caught']==(int(outcome=='caught') if valid else None), (case,result)
        assert result['mutants']==({'one':outcome} if baseline else {}), (case,result)
        if not baseline: assert not any(mutant for mutant,_ in calls), (case,calls)
        if case in ('timeout','timeout_group_gone'):
            assert killed==drained==[FakeProcess.pid], (case,killed,drained)
        else: assert not killed and not drained, case
        if case=='unsupported_platform': assert not calls
        print(case,'PASS')
# Exercise the actual helper, not the mocked process API. Extract only its
# definition so this probe does not run Cargo or the scorer entrypoint.
module=ast.parse(scorer.read_text())
helper=next(n for n in module.body if isinstance(n,ast.FunctionDef) and n.name=='run_owned')
ns=dict(subprocess=subprocess,os=os,signal=signal)
exec(compile(ast.Module(body=[helper],type_ignores=[]),str(scorer),'exec'),ns)
real_popen=subprocess.Popen
owned=[]; descendants=[]
def ready_popen(*args,**kw):
    p=real_popen(*args,**kw); owned.append(p)
    # Handshake before starting the timeout: the descendant exists, inherits
    # both pipes, and belongs to the leader's process group.
    assert select.select([p.stdout],[],[],5)[0], 'descendant failed to start'
    child=int(p.stdout.readline().strip()); descendants.append(child)
    assert os.getpgid(child)==p.pid
    return p
def live(pid):
    # A killed orphan may remain a zombie until the host's init reaps it.
    try: return pathlib.Path(f'/proc/{pid}/stat').read_text().split(') ',1)[1][0]!='Z'
    except FileNotFoundError: return False
try:
    subprocess.Popen=ready_popen
    try:
        ns['run_owned']([sys.executable,'-c',
            'import os,time\npid=os.fork()\nif pid==0:\n print(os.getpid(),flush=True)\ntime.sleep(60)'],
            cwd=str(scorer.parent),env=dict(os.environ),timeout=0.1)
        raise AssertionError('expected timeout')
    except subprocess.TimeoutExpired: pass
    assert len(owned)==len(descendants)==1
    assert owned[0].returncode==-signal.SIGKILL, 'leader not reaped'
    deadline=time.monotonic()+5
    while live(descendants[0]) and time.monotonic()<deadline: time.sleep(0.01)
    assert not live(descendants[0]), 'descendant survived timeout'
finally:
    subprocess.Popen=real_popen
    for p in owned:
        try: os.killpg(p.pid,signal.SIGKILL)
        except ProcessLookupError: pass
        p.communicate()
print('timeout_process_group PASS')

```


### Native Cargo timeout regression (required before reporting fixture proof)

On Linux with Rust 1.95.0, copy a disposable first-party fixture and add a named
libtest that spawns a same-group child which writes a readiness marker, then
blocks. Record both PIDs and the inherited process-group ID. Use an evaluator-only
copy of the scorer with a short timeout after readiness (leave its classification
logic unchanged). Check that the JSON reports baseline `timeout`,
`baseline_test_pass: false`, `comparison_valid: false`, `caught: null`, and no
mutants. Verify Cargo is reaped, the test and child are no longer live, and no
inherited output pipe or target lock remains held. Repeat with the timeout in a
mutant only: preserve the passing baseline and classify that mutant as `timeout`,
not `caught`. This native regression is distinct from the Python process-group
control; do not describe it as passed until it has actually run.
