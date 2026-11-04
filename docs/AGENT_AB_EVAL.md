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
| Scorer | Hand-written mutants in `benchmarks/agent_ab/tasks/answer-keys.json`, scored by the reference scorer in the appendix (`score.py KEY TASK TREE`), one textual replacement each; a mutant is *caught* when `cargo test` fails with it applied. Equivalent mutants are not allowed (a `>=`/`>` flip at a no-op truncate was removed during validation). |
| Positive control | Before any scored run, a tree with the known-good test in `benchmarks/agent_ab/controls/<task>/<task>.rs` must catch every mutant of its task, and the weak baseline must catch none. A scorer that fails either is broken, and no result is reported. |
| Production edits | A run that edits `src/` is scored but flagged; the brief forbids it. |

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
  difference that appears on these is a floor for harder tasks; an absence is
  not evidence of no benefit on large or unfamiliar code. Harder targets
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
Save as `score.py` and run `python3 -I score.py benchmarks/agent_ab/tasks/answer-keys.json TASK TREE`.

```python
import sys,subprocess,shutil,os,json
# usage: score.py KEYFILE TASK TREE
key,task,tree=sys.argv[1:4]
k=json.load(open(key))['tasks'][task]
tree=os.path.abspath(tree)
work=tree+'.mut'
tgt=tree+'.mut-target'
env=dict(os.environ,CARGO_TARGET_DIR=tgt)
def run(d,t=None):
    e=dict(env,CARGO_TARGET_DIR=t or tgt)
    return subprocess.run(['cargo','test','-q','--offline'],cwd=d,capture_output=True,env=e).returncode
res={'task':task,'baseline_test_pass':run(tree,tree+'.base-target')==0,'mutants':{}}
for m in k['mutants']:
    if os.path.exists(work): shutil.rmtree(work)
    shutil.copytree(tree,work,ignore=shutil.ignore_patterns('target','*.mut-target'))
    p=os.path.join(work,k['file']); s=open(p).read()
    if s.count(m['from'])!=1:
        res['mutants'][m['id']]='NOMATCH'; continue
    open(p,'w').write(s.replace(m['from'],m['to']))
    res['mutants'][m['id']]='caught' if run(work)!=0 else 'MISSED'
shutil.rmtree(work,ignore_errors=True)
c=sum(v=='caught' for v in res['mutants'].values())
res['caught']=c; res['total']=len(k['mutants'])
print(json.dumps(res))
```
