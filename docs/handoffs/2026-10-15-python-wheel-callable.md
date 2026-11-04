# Python wheel exact-subject leaf (#4987)

## Scope

This leaf makes `.github/workflows/python-wheel-qualification.yml` callable and
adds its contract test (`tools/repo-policy/tests/python_wheel_workflow.rs`), a
package-manifest codegen setting (`packaging/python/pyproject.toml`) and this
note. No caller is added, no existing trigger is removed, and no activation
occurs.

The single build-wheel / pip / uv implementation serves both the new
`workflow_call` route and the retained legacy paths: the `pull_request` path
filters, `workflow_dispatch` with its optional `candidate_sha` release-candidate
input (#2769) and the independent `workflow_dispatch(mcp_evidence_pair=true)`
experiment. The experiment keeps its own pinned objects, 60-minute budget and
advisory semantics. The candidate commit resolves as
`inputs.subject_sha || inputs.candidate_sha || pull_request.head.sha ||
github.sha`; the callable subject wins, and the manual release-candidate input
is preserved (the test pins this).

## Callable contract

| Input | Meaning |
| --- | --- |
| proof_profile | `qualification` or `diagnostic` |
| request_id | native run-attempt, digits-digits |
| head_sha | immutable H |
| base_sha | immutable B, required for qualification |
| subject_sha | actual execution commit T (H for diagnostic) |
| subject_tree | exact T tree, required for qualification |
| workflow_generation | calling workflow generation, matched to GITHUB_WORKFLOW_SHA |

`workflow_call` inherits the calling event, so the callable route is detected by
input-key presence, which also catches calls whose values are all empty. An early
shell guard rejects malformed identities, a wrong run-attempt, wrong repository,
wrong generation, a stale checkout, a wrong tree, or parents other than B H.
Diagnostic requires subject=H. A callable request always runs the wheel
obligation and never runs the legacy event-diff selector. The legacy
policy/applicability job keeps its immutable event base/head selection and its
conservative failure behavior. A failed callable guard blocks expensive work and
leaves the run red.

Callable job limits are 3 + max(8, 12) + 5 minutes; legacy limits are
unchanged. The package-source contract commands run in a `source-contract` job
alongside the release wheel build, and each pip/uv consumer requires both to
succeed. Queue and setup overhead and hosted performance are not measured here.

Permissions stay `contents: read`; checkout credentials are not persisted; there
are no secrets, check-write, OIDC or publication permissions. The artifact is
`ripr-rs-python-wheel-<subject_sha>` and keeps the original wheel/payload hashes
and the metadata, RECORD and payload corruption negatives. The workflow declares
no outputs; a caller would read the reusable job result.

## Rollback

Reverting this change removes the additive callable interface and restores the
serial source-contract step and the single-unit wheel codegen.

## Evidence and limitations

The contract test replays the real workflow shell and job predicates against
immutable Git histories (callable guard payloads, graph shape, parallel
source-contract binding, manual candidate input, codegen recipe). It does not
build a wheel or run the hosted pip/uv jobs. Source checks are not hosted proof
or measured savings. Changelog fragments: `changelog.d/4987-wheel-callable-subject.md`,
`4987-wheel-parallel-contract.md`, `4987-wheel-codegen.md` (Internal / CI).

## Installed ordinary-test hook (#10690)

The existing `source-contract` producer also builds the `boundary_gap_closure`
integration runner from the selected source. It selects exactly one test
executable from Cargo's JSON messages, retains the build status and streams,
and binds the staged bytes to the candidate SHA/tree and SHA-256. The separate
`ripr-python-ordinary-runner-<subject_sha>` artifact carries that runner and its
source receipt. It carries no workspace analyzer executable and creates no
second wheel build.

Only the pip consumer adds the ordinary row. Its installation and Python
preview still run with the existing Rust-free environment, offline wheelhouse,
planted PATH decoy and no source checkout. After those steps, a separately named
toolchain step provisions Rust 1.95.0 for the fixture's native Cargo tests. The
uv consumer retains its existing install, preview, reinstall and uninstall
coverage. Both consumers still require the source contract and wheel build.

The pip hook cross-checks the runner receipt and wheel receipt against the same
source/tree, checks the staged digest, and invokes exactly
`ordinary_daily_loop::ordinary_loop_preserves_production_subject_and_native_outcome`.
All five strict inputs are supplied in an otherwise cleared environment:
`RIPR_ORDINARY_MODE=installed`, the already verified `CONSUMER_EXECUTABLE`, its
expected payload SHA-256, its complete source-bearing version line, and a fresh
absolute evidence directory. Missing or mismatched input cannot select the
Cargo-built analyzer. The selected fixture is embedded in the test runner;
neither a checkout nor a hidden source-fixture copy is required in the consumer.

A positive hook requires one exact selected and executed test, native exit zero,
the strict admission and completion records, one executed positive project test,
retained command streams/outcomes, unchanged analyzer and runner bytes, and
fixture cleanup. The existing case separately checks wrong input, wrong base,
zero project-test execution, wrong expectation and restoration. The artifact
`ripr-python-ordinary-installed-<subject_sha>` retains the invocation, package
and runner identities, tool versions, stdout/stderr, statuses and raw journey
evidence for seven days, including partial evidence when the invocation fails.
An incomplete or failed run is not promoted to a completed installed journey.

The workflow contract's `ordinary` child module replays the actual added shell
with explicit Cargo and runner test doubles. It rejects ambiguous compiler
artifacts, failed builds, dirty/wrong source, wrong transferred identities,
empty execution, missing completion and partial strict inputs. These are
orchestration controls, not a wheel build or a native fixture pass. The retained
8-minute source-contract and 5-minute callable-consumer budgets still require
measurement on the exact hosted candidate.

This is a controlled ordinary fixture row under #10690/#4508. It does not
complete the governed repair, literal three-language matrix, real client/editor,
blind operator, formal-verifier or empirical adoption acceptance. Static
movement remains separate from the observed native result. Reverting this hook
removes its staging and pip-only native phase while preserving the callable
interface and the existing Rust-free package checks.

## Parallel source contract

In an observation recorded on the source branch (not re-run here), a callable
run cancelled the wheel job at its 12-minute cap after the serial
package-source step consumed 253 seconds (dev xtask build, test executable
build, 20 distribution tests) before release compilation started. Running those
same two commands in a parallel job preserves every build, negative and consumer
command, the release profile, tool pins, triggers and permissions.

## Wheel codegen

A later standalone producer on the source branch succeeded in 12m49s (12m07s
of it release compilation), exceeding the 12-minute callable cap, so parallelism
alone was insufficient. `[tool.maturin]` now selects four release codegen units;
optimization level 3, fat LTO, stripping, features and locked dependencies are
unchanged, and the root Cargo release profile and the MCP evidence route keep
their single-unit recipe.

Source-branch local experiment (Linux x86_64, Rust 1.95, Maturin 1.14.1, cold
targets, only the codegen override varied; directional, not a hosted guarantee):

| Recipe | Wheel build | Native payload bytes | Disposition |
| --- | ---: | ---: | --- |
| One unit | 610.390s | 32,776,256 | Baseline |
| Sixteen units | 402.904s | 35,705,336 | Rejected: 8.94% payload growth |
| Four units | 353.610s | 34,084,976 | 42.1% less time; 3.99% payload growth |

Limits were fixed beforehand: at least 20% compile saving, at most 5% payload
growth, at most 5% median CPU regression. Baseline and four-unit wheels passed
metadata, RECORD and payload negatives and pip/uv install, reinstall and
uninstall controls. Interleaved runs on three bounded workloads (Python, Rust,
mixed) gave median CPU ratios 1.012, 0.960 and 0.906 with identical results;
these are small workloads, not universal equivalence. Peak compiler RSS of
7,429,136 KiB was observed; hosted memory headroom is unqualified.

The manifest setting applies to every Maturin wheel target, but only Linux x86_64
was measured; Windows and macOS runtime, size and timing are not established.
Hosted timing of the exact candidate is also not established.
