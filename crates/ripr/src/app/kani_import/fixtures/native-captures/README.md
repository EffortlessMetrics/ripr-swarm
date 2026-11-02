# Retained native Kani conformance inputs

This is a bounded #10746 conformance subset for the #10689 importer supplied by
[PR #11387](https://github.com/EffortlessMetrics/ripr-swarm/pull/11387).
It adds cheap offline regression evidence to the existing parser, source-binding
and report owners. It does not close either issue or qualify a support tier.

## Capture identity

The eight `*.native.json` files are untouched exports from clean
[`model-checking/kani@744b3e84d6eb68fd5456826997568666791bd355`](https://github.com/model-checking/kani/tree/744b3e84d6eb68fd5456826997568666791bd355),
schema `0.1.0`, captured on October 31, 2026. The producer used its supported
development build, pinned nightly `2026-10-23`, CBMC `6.11.0`, CaDiCaL and one job
on Linux x64. The printed `0.68.0` version alone does not identify this exporter.

`proof.rs.txt` preserves the exact standalone input bytes; its original filename
was `proof.rs`. The `.txt` suffix marks retained source data, not a Rust module
of ripr. Tests copy these bytes to `proof.rs` in an isolated temporary root.
The file SHA-256 is
`ce3de392ea684b2222522ca2f2a05c4de7ce9c156bbc63ce155cf8967c22876e`.

`provenance.json` is an explicitly authored portable projection of the original
observations. It retains producer/compiler/solver and source commitments, run
selection, timestamps, process exit, raw-export hashes and hashes of the original
observation, binding and log. The original 96-file evidence inventory hash is
`be01b34fcb229b93f3d1081ae6b60355d635996d0ba88882c1c4ce20d09cc5b1`.
Original launcher paths, full logs, build artifacts and authored capture bindings
are outside this subset. Their hashes are references, not a claim that those
files are present here or that execution has been authenticated.

Native diagnostic locations, including original absolute toolchain paths in
unsupported-construct records, remain byte-for-byte intact. They are historical
strings in the imported document, never paths the importer reads. Only the
explicit binding's source inventory governs local source reads.

## Independent interpretation

The expected semantics below were read against the retained source and the
pinned native contract. Rust tests assert literal outcomes and counts; expected
reports are authored, not produced by calling the parser under test.

| Capture | Selected harness | Retained result and required distinction |
| --- | --- | --- |
| success | `b_success` | SUCCESS, two successful checks and one satisfied cover |
| failure | `a_failure` | FAILURE, one failed assertion and one successful check |
| vacuous | `c_vacuous` | SUCCESS, zero successful checks; assertion and cover unreachable |
| expected-panic | `d_expected_panic` | SUCCESS because an assertion fails under expected-panic polarity |
| missing-expected-panic | `e_missing_expected_panic` | FAILURE despite one successful check and zero failed properties |
| bounded-success | `f_bounded_success` | SUCCESS at unwind 3; 31 successful checks and one unreachable check |
| insufficient-bound | `g_insufficient_bound` | FAILURE at unwind 3; one unwind failure and 31 undetermined checks |
| partial | `g_insufficient_bound`, `b_success` | PARTIAL under fail-fast; one failure returned and one selected harness missing |

The ordinary failure asserts a false relation over the same `increment` body;
it is not an implementation-mutant experiment. The loop cases use different
admitted domains. No baseline/mutant/restoration pair, held-out adjudication,
general non-vacuity result or adequacy score is claimed. The vacuous case has
contradictory assumptions (`x < 5` and `x > 10`); its native SUCCESS must remain
visible without implying any successful assertion or reachable cover.

Both loop exports say `is_bounded: false`: that field concerns automatic argument
generation, whereas `resolved_unwind: 3` records their loop bound. The successful
loop also retains two successful unsupported-construct records with native IDs.
Their presence does not mean those checks failed, and these partial identities
do not establish a complete successful-property inventory.

A ninth invocation selected a missing harness and exited 1 without an export.
Its observation is retained in the provenance projection; no JSON result or
binding is fabricated for it. Resource failures, autoharness, contracts, stubs
and code-coverage modes have no genuine capture in this subset.

## Offline consumers and reproduction

`native/tests.rs` checks the native semantic distinctions. Application tests
import all eight exports with newly authored portable bindings, checking exact
source/export commitments, complete document retention and unattested authority.
Those test bindings use `portable-fixture-*` run IDs and a descriptive replay
recipe; they are not the original capture bindings or executed commands.
Source mutation and export substitution under an unchanged binding must refuse.

The vacuous report has an independently authored complete owned-envelope oracle
in `fixtures/evidence-promotion-honesty-corpus/reports/kani-vacuous-capture-unattested.json`
(relative to the repository root). The existing evidence-promotion honesty
corpus rejects static/repair/receipt promotion, and the output test compares this
oracle with a real import while retaining the complete native JSON in Markdown.
Only volatile metadata and the separately checked external payloads are masked.

Normal tests neither install nor execute Kani. For a deliberate fresh native run,
build the exact producer with its locked upstream `build-kani build-dev` route,
copy the source bytes to a file named `proof.rs`, and use a fresh export path:

```text
kani proof.rs -Z export-json --export-json fresh.json --solver cadical --jobs 1 --harness b_success --exact
```

The provenance projection lists each selection tail, including partial-run
`--fail-fast`. Preserve original exports when making another observation and
record the new tool/source/configuration and process identities separately.
Fixture replay is not fresh native execution or authenticated attestation.
