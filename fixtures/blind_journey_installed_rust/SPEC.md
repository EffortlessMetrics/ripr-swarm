# Installed Rust Journey Fixture

Spec: RIPR-SPEC-0207

## Given

The 0.11 installed Rust repair journey (#4516, parents #4508/#4600, contract
#4603/RIPR-SPEC-0200, executor #4604/RIPR-SPEC-0205) needs one retained,
bounded Cargo fixture whose literal scripted journey the deterministic executor
can stamp and validate offline, so the language row is executable through the
gate before the frozen #1609 candidate and the #4510 harness make the real
installed run terminal.

This fixture commits one fresh two-source Cargo repository
(`fixtures/blind_journey_installed_rust/repository/`): `src/lib.rs` owns one
loyalty-tier predicate and `tests/tier_boundary.rs` is the single related
test. Three bound snapshots are retained:

- `base` — the boundary is exclusive (`quantity > 20`); the test covers
  quantities 19 and 21 only;
- `head` — the committed production change makes the boundary inclusive
  (`quantity >= 20`) and adds `/target/` to `.gitignore`;
- `head-no-ignore` — the same head production change without any `target/`
  ignore rule.

Every snapshot file is bound by SHA-256 in `manifest.json`; the manifest also
records the exact, reproducible git commit/tree identities (pinned author,
fixed timestamps, `core.autocrlf=false`) and the substitution contract for the
printed command templates. The expected repair is independently understandable
from source — one equality assertion `tier(20) == "gold"` — and requires no
production edit.

The no-ignore identity is explicitly corrected to
`7eaa951e85de8ede45441272f06fb99327440416`, retaining tree
`36e9bee8d075276b344af3389b3ad76510b2f930` and sole parent
`01821f774cb2cc2bc745b7506a3941312456fb4e`. Both author and committer are
`ripr fixture <fixture@example.invalid>` at `2026-10-01T00:02:00Z`
(`1790812920 +0000`); the message is `variant: no-ignore` with one final LF.
The manifest's `variant_construction_recipe` records every field. This is new
canonical metadata. The original `7831d6f6e31fe47fad83d204f8644bd9618f2bb4`
remains in `historical_unreproduced_commits`, linked to the original source:
its exact message and unambiguous dates were not retained. It is not an active
snapshot or recovered history. The base/head positive identities and all
snapshot bytes are unchanged.

## When

`cargo xtask check-fixture-contracts` validates this fixture: the manifest
parses at `blind_journey_installed_rust_fixture.v1`, every recorded snapshot
digest matches the bytes on disk, every active git commit/tree identity is
reconstructed from the retained bytes and complete recipe,
the selected repair names exactly one edit target inside the expected edit
cage, and every scripted scenario id named by the manifest exists in the
RIPR-SPEC-0205 executor corpus. `cargo xtask blind-journey-execute` then runs
the ten scripted installed-Rust scenarios through the real executor; each
emitted packet is stamped and must be accepted by the live RIPR-SPEC-0200
validator, so a hand-edited expectation cannot make a wrong journey emit.

Reconstruction uses SHA-1 repositories, exact bytes and regular `100644` files,
`core.autocrlf=false`, verbatim message cleanup, pinned identities/dates and
no host templates, hooks, signing, global/system configuration or external
ignore rules. Missing variant metadata, a changed parent, or a forged
commit/tree identity must fail the fixture gate.

The positive row follows only public docs/help and literal product-emitted
commands — `ripr doctor`, `ripr pilot`, one canonical item selection, the
printed `ripr agent repair --phase before`, one bounded test edit, the exact
printed focused `cargo test --test tier_boundary` between the phases, the
printed `--phase after`, and the printed receipt route — recorded from a
foreign launch directory against the selected root, with the ordinary Cargo
build output admitted under the accepted `ignored_build_output` cage policy.

## Then

- The positive row and the second-root repeat derive `passed_blind_journey`
  and share one portable identity (concrete root spelling is retained
  evidence, never identity).
- The no-ignore-rule repository stops before any edit with the precise
  printed precondition; the skipped-verification and deleted-artifact rows
  keep verification `not_run` / honest-limitation respectively; the
  non-artifact `target/notes.rs` edit and the production edit each derive
  `unsafe_or_wrong_edit`; the failing focused test derives
  `verification_failure_visible` while static movement stays a separate axis;
  the planted workspace binary derives `candidate_identity_failure`; the
  interrupted receipt write recovers through a clean rerun and never yields a
  half-authoritative receipt.
- Every negative control keeps its own non-positive terminal result; no
  aggregate converts them into success.

## Must Not

- Do not run a candidate, launch a process, or decide a release verdict.
- Do not record any production edit as admitted: `src/lib.rs`, `Cargo.toml`
  and `target/notes.rs` stay forbidden in every positive row.
- Do not let static movement, project verification, or receipt issuance
  imply one another; the evidence axes stay separate in every scenario.
- Keep the pinned static vocabulary only; this fixture records no dynamic or
  mutation-testing verdicts and no coverage claims.
- Do not count any scripted executor success as installed usefulness, blind
  qualification, candidate selection or parent acceptance.
