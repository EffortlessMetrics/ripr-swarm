# Agent cohort evaluation core (#12902, #12912)

Question: do agents given RIPR produce better test outcomes with less total effort?
This directory contains the scoring and receipt-accounting core. The opt-in agent
collection tools, task catalog and historical cohorts are the separate residual
of #13071; this core neither runs a provider nor claims measured token savings.

## Use the instrument

From the repository root, with Python 3.11+ and Git:

```sh
python3 -I benchmarks/agentic/cohort/harness/selftest.py
python3 benchmarks/agentic/cohort/harness/scoreboard.py report receipts.jsonl
```

The report command reads supplied receipts and writes `report.json` and
`report.md`. Missing measurements remain unknown. Historical reported quality
is not silently promoted to verified runtime quality.

The native test route requires Cargo and refuses skips, missing native controls
or an empty test suite:

```sh
python3 -I benchmarks/agentic/cohort/harness/selftest.py --require-native
cargo test -p xtask --test agent_cohort_selftest
```

Six named native controls cover exact-boundary detection, an insufficient smoke
test, zero tests, compiler rejection, abnormal process exit and custom manifests
inside an enclosing Cargo workspace. Local discovery explicitly skips native
controls when Cargo is absent; the Rust-owned route cannot count that as a pass.
These controls use tiny dependency-free crates and `--offline`, not model calls.

## Evidence boundary

Read [the core contract](CORE_CONTRACT.md) for scoring, accounting and comparison
semantics. Execute only trusted tasks in disposable secret-free environments:
source reconstruction is not a hostile-code sandbox. The instrument verifies
receipt consistency, not external authenticity, causal benefit or general test
adequacy. The broader multi-provider experiment remains open under #12902.
