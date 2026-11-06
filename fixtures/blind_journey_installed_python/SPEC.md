# Installed Python Journey Fixture

Spec: RIPR-SPEC-0209

## Given

The 0.11 installed Python repair journey (#4518, parents #4508/#4600,
contract #4603/RIPR-SPEC-0200, executor #4604/RIPR-SPEC-0205, sibling pattern
#4516/RIPR-SPEC-0207) needs one retained, bounded Python fixture whose literal
scripted journey the deterministic executor can stamp and validate offline, so
the language row is executable through the gate before the frozen #1609
candidate and the #4510 harness make the real installed run terminal.

This fixture commits one fresh flat-layout package repository
(`fixtures/blind_journey_installed_python/repository/`): `pricing/core.py`
owns one bulk-discount boundary predicate, `pricing/__init__.py` exports it,
`tests/test_pricing.py` is the single related test, and `pyproject.toml`
selects the pytest environment. Two bound snapshots are retained:

- `base` — the boundary is exclusive (`units > 20`); the test covers
  quantities 19 and 21 only;
- `head` — the committed production change makes the boundary inclusive
  (`units >= 20`); the test is unchanged.

The package is never installed editable and carries no path-repair conftest,
so the environment-binding control discriminates in the clean shape: run from
`<selected-root>`, `python -m pytest tests/test_pricing.py` adds the project
root (the current working directory) to `sys.path` and `import pricing`
resolves, while the bare `pytest tests/test_pricing.py` console script does
not add the project root and fails with an import error. Every snapshot file
is bound by SHA-256 in `manifest.json`; the
manifest also records the exact, reproducible git commit/tree identities
(pinned author, fixed timestamps, `core.autocrlf=false`) and the substitution
contract for the printed command templates. The expected repair is
independently understandable from source — one equality assertion
`bulk_discount_rate(20) == 0.15` — and requires no production edit.

## When

`cargo xtask check-fixture-contracts` validates this fixture: the manifest
parses at `blind_journey_installed_python_fixture.v1`, every recorded snapshot
digest matches the bytes on disk, every recorded git identity is well-formed,
the selected repair names exactly one edit target inside the expected edit
cage, the focused verification command is the documented module form while
the bare form is recorded as a historical artifact at documented strength,
and every scripted scenario id named by the manifest exists in the
RIPR-SPEC-0205 executor corpus. `cargo xtask blind-journey-execute` then runs
the eleven scripted installed-Python scenarios through the real executor;
each emitted packet is stamped and must be accepted by the live
RIPR-SPEC-0200 validator, so a hand-edited expectation cannot make a wrong
journey emit.

The positive row follows only public docs/help and literal product-emitted
commands — `ripr doctor`, `ripr pilot`, one canonical item selection, the
printed `ripr agent repair --phase before`, one bounded test edit, the exact
printed module-form `python -m pytest tests/test_pricing.py` between the
phases, the printed `--phase after`, and the printed receipt route —
recorded with the `ripr` invocations running from a foreign launch directory
against the selected root (bound through `--root`), while the printed
module-form verification command runs from `<selected-root>`, with the
decoy-write watchdog observing no matching writes outside the selected root.

## Then

The separate actual consumer at
`xtask/src/reports/release/candidate_harness/python_journey.rs` materializes
these inputs through private `InstalledCandidate` getters, reproduces the
pinned base/head objects, then commits the public `.gitignore` prerequisite
as a separately identified operator preparation. It follows the live
gap-bound start and continuation from a foreign decoy repository. The live
card requests `StrengthenExistingTest`, so the operator adds the missing
equality assertion to the exact suggested existing test; the printed pytest
selector remains unchanged. This differs from the historical scripted row's
new test name and is recorded explicitly.

Its opt-in workspace test requires a caller-supplied built binary and an
already activated Python environment containing actual pytest. The consumer
never installs dependencies, activates an environment or repairs import paths.
It records runtime denominators and wrong-boundary/restoration observations
separately from static closure, the static attempt receipt and first-pr's
caller-declared status receipts. The gap-bound packet currently carries
verification strings, with no typed runtime command spec. Public
`agent verify-execute` admits only static `ripr agent verify`; the separately
governed Python verify phase requires an accepted repair-trust selection that
this consumer does not create. These facts leave authenticated project-run
receipt authority unestablished.

The production entry accepts only an admitted installed handle; a workspace
replay and a synthetic-package custody control cannot qualify an immutable
release candidate. The actual consumer reports preparation only until the
central candidate and final language matrix exist. The scripted rows below
retain their historical proof scope.

- The positive row and the historical-bare-form compatibility row derive
  `passed_blind_journey`; the interrupted-receipt row recovers through a
  clean rerun and derives `passed_blind_journey` with no half-authoritative
  receipt.
- The bare-command row keeps the import failure visible as
  `verification_failure_visible` with static movement on its own axis; the
  no-environment row stops before any edit with the precise printed
  prerequisite (`verification_not_run_visible`); the skipped-verification row
  keeps verification `not_run` despite static movement; the receipt-path
  mismatch and the deleted before-artifact rows stop honestly
  (`honest_limitation`) naming the printed recovery; the production edit
  derives `unsafe_or_wrong_edit`; the planted decoy interpreter or workspace
  candidate derives `candidate_identity_failure`.
- Every negative control keeps its own non-positive terminal result; no
  aggregate converts them into success.

## Must Not

- Do not run a candidate, launch a process, or decide a release verdict.
- Do not record any production edit as admitted: `pricing/core.py`,
  `pricing/__init__.py` and `pyproject.toml` stay forbidden in every
  positive row.
- Do not silently repair PATH/PYTHONPATH or activate an environment: the
  no-environment row terminates with the printed prerequisite.
- Do not let static movement, project verification, or receipt issuance
  imply one another; the evidence axes stay separate in every scenario.
- Keep the pinned static vocabulary only; this fixture records no dynamic or
  mutation-testing verdicts and no coverage claims.
- Do not count any scripted executor success as installed usefulness, blind
  qualification, candidate selection or parent acceptance.
