# Cohort collection and report receipts (#12902, #12912)

The [core contract](CORE_CONTRACT.md) owns token accounting, runtime quality,
paired comparisons and report interpretation. Collection reuses those owners;
it does not define a second success rule or turn lower token use into a win.

## Collection identity and costs

New collected records use `receipt_schema_version: 3`. Each sweep generates a
fresh cohort ID; `COHORT_ID` may join files from one deliberately frozen experiment.
Task definitions, selected mutants, common prompt, tool grants, timeout and
harness source are hashed. Model aliases are not immutable model snapshots and
reported RIPR versions are not executable attestations. Recorded hashes establish
internal input agreement, not independent execution authenticity.

The terminal result's input, output, cache-creation and cache-read counters are
preserved once. Missing components keep total usage unknown. The provider SDK's
`cost_usd` is an estimate, not a bill; cached token volume and dollars are distinct.
Partial transcripts retain observed call counts separately from complete counts.
Error results may retain incurred cost. Scorer and metadata errors preserve
already-collected usage instead of turning a failed attempt into a free trial.

Each trial has a unique workspace and control shim. One sweep writer appends
completed receipts, including exception records with the actual model/task/arm,
replicate and turn cap. An interrupted process can still omit planned trials;
this format is not a predeclared attempt manifest. The report's denominator is
supplied receipts, not an independently authenticated count of scheduled work.
Do not claim experiment completeness without reconciling the intended matrix.

## Graded artifacts and quality

`score` retains evaluator-owned source/test digests, a nonempty pristine result,
every declared mutant and restored-code controls. The scorer archives the exact
transferred tests before grading; `final_tests_sha256` binds that archive rather
than copying a later agent-checkout state. Missing archive evidence cannot earn
verified-quality credit. Top-level harness errors remain failures even when a
nested score appears internally consistent.

The core report v3 distinguishes historical reported outcomes from quality
satisfying the execution protocol. `tokens_per_verified_success` and
`cost_usd_per_verified_success` include all attempt spend, including failures,
and are null for unknown spend or no verified completed successes. Matched
quality coverage remains separate from matched token coverage. Descriptive
ratios, point estimates and Wilson intervals do not establish a causal benefit.

`scoring_wall_s` measures the grading call separately from agent execution.
`evaluation_wall_s` is their sum, not total startup, artifact flush, cleanup or
billing duration. Legacy missing timing and cache components stay unknown.
Historical receipts and generated tables retain their original bytes; reading
them through the current report does not requalify their execution evidence.

## Cheap verification before a paid experiment

From the repository root:

```sh
python3 -I benchmarks/agentic/cohort/harness/selftest.py --require-native
cargo test -p xtask --test agent_cohort_selftest
```

Both paths use deterministic instrument tests and tiny offline Cargo fixtures,
not a provider. The strict selftest requires all six native controls and zero
skips; a missing tool is not a passing result. Collection-boundary tests stub
the external model process and establish no live provider compatibility.

Before an actual cohort, freeze task/model/binary/protocol identities, qualify
reference suites, reconcile the intended attempts and predeclare quality and
budget criteria. Harder representative tasks, randomized order, provider breadth,
full agent-visible output accounting and baseline/scoreboard integration remain
separate work under the parent issues. Do not infer usefulness from compression
alone or spend on a model cohort to diagnose deterministic instrument defects.
