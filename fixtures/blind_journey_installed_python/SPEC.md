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

## Optional real-runtime preparation control

The ordinary installed-language integration tests use shell stand-ins for
deterministic argv/CWD checks. A separate ignored test runs real pytest with
the workspace-built binary. Supply an isolated Python environment containing
pytest and its console script on PATH, then run:

```bash
cargo test --locked -p ripr --features lang-python \
  --test installed_python_typescript_journeys \
  python_real_pytest_replays_printed_verify_and_discriminates_boundary \
  -- --ignored --exact --nocapture
```

The test installs nothing and does not activate an environment. Supply Python
and the pytest console script from the same isolated environment's executable
directory. Their observed paths, the console shebang, module location and
versions are recorded; different executable directories or versions refuse the
comparison. This routes the comparison to one environment; it does not
authenticate custom wrapper contents or SDK bytes. Missing Python,
pytest, or its console script fails this explicitly requested control; default
test execution reports it ignored. It removes inherited Python path overrides
and pytest plugin/options overrides, disables plugin autoload and bytecode
writes, clears child Python/pytest shell functions and disables BASH_ENV, and
otherwise uses the caller's PATH. It records interpreter and pytest
identity, replays the exact printed module-form action from a foreign CWD,
requires a nonzero passing selector, and checks that the bare console-script
replacement fails to import the fixture package. In the disposable fixture it
then adds the equality assertion, requires it to pass, rejects the exclusive
boundary with an assertion failure, restores the source and reruns successfully.
The intentional wrong-source experiment is outside the repair edit cage and is
never an admitted edit. The decoy repository must remain untouched.

The product check and runtime subprocesses in this optional control reuse
`common/boundary_gap.rs::run_program_recorded`:
each has a 30-second deadline, a combined 4 MiB capture-admission limit and
isolated process ownership. Termination and primary reap occur before the
shared runner returns a terminal result. A separate default Linux control
deliberately hangs a shell with a TERM-ignoring descendant, requires the
deadline refusal, inspects the shared terminal record and verifies primary
reap and descendant stoppage before fixture cleanup. A default metadata control
rejects a console script selected from another environment directory. Run them
without a Python prerequisite using the `python_runtime_` test filter.

This is fixture/runtime and workspace-command preparation evidence. It does
not connect the installed archive custody handle to a language journey, record
static closure or a receipt, qualify an immutable candidate, or supply a blind
operator observation. Those remain separate downstream obligations.

## Must Not

- The scripted executor must not launch candidate processes or decide a release
  verdict. The optional preparation control above executes workspace-built
  commands only and supplies no candidate qualification.
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
