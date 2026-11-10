# Coming from cargo-mutants

You already run cargo-mutants and want to know what to use in ripr, and where
it differs. Short version: ripr does not run mutants. `ripr check` reads the
diff and reports changed lines where it cannot find a test that appears to
notice a wrong value. That is static evidence, not a runtime result. Use it as
the fast inner loop; keep cargo-mutants as the occasional runtime check where
the cost is worth it.

| | cargo-mutants | ripr |
| --- | --- | --- |
| Question | Does a test fail when this mutant runs? | Does a test appear to contain a discriminator for this change? |
| Method | Builds and runs the suite per mutant | Static: reads the diff, tests and assertions |
| Cost | Minutes to hours | Under a second on a small crate; 38-53 s and about 1.2 GB for `ripr check` on a two-file diff in a 2,500-file workspace (measured 2026-11-08) |
| Verdict | `caught` / `missed` / `unviable` / `timeout` | `exposed` / `weakly_exposed` / `reachable_unrevealed` / `no_static_path` / `infection_unknown` / `propagation_unknown` / `static_unknown` |
| Authority | Runtime | Advisory; never claims a mutant was killed |

A ripr finding is a prompt to write or strengthen a test. It is not a runtime
result. See [Why ripr](WHY_RIPR.md).

## Command map

| You run | In ripr | Notes |
| --- | --- | --- |
| `cargo mutants` | `ripr check` | Diff against the default base, no run. Whole repo: `ripr pilot`. |
| `cargo mutants --in-diff pr.diff` | `ripr check --diff pr.diff` | `--diff -` reads stdin. Without `--diff`, `--base REV` is diffed for you (default: the first of `origin/HEAD`, `origin/main`, `origin/master`, `main`, `master` that exists). |
| `git diff main \| cargo mutants --in-diff /dev/stdin` | `ripr check --base main` | Includes uncommitted edits by default; `--committed` limits to history. |
| `--file src/foo.rs` / `-f` | `--path src/foo.rs` | Repeatable; file, directory or glob. |
| `--exclude 'src/gen/**'` / `-e` | `--exclude 'src/gen/**'` | Repeatable; wins over `--path`. |
| `-p <pkg>` / `--package` | `--path crates/<pkg>` | There is no `-p`; select the package by its directory. Running from inside a member still analyzes the whole workspace. |
| `--workspace` | `ripr check` | The default considers every changed file in the workspace; configured exclusions and analysis budgets can leave disclosed omissions or a partial result. |
| `--re REGEX` / `--exclude-re REGEX` | none | Selection is by file path and by diff, not by mutant name. |
| `--list` / `--list-files` | `ripr check --format json` | Lists probes and findings without any test run (the only mode). |
| `--check` (compile only) | not applicable | Nothing is built. |
| `--json` | `ripr check --json` | Schema in [Output schema](OUTPUT_SCHEMA.md). |
| `--jobs`, `--shard k/n`, `--timeout`, `--minimum-test-timeout` | `--timeout SECS` | One bounded analysis; no sharding or per-mutant timeouts. |
| `--baseline run/skip` | `ripr baseline create` / `diff` | Different meaning: ripr baselines reviewed *gaps*, not an unmutated test run. |
| `--in-place`, `--cargo-arg`, `--test-tool` | not applicable | ripr never executes your tests. |
| `mutants.out/` | `target/ripr/` (`ripr pilot`, `ripr agent ...`) | See Output map. |
| `cargo mutants` against one function after a fix | `ripr agent verify` / `ripr outcome --before --after` | Re-run on the diff and compare. |

## Config map

ripr does not read `.cargo/mutants.toml`. Carry its settings over by hand:

| cargo-mutants (`.cargo/mutants.toml`) | ripr |
| --- | --- |
| `exclude_globs` / `examine_globs` | `--exclude` / `--path` flags ([Configuration](CONFIGURATION.md)) |
| `exclude_re` / `examine_re` | none |
| `additional_cargo_test_args`, `test_tool` | not applicable |
| `#[mutants::skip]` | honored as-is on a changed function or method, including `#[cfg_attr(<predicate>, mutants::skip)]`: `ripr check` withholds that owner's findings and discloses them (count and `owner_mutants_skip`; see [Output schema](OUTPUT_SCHEMA.md#owner_opt_outs-and-withheld-findings-top-level-additive-13819)). A skip on an `impl` block or module is not read; use `.ripr/suppressions.toml` ([example](suppressions.example.toml)) with `--suppression-policy` for those |

`ripr init` writes a `ripr.toml` with the built-in defaults. It is optional.

## CI map

| cargo-mutants CI habit | ripr equivalent |
| --- | --- |
| `cargo mutants --in-diff pr.diff` on each PR, fail on exit 2 (missed) | `ripr gate check --base origin/main` (exit `3` when blocked; see [CI gate](#ci-gate-the-exit-2-equivalent)), or `ripr init --ci github` and [Adopting in CI](ADOPTING_IN_CI.md) for the full workflow |
| Upload `mutants.out` as an artifact | the generated workflow uploads the reports under `target/ripr/reports` and can render SARIF |
| Nightly full run | `ripr pilot` for a whole-repo ranked list |
| Exit code | `ripr check` always exits `0` when it completes; the pass/fail decision is `ripr gate check` or `ripr gate evaluate` (exit `3` when blocked). See [Exit codes](EXIT_CODES.md). |

Do not build a gate on `ripr check`'s exit code.

## Output map

| `mutants.out/` | ripr |
| --- | --- |
| `missed.txt` (one mutant per line) | `ripr check --format missed-txt`: one sorted line per gap (see below). It lists every actionable class except `exposed`: `weakly_exposed`, `reachable_unrevealed`, `no_static_path` and the three `*_unknown` classes. Otherwise use the findings, each with the changed line, the missing discriminator and a next step. `ripr check` human output, `--json`, or `ripr agent brief --json` for a bounded fix list |
| `caught.txt` | `exposed` findings (a strong discriminator was found) |
| `unviable.txt` | no equivalent (nothing is compiled) |
| `timeout.txt` | the `analysis_timeout` refusal (never a clean pass) |
| `outcomes.json`, `mutants.json` | `ripr check --json`; `--format repo-exposure-json` for a repo snapshot |
| `diff/` per-mutant patches | none; ripr describes the probe, it does not apply it |
| `log/` | `--quiet` controls stderr progress |

## `missed.txt` → `--format missed-txt`

`--format missed-txt` ships after ripr 0.10.0; older releases reject it as an
unknown format.

```bash
ripr check --base origin/main --format missed-txt
```

Observed on a small crate where `n > 10` became `n >= 10` and the only test
is `assert!(is_large(50))`:

```text
src/lib.rs:2:1: weakly_exposed predicate: n >= 10 -> n == 10
src/lib.rs:2:1: weakly_exposed return_value: n >= 10 -> n == 10
```

Each line is `path:line:col: <classification> <probe family>: <expression>
-> <missing discriminator>`, sorted. `exposed` findings print nothing; after
adding a test that asserts `is_large(10)` and `!is_large(9)`, the same
command printed zero bytes and both findings were `exposed`.

Two rules for scripts that read this file:

- A line starting with `# ` is a comment, never a gap. ripr writes
  `# ripr: ...` comments when the run is incomplete, when sources are
  uncommitted, or when findings were left out. Skip them when listing gaps,
  but do not treat comment-only output as clean: only zero bytes means a
  fully analyzed run with nothing to list.
- `ripr check` exits `0` whether or not it found gaps. A non-zero exit means
  the analysis did not complete. Do not gate on `check`'s exit code.

Full field and selection rules: [output schema, `missed-txt`](OUTPUT_SCHEMA.md#missed-txt-sorted-gap-lines).

## CI gate: the exit `2` equivalent

One command:

```bash
ripr gate check --base origin/main
```

It runs the two steps below in one process, defaults to `--mode
acknowledgeable` and exits like `gate evaluate`. If the first step fails,
gate check does not reuse an older `comments.json`; it exits `2`. Run the
steps yourself when you want to keep the guidance from one run and
evaluate it later:

```bash
ripr review-comments --base origin/main --head HEAD
ripr gate evaluate --pr-guidance target/ripr/review/comments.json --mode acknowledgeable
```

| `gate check` / `gate evaluate` exit | Meaning |
| --- | --- |
| `0` | pass, advisory only, or every blocking gap acknowledged |
| `3` | blocked: at least one policy-eligible gap and no acknowledgement |
| `2` | could not complete (missing input, misconfiguration); fail closed |

The gate writes `target/ripr/reports/gate-decision.json` and
`gate-decision.md` before it exits; the Markdown names each blocking gap and
the test to add. See [exit codes](EXIT_CODES.md).

What it blocks: gaps that are policy-eligible and have a complete, bounded
repair route. It does not block every finding. In the example above
`missed-txt` listed two lines, while the gate reported one blocking gap and
kept the other advisory (`incomplete_repair_route`). A gap stays advisory, for
instance, when ripr cannot name a repair or when a test next to it changed in
the same pull request. `gate-decision.md` states the reason for each one.

### Observed

Run with a development build of `ripr 0.11.0` on the example crate, with a
bare clone as `origin`:

| Case | `review-comments` | `gate evaluate` |
| --- | --- | --- |
| `n >= 10`, only `assert!(is_large(50))` | `0` | `3`, `Decision: blocked`, `Blocking: 1` |
| same, with `--label ripr-waive` | `0` | `0`, `Decision: acknowledged` |
| boundary test added (`is_large(10)`, `!is_large(9)`) | `0` | `0`, `Decision: pass` |
| no `origin` remote | `2` (base does not resolve) | `2`, `config_error` (guidance not found) |

### Prerequisites

- Full history and the base ref: check out with `fetch-depth: 0` so
  `origin/main` (or `origin/$GITHUB_BASE_REF`) resolves. Without it,
  `review-comments` exits `2` and the gate then exits `2`; it never passes on
  missing input.
- The `ripr-waive` label exists in the repository, so a reviewer can
  acknowledge a gap for one pull request. The gap stays visible in the report.
- Large workspaces: `review-comments` refuses with `review_guidance_oversized`
  (exit `2`, so the gate also exits `2`) when its index input passes about
  1,200 files (the workspace corpus plus changed files it reads for
  attribution). Raise `RIPR_REVIEW_GUIDANCE_MAX_INDEX_FILES` on a runner with
  enough memory; the limit is an input cap, not a memory bound, and its memory
  cost on large workspaces is not measured here. Separately, `ripr check`
  can return a partial result when the diff exceeds the 200-file or
  1,000-changed-line budget. Raise `RIPR_PARTIAL_DIFF_FILE_BUDGET` and/or
  `RIPR_PARTIAL_DIFF_LINE_BUDGET` as needed. The effective line budget is
  capped by `RIPR_MAX_DIFF_CHANGED_RUST_LINES` (2,000 by default), and the
  first selected file is analyzed whole even when it alone exceeds the line
  budget. See [Configuration](CONFIGURATION.md).

### Minimal GitHub Actions steps

```yaml
on:
  pull_request:
    types: [opened, synchronize, reopened, labeled, unlabeled]

permissions:
  contents: read

jobs:
  ripr:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0
      # install ripr here
      - name: ripr gate
        env:
          BASE_REF: ${{ github.base_ref }}
          LABELS: ${{ toJSON(github.event.pull_request.labels.*.name) }}
        run: |
          mkdir -p target/ci
          printf '%s' "$LABELS" > target/ci/labels.json
          ripr gate check --base "origin/$BASE_REF" \
            --labels-json target/ci/labels.json
```

`labeled` and `unlabeled` rerun the job when someone adds or removes
`ripr-waive`. Passing labels as a file through `env` keeps label text out of
the shell script. With the label absent, the observed exit was `3`; with
`["bug", "ripr-waive"]` it was `0`.

For the full workflow (pinned install, job summary, SARIF, artifacts, mode
switching through repository variables), generate it instead:

```bash
ripr init --ci github
```

and follow [adopting ripr in CI](ADOPTING_IN_CI.md). Until ripr 0.11.0 is
published, a development build generates a workflow that installs 0.10.0,
which lacks commands the workflow runs; `ripr init` says so on stderr. Use the
minimal steps above until then. That route starts
advisory and moves to `acknowledgeable` or `baseline-check` through
`RIPR_GATE_MODE`.

## Using both

ripr can import a cargo-mutants run and show where the two disagree:

```bash
cargo mutants --in-diff pr.diff
ripr check --format repo-exposure-json > exposure.json
ripr calibrate cargo-mutants --mutants-json mutants.out --repo-exposure-json exposure.json
```

The exposure snapshot covers the whole repository, not just the diff, so it
costs more than a plain `ripr check`. The import is advisory and does not change classifications
([Integrations](INTEGRATIONS.md#cargo-mutants)).

## Where you will get stuck

- **No fail-on-missed flag.** `ripr check` never fails on findings. Use the
  one-command gate above, `ripr gate check`.
- **Gaps, not mutants.** `--format missed-txt` lists one line per gap (a probe
  on a changed line), not per mutant, and there is no mutant-level regex
  selection.
- **Static, not exhaustive.** ripr can say `static_unknown`. Treat that, and any
  `exposed`, as a claim about what tests appear to check, not proof a mutant dies.
  Measured agreement with real mutants is tracked in the
  [replacement-readiness work](https://github.com/EffortlessMetrics/ripr-swarm/issues/12901);
  until it meets its criterion, keep cargo-mutants for the changes where a
  missed mutant would be expensive.
- **Languages.** Rust is the mature path. Python and TypeScript are preview; see
  [Capability matrix](CAPABILITY_MATRIX.md).
- **Two vocabularies.** `ripr check` reports `weakly_exposed` /
  `no_static_path`; the gate, `pilot` and calibration describe the same
  evidence as seams that are `weakly_gripped` / `ungripped`
  ([mapping](OUTPUT_SCHEMA.md)).
- **Gate gaps being fixed.** Known cases where the gate is more lenient than
  `check`: an untested changed function can be left out of review guidance
  ([#13892](https://github.com/EffortlessMetrics/ripr-swarm/issues/13892));
  a changed test anywhere in the same file keeps a gap advisory
  ([#13893](https://github.com/EffortlessMetrics/ripr-swarm/issues/13893));
  `--suppression-policy` does not reach the gate
  ([#13895](https://github.com/EffortlessMetrics/ripr-swarm/issues/13895)).
  Until those land, read `missed-txt` alongside the gate decision.
- **Exclusions read as incomplete.** A changed file left out of the analysis
  currently marks the run `partial_with_limitations`, so `missed-txt` prints a
  `# ripr:` comment even on a clean change. That happens when a
  `--path`/`--exclude` scope leaves it out, and also when ripr skips it as
  generated (`generated_file_patterns`, or names such as `generated.rs` or
  `src/gen/`), even with no flags
  ([#13894](https://github.com/EffortlessMetrics/ripr-swarm/issues/13894)).
- **Mutation operators.** ripr's probe families are not cargo-mutants' operator
  list, so counts will not line up one to one.
