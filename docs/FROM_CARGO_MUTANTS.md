# Coming from cargo-mutants

You already run cargo-mutants and want to know what to use in ripr, and where
it differs. Short version: ripr does not run mutants. It reads the diff and
tells you, in seconds, which changed lines have no test that would notice a
wrong value. Use it as the fast inner loop; keep cargo-mutants as the
occasional runtime check where the cost is worth it.

| | cargo-mutants | ripr |
| --- | --- | --- |
| Question | Does a test fail when this mutant runs? | Does a test appear to contain a discriminator for this change? |
| Method | Builds and runs the suite per mutant | Static: reads the diff, tests and assertions |
| Cost | Minutes to hours | Seconds on the diff |
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

| cargo-mutants (`.cargo/mutants.toml`) | ripr |
| --- | --- |
| `exclude_globs` / `examine_globs` | `--exclude` / `--path` flags ([Configuration](CONFIGURATION.md)) |
| `exclude_re` / `examine_re` | none |
| `additional_cargo_test_args`, `test_tool` | not applicable |
| `#[mutants::skip]` | `.ripr/suppressions.toml` ([example](suppressions.example.toml)); applied to findings with `--suppression-policy` |

`ripr init` writes a `ripr.toml` with the built-in defaults. It is optional.

## CI map

| cargo-mutants CI habit | ripr equivalent |
| --- | --- |
| `cargo mutants --in-diff pr.diff` on each PR, fail on exit 2 (missed) | `ripr init --ci github`, then follow [Adopting in CI](ADOPTING_IN_CI.md): advisory first, then `RIPR_GATE_MODE=baseline-check` to block only new gaps |
| Upload `mutants.out` as an artifact | the generated workflow uploads the reports under `target/ripr/reports` and can render SARIF |
| Nightly full run | `ripr pilot` for a whole-repo ranked list |
| Exit code | `ripr check` always exits `0` when it completes; the pass/fail decision is `ripr gate evaluate` (exit `3` when blocked). See [Exit codes](EXIT_CODES.md). |

Do not build a gate on `ripr check`'s exit code.

## Output map

| `mutants.out/` | ripr |
| --- | --- |
| `missed.txt` (one mutant per line) | findings with `weakly_exposed`, `reachable_unrevealed` or `no_static_path`, each with the changed line, the missing discriminator and a next step. `ripr check` human output, `--json`, or `ripr agent brief --json` for a bounded fix list |
| `caught.txt` | `exposed` findings (a strong discriminator was found) |
| `unviable.txt` | no equivalent (nothing is compiled) |
| `timeout.txt` | the `analysis_timeout` refusal (never a clean pass) |
| `outcomes.json`, `mutants.json` | `ripr check --json`; `--format repo-exposure-json` for a repo snapshot |
| `diff/` per-mutant patches | none; ripr describes the probe, it does not apply it |
| `log/` | `--quiet` controls stderr progress |

## Using both

ripr can import a cargo-mutants run and show where the two disagree:

```bash
cargo mutants --in-diff pr.diff
ripr check --diff pr.diff --format repo-exposure-json > exposure.json
ripr calibrate cargo-mutants --mutants-json mutants.out --repo-exposure-json exposure.json
```

The import is advisory and does not change classifications
([Integrations](INTEGRATIONS.md#cargo-mutants)).

## Where you will get stuck

- **No fail-on-missed flag.** `ripr check` never fails on findings. Use the gate.
- **No mutant list.** There is no one-line-per-mutant `missed.txt` view and no
  mutant-level regex selection. You get probes grouped by changed line.
- **Static, not exhaustive.** ripr can say `static_unknown`. Treat that, and any
  `exposed`, as a claim about what tests appear to check, not proof a mutant dies.
  Measured agreement with real mutants is tracked in the
  [replacement-readiness work](https://github.com/EffortlessMetrics/ripr-swarm/issues/12901);
  until it meets its criterion, keep cargo-mutants for the changes where a
  missed mutant would be expensive.
- **Languages.** Rust is the mature path. Python and TypeScript are preview; see
  [Capability matrix](CAPABILITY_MATRIX.md).
- **Mutation operators.** ripr's probe families are not cargo-mutants' operator
  list, so counts will not line up one to one.
