# Shadow-heavy re-export counterexample check

The ordering has a real work tradeoff: candidate `a80be9118d460f1fddeb68413af097f6122208cb` can resolve a genuine re-export target before rejecting a body-local binding shadow that frozen base `b51aebc7e4735529befd677ebf14071c7f09e477` rejects first. This bounded check found **no material warm regression established** in two small controls. It retains a cold fan-out cost signal and a sub-MiB RSS increase. It does not establish a universally faster ordering or bound larger export graphs.

The source candidate, prior published report, 227 observations and original archives remain immutable. This supplement neither changes the source nor ports any evidence to central candidate `fdc018e9…`. The coordinator has explained that fdc018e9 is a local frozen subject, not yet published; the prior `not our ref` result is expected object availability, not an access-policy blocker. No fetch retry, main substitution, build, full suite, CI request or expensive remeasurement was performed here.

## Source assessment

`ReExportIndex::resolve_to_owner` resolves the import module, checks mocks, then traverses the existing export graph with a fresh traversal stack. `MAX_REEXPORT_HOPS` remains 4; #13300's per-module adjacency remains intact. Resolution is not a per-test cached target answer, so the moved call can add a graph walk for each shadowed imported call. It may perform existing path/config probes on a resolver-cache miss and warm existing per-run memos or lazy adjacency; no persistent write, new cache machinery or new execution authority is introduced by the moved expression. Its final Boolean is still conjoined with both local and enclosing binding refusals.

Each control contains **256 extracted shadow tests**, each importing `price as cost` through a genuine barrel and locally binding/calling its own `cost`, plus **one genuine test** that reaches the changed owner. Cached extraction was inspected: all 256 shadow bodies retain the local declaration/call and have no alternate `reach_body_text`. This prevents a faster result through empty extraction or loss of the shadowed subjects.

- `chain3`: three explicit re-export hops to the owner.
- `star16`: one barrel with sixteen star targets; exactly one exports the owner, the other fifteen have unrelated exports.

Source-derived work accounting, **not a newly instrumented counter**: on those 256 shadowed `cost(...)` calls the old arm rejects before `resolve_to_owner`; the new arm can make 256 additional target queries. A successful three-hop chain visits four module frames; the star case checks its root and sixteen targets. The timing/CPU/RSS observations below measure the complete real CLI, including any offsetting reductions in unrelated guard work.

## Same-host observations

N=3 per cell, alternating baseline/candidate trial order. Each trial starts with an empty isolated application cache and then performs one unchanged warm check. Same retained source-built binaries and release profile as the original handoff; no compiler or competing workload ran. The existing `wait4` harness has a 45-second child-tree limit and polls completion every 5 ms. OS page caches were not flushed.

| Control/state | Wall median baseline → candidate (ms) | CPU median baseline → candidate (ms) | Max RSS baseline → candidate (KiB) |
| --- | ---: | ---: | ---: |
| Chain 3, cold | 46.4 → 46.8 | 37.1 → 38.5 | 31,536 → 32,108 |
| Chain 3, warm | 36.3 → 36.4 | 30.8 → 29.3 | 30,512 → 31,180 |
| Star 16, cold | 46.8 → 52.0 | 37.3 → 38.2 | 31,196 → 32,064 |
| Star 16, warm | 36.2 → 36.1 | 30.6 → 31.0 | 30,388 → 30,908 |

Warm CPU ranges overlap: chain baseline 28.5–31.5 vs candidate 29.3–34.8 ms; star baseline 30.2–31.2 vs candidate 30.1–34.2 ms. Star cold wall increased by about 5.3 ms (one harness polling interval), while its CPU ranges overlap at 35.8–37.7 vs 36.3–40.9 ms. The observed cold cost and small RSS increase are not dismissed as zero; N=3 and this timing granularity cannot establish a reliable general cold-regression estimate. No improvement is claimed on these controls.

All **24 invocations** ended with exit 0, complete outcomes and no timeout. Within each control, every cold/warm/baseline/candidate stdout byte is identical: one `weakly_exposed` finding, reach `yes`, and exactly the genuine related test, with `re_export_chain_followed`. All 256 shadowed calls remain uncredited. The uncertainty (`observation_unconfirmed`) is retained, not promoted into an exposure claim.

## Candidate decision and handoff

This check does not change the scoped recommendation to keep a80be9118 eligible for central selection, with a workload-dependent tradeoff recorded. Its earlier useful-reach large-workload gains remain bound to their original b51-based subjects; they are not a claim that shadow-heavy re-export workloads improve. Larger/deeper branching graphs and suites dominated by shadowed imports remain outside this small check. If composition review chooses a different ordering, this control is a useful counterexample to retain, not a new release gate.

No source revision or cache design was added. The existing central integration owner `01a1a959-f983-763a-96e6-7dbe1cced019` retains selection and affected semantic qualification after its integrated candidate settles. The original reproduced strict-Clippy failure is unchanged and is not rerun or counted as passing here.

## Reproduce and identify

- Baseline binary SHA-256: `ca2cedb6555259a2eaffb8864062de4cd990a3a01f48118e7aa13fab5298352f`, source b51aebc7e.
- Candidate binary SHA-256: `4bd0e0c71d28a814417102e326d5159e2171dcc43cffa933258dca9f28da8a3e`, source a80be9118.
- Command: `RIPR_CACHE_DIR=<isolated-cache> RAYON_NUM_THREADS=4 <absolute-binary> check --base HEAD~1 --format json --mode draft`.
- `control.py` reconstructs both deterministic Git fixtures and executes the small paired check; exact Git bundles, fixture identities, raw stdout/stderr and 24 resource records are retained. `summary.json` contains ranges as well as medians and maxima.
- Prior immutable report/artifacts: `ff7113451a5a768a69d4aad008386f1fba009a57`, `metrics/ts-owner-10925-a80be9118/`; source remains `a80be9118d460f1fddeb68413af097f6122208cb`.
