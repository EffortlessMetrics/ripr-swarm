# #8935 isolated public-literal compatibility contribution

The contribution restores the original four-field public `ValueFact` Rust literal while retaining private file-aware deduplication and deterministic ordering. It is for adoption by the existing #8935 writer; it does not authorize merging or replace that writer's branch.

Accepted contribution route: https://github.com/EffortlessMetrics/ripr-swarm/pull/8935#issuecomment-6306240343

## Source identity and scope

Base: `4c89840f1c30162c883f8e9b35ef08dbe9a1cc3c` (tree `e5cbf84edfee6f47c7e7fd590d235e3e6df75a7f`). Tested source: `513406bfc4d1f85698366c6a2ab7dfe54982d711` (tree `75473e05c0c71d0ec6aa57cdb7d62d23aa43ef73`). Main observed at `a3f34511caa41d41e78ade59ffcc8644f60e02bd` was supplied as integration context; a fresh remote read subsequently observed `9911925ad6267bb3929be323cbcfdbe6da18f613`. Neither main state was adopted or used to restack this contribution. The original PR remains Draft at 4c. No original writer branch was changed. #8838 collision identity migration and #8842 normalization remain separate.

Public ValueFact again has exactly `line`, `text`, `value`, `context`. Finding and ActivationEvidence required fields are unchanged. Private LocatedValueFact carries file identity through Rust/Python classification, sorting and deduplication, cache and output projection. ValueOrigins receipts capture the final occurrence-aligned vector in one owned projection. SourceLocation routes a bucket; full Probe and complete ordered fact-vector equality validate lookup. Conflicting origins remain sticky ambiguous. No short ID, token/line reconstruction or public required-field relocation is used. The additive CheckOutput::source_value_files accessor returns validated context or None. Fresh explain uses the owning context; detached DTOs use bare lines.

Explicit cache wire encodings and generations (full 1.86, sharded/compact 0.92) remain unchanged; known, unknown and non-UTF8 file cases are tested. Documentation and traceability entries describe the DTO/context distinction. The source delta spans 78 files including mechanical constructor/fixture updates; inspect the complete base-to-source diff rather than treating those edits as independent changes.

## Executed proof on tested source

Pinned Rust 1.95.0; commands used --locked where shown, one Cargo build job, owned target directory, debug info disabled. Test groups overlap and must not be summed as unique tests.

| Command | Observed result |
|---|---|
| cargo test --locked -p ripr --test value_fact_literal_compat --test value_fact_witness_file | 5 passed |
| cargo test --locked -p ripr --lib value_origins | 5 passed |
| cargo test --locked -p ripr --lib value_facts | 9 passed |
| cargo test --locked -p ripr --lib observed_value | 23 passed |
| cargo test --locked -p ripr --lib python | 903 passed |

The actual classifier consumer tests cover two files, 12-to-13 totals, human first-eight display, JSON ranked-32 cap, 8/9 and 32/33 boundaries, second-file facts outside the human window, removal controls, guard/term and slice inputs, fresh explain, and rejection of mutated receipt payloads. Context tests cover sticky conflicts in AAB/ABA/BAA orders, same location/short ID with different full probes, malformed slots, clones, unknown slots and rebasing.

External-consumer rustc input (edition 2024):

```rust
use ripr::domain::{ValueContext, ValueFact};
fn main() {
    let fact = ValueFact {
        line: 7,
        text: "assert_eq!(price(10), 1)".to_string(),
        value: "10".to_string(),
        context: ValueContext::FunctionArgument,
    };
    assert_eq!(fact.line, 7);
}
```

Run with `rustc --edition=2024 external_four_fields.rs --extern ripr=<library.rmeta> -L dependency=<target/debug/deps> --emit=metadata`. The unchanged input exits 0 against the successor and exits 1 with E0063, missing field `file`, against frozen exact-4c metadata. Adding only `file: None` makes the latter compile (exit 0), confirming the library/control setup. Initial improperly named metadata caused E0463 and was corrected; that setup failure is not counted as the negative proof. Explicit-status reruns confirmed both positives.

Input SHA256: `024b67faf32c8828e3c6d0a7af479f8d82352affd9d27e7bc82284a0363b50a2`. Frozen exact-4c metadata SHA256: `b1a2ebaeaf8bb482348d5457ee12eb8362db9c7d60ea23c78ca00daf5709810c`. Successor metadata at execution: `7f7f06f3c8d00953f752e692888572892490a7af51326ad92d75c735bfe527ce` (build tools may subsequently overwrite metadata).

A controlled temporary fault on c36 removed only `left.file == right.file` from deduplication. The consumer test compiled and exited 101: first-file 12 passed; expected two-file 13 became 12, human showed 8 of 12 and both JSON arrays had 12. Fault patch SHA256: `6491d2e9585518ebaa1ea0662ea9a49f9bd0df2f27ac0ed8bd76c40cbae44a0a`. The fault was restored before successor commits; fresh successor consumer tests pass. This negative execution belongs to the c36 source epoch, not an assertion of executing a modified 513 tree.

Actual successor-built CLI parametrization control: Python price(x) guard changed from >=5 to >5; test_a parametrizes [10,11], test_b [20,21], both at line 6 with `assert price(x) == 1`. With both files, human reports four correct file-qualified origins and both JSON arrays contain four facts with no file property. Removing B gives two bare-line facts (10,11); 20/21 disappear. Four CLI runs and explicit assertions passed. Binary SHA256 `11b9866963c0c029b956bb187b17c8add3e516e775f2663b24d89c2de346a8bc`, version identifies tested source 513406. Fixture, raw human/JSON outputs and receipt are retained locally.

## Independent review and qualification limits

Independent reviewer `/root/review_8935` reviewed exact source/tree above and complete carried-forward source/API/cache/consumer changes; no actionable findings remained. Reviewer inspected the five passing logs, external positive/negative controls, file-blind patch and actual failure, raw parametrized outputs and binary hash. It did not rerun native tests. Early findings about fresh explain context, receipt scan cost and mutex-held cloning were repaired before this reviewed source.

Selected static gates passed: formatting, traceability, public API, no-panic, allow attributes, process and network policy. Native check-fast on 513 exited 1 solely on Clippy unwrap_used in the new rebase unit test (value_origins.rs:253). Formatting, static-language, command-catalog, CLI consistency, no-panic, allow-attribute and file-policy gates passed first. The test-only successor 5e97141af8748bd47d172691faf552b8197d2834 (tree 540f68a8a48a6024fbfc13da8fb8f54e8433c446) replaces unwrap with a direct mapped Option assertion; missing context still fails and production bytes are unchanged. On exact 5e97141, cargo fmt --all --check passed, and cargo test --locked -p ripr --lib value_origins passed all five tests (successor-context.log). Its native rerun result is recorded below when terminal. The 513 failure is retained as a failure, not overwritten or called a pass. Exact-base check-fast passed independently. Base doctor failed: detached branch first, then named branch solely behind main; that failure was not waived or called a pass. No current-source full precommit/golden suite, hosted matrix or joined-owner qualification is claimed.

Owned local evidence directory: `/workspace/ripr8935-compat-proof`; test logs named qualified-*.log, external inputs and frozen metadata, fault patch/log, parametrization script/receipt. Reproduction source is delivered through the contribution Git commit, not by claiming local binary paths are remote artifacts. Retained source, binaries, metadata, registry and logs; released only 13 GiB of owned disposable incremental artifacts after all test processes were terminal. Final policy build admitted with 26 GiB free, 8 GiB reserve, one job and CARGO_INCREMENTAL=0.

Existing writer must adopt this contribution into #8935, obtain independent review of the exact joined source and satisfy normal required checks. Integration lead retains merge authority and must verify main after any qualified merge. No releases, deployments, permission changes, original-branch writes or merge were performed. GitHub comment content limits remain active; this durable source receipt replaces repeated status posting.

## Parametrized consumer reproduction receipt

For each fixture state run `ripr check --root <fixture> --diff <fixture>/change.diff --mode fast --format human-full --quiet` and the same command with `--format json`. Remove only tests/test_b.py for the second state. Exact sanitized inputs and expected observations follow:

```json
{
  "source_head": "513406bfc4d1f85698366c6a2ab7dfe54982d711",
  "binary_sha256": "11b9866963c0c029b956bb187b17c8add3e516e775f2663b24d89c2de346a8bc",
  "inputs": {
    "ripr.toml": "[languages]\nenabled = [\"python\"]\n",
    "pyproject.toml": "[project]\nname = \"value-origins\"\n",
    "src/pkg/__init__.py": "",
    "src/pkg/mod.py": "def price(x):\n    if x > 5:\n        return 1\n    return 0\n",
    "change.diff": "diff --git a/src/pkg/mod.py b/src/pkg/mod.py\n--- a/src/pkg/mod.py\n+++ b/src/pkg/mod.py\n@@ -2 +2 @@\n-    if x >= 5:\n+    if x > 5:\n",
    "tests/test_b.py": "import pytest\nfrom pkg.mod import price\n\n@pytest.mark.parametrize(\"x\", [20, 21])\ndef test_price(x):\n    assert price(x) == 1\n",
    "tests/test_a.py": "import pytest\nfrom pkg.mod import price\n\n@pytest.mark.parametrize(\"x\", [10, 11])\ndef test_price(x):\n    assert price(x) == 1\n"
  },
  "cases": [
    {
      "name": "two_files",
      "activation_count": 4,
      "promoted_count": 4,
      "human_lines": [
        "- source function argument value x = 10 at tests/test_a.py:6",
        "- source function argument value x = 11 at tests/test_a.py:6",
        "- source function argument value x = 20 at tests/test_b.py:6",
        "- source function argument value x = 21 at tests/test_b.py:6"
      ]
    },
    {
      "name": "removed_b",
      "activation_count": 2,
      "promoted_count": 2,
      "human_lines": [
        "- source function argument value x = 10 at line 6",
        "- source function argument value x = 11 at line 6"
      ]
    }
  ]
}
```

## Owner adoption route

Once published, fetch only the isolated contribution: `git fetch origin contrib/8935-value-fact-compat-4c`; inspect `git diff 4c89840f1c30162c883f8e9b35ef08dbe9a1cc3c FETCH_HEAD` and adopt the contribution commits through the existing writer. Expected source commits before this receipt are c36a709764eba1db01fa2be8f25c7b00eb3c7c1a, 06b30721eac48d9395f217628c6165144b06710c, 513406bfc4d1f85698366c6a2ab7dfe54982d711, and test-only 5e97141af8748bd47d172691faf552b8197d2834. Publication must be verified by remote SHA before claiming delivery.

Immutable reviewed source: https://github.com/EffortlessMetrics/ripr-swarm/commit/5e97141af8748bd47d172691faf552b8197d2834 . Source comparison: https://github.com/EffortlessMetrics/ripr-swarm/compare/4c89840f1c30162c883f8e9b35ef08dbe9a1cc3c...5e97141af8748bd47d172691faf552b8197d2834 . The publication receipt must identify the final proof-only successor SHA; the above commit deliberately identifies the tested source rather than pretending to name its later receipt commit.

## Terminal native qualification

Exact source `5e97141af8748bd47d172691faf552b8197d2834`, tree `540f68a8a48a6024fbfc13da8fb8f54e8433c446`: `cargo xtask check-fast` exited 0. It ran 15 gates over 84 paths selected against the local origin/main merge-base; 78 is the distinct contribution-vs-4c path count. Passed formatting, static-language, command-catalog, CLI-help parity (1 test), no-panic, allow-attributes, file-policy, workspace/all-target Clippy with -D warnings, covered-by, process, network, fixture contracts, generated, generated-clean and lint-policy. Support-tier validation alone was not applicable. Rerun elapsed about 16 minutes; Clippy itself exited 0 after 375476 ms. Local logs `successor-check-fast.log` and `successor-check-fast-report.md` preserve actual output. The earlier H513 check-fast failure is retained separately. This pass belongs to 5e97141, not to the later receipt-only commit.

The receipt-only addition was checked separately with `git diff --cached --check` and the unchanged native xtask `check-static-language` and `check-command-catalog` commands; each exited 0. These documentation checks do not extend the source test execution epoch to the later receipt commit.
