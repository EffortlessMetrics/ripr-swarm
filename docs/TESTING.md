# Testing

Tests should prove product behavior, not implementation trivia. For analyzer and
output work, use BDD-shaped names and fixtures that make the behavior question
plain:

```text
given_changed_boundary_when_equal_value_is_missing_then_reports_weak_exposure
```

Behavior changes should have a three-way match:

```text
spec -> test -> code
```

See [Spec-test-code traceability](SPEC_TEST_CODE.md) for the expected mapping.
See [Test taxonomy](TEST_TAXONOMY.md) for required proof levels by change type.

Prepare the pinned fallible owner-return fixture before offline analyzer tests
or fixture/golden checks. Its anyhow 1.0.100 dependency is separate from the
workspace dependency graph; a workspace build does not fetch it. Cargo performs
this locked fetch explicitly; production analyzer metadata stays offline.

```bash
cargo fetch --locked --manifest-path fixtures/owner_return_pin_fallible_literal/input/Cargo.toml
```

The literal-JSON owner-return fixture pins serde_json 1.0.151, which the audited
expansion in `assertion_dependency.rs` requires and the workspace lock no longer
carries. Prepare it the same way before the owner-pin JSON tests:

```bash
cargo fetch --locked --manifest-path fixtures/owner_return_pin_json_real_literal/input/Cargo.toml
```

Use the same Cargo home for preparation and checks. A new Cargo home or an
installed analyzer comparison needs its own preparation against the canonical
fixture manifest. CI's Rust, coverage, analytics and advisory test paths run
this step before offline checks. Missing preparation fails the positive; it is
never treated as a skip or a reason to lower exposure assertions.

## Focused local iteration

Select the package, Cargo target, and language features before applying a test
name filter. A filter such as `structural_workspace_markers` selects tests to
execute; by itself it does not limit which integration-test targets Cargo
compiles. `--test cli_smoke` selects that integration target and its required
product binary. Use `--lib` for library-only tests.

Keep an isolated `CARGO_TARGET_DIR` stable for repeated runs of the same
candidate. From the repository root in Bash, place it below the checkout's
`target/` and create it before invoking Cargo:

```bash
export CARGO_TARGET_DIR="$PWD/target/contributor-rust"
mkdir -p "$CARGO_TARGET_DIR"
```

This creates both the build-artifact directory and its parent `target/`.
The repository's `.cargo/config.toml` forces `TEMP`, `TMP`, and `TMPDIR` to
the checkout's `target/`, independently of `CARGO_TARGET_DIR`. If you place
build artifacts outside the checkout, also run `mkdir -p target` from the
repository root before building. Exporting another temporary directory does
not override that forced setting. See the
[workspace linker temp redirect](agent-context/validation.md#workspace-linker-temp-redirect)
for the MSVC failure and recovery details.

Record the package, target, profile, and effective features with each
result: binary builds, test preparation, and Clippy can populate different build
units even when they share that directory.

For example, these commands prepare and exercise the Rust workspace-marker
tests from the repository root:

```bash
cargo build --locked --offline -p ripr --bin ripr \
  --no-default-features --features lang-rust --message-format=json
cargo test --locked --offline -p ripr --test cli_smoke \
  --no-default-features --features lang-rust --no-run --message-format=json
cargo test --locked --offline -p ripr --test cli_smoke \
  --no-default-features --features lang-rust structural_workspace_markers
```

These examples assume a prepared dependency registry. On first use, omit
`--offline` so Cargo can download missing workspace dependencies; prepare the
separate fixture dependency above when the selected tests require it. The JSON
compiler output records emitted targets and effective features. `--no-run`
separates preparation from execution; the final command can also prepare and
run the selected target directly.

Choose features for the behavior being tested. The example intentionally
selects Rust only. For default-adapter coverage, omit both
`--no-default-features` and `--features lang-rust`; defaults include Rust,
TypeScript/JavaScript, and Python. Perl remains opt-in through `lang-perl`.
Runtime language configuration is separate from compiled adapter availability;
see the [language-adapter workflow](LANGUAGE_ADAPTER_PREVIEW.md#enable-preview-adapters).

Keep `-p ripr` when a package-scoped feature check is intended. `xtask` depends
on RIPR with default features enabled, so
`cargo tree --locked --workspace --no-default-features --features ripr/lang-rust -e normal,build`
still resolves the TypeScript and Python parsers through that dependency.
Inspect the selected package instead with
`cargo tree --locked -p ripr --no-default-features --features lang-rust -e normal,build`.
For workflow-policy, agent-skill, and executable-file edits, use the existing
[`cargo policy` frontdoor](../CONTRIBUTING.md#local-setup) for focused feedback.

### Bounded module-motion controls

The #4680 controls implement the
[bounded module-motion contract](specs/RIPR-SPEC-0005-repo-seam-inventory.md#bounded-rust-module-motion).
They build real disk-backed indexes for parent, inline-child and file-child
placements, pair accessible observers with wrong-owner, changed-value and private
child negatives, and check descendant/helper preservation. They also exercise
default-target authority, all five compact stage states, portable snapshot
identity, and production repair-target selection through rendered card JSON.

```bash
cargo test --locked --offline -p ripr --lib module_extraction
cargo test --locked --offline -p ripr --lib unit_target_discovery_flags_do_not_change_shared_root_authority
```

The aggregate `module_extraction_controls` reports every control family even when
one fails; a failure still fails the test. These tests do not close the separate
local binding/shadowing or duplicate-import residuals on #4680. Record the exact
head, selected features and controls actually reached when reporting a hosted
fail-fast run. Host restrictions take precedence over these local examples;
use the existing permitted hosted qualification route when local builds are held.

### Measured example and limits

At `97cc171257fd76a1b3b9e79cc41ad949e0ecc861`, the three Cargo commands above were
executed on Linux x86_64 with Rust/Cargo 1.95.0, `CARGO_BUILD_JOBS=2`, and an
initially empty isolated `CARGO_TARGET_DIR`. The toolchain and registry were
prepared before timing; commands ran sequentially and reused earlier outputs.

| Command | Observed result |
| --- | --- |
| Explicit binary build | 115.23 s; RIPR library and binary, effective product features `["lang-rust"]`. |
| `cli_smoke` preparation | 106.46 s following that build; one integration-test executable and its required product binary. |
| Filtered `cli_smoke` execution | 0.42 s including Cargo; 3 passed, 476 filtered out. |

At the same revision, `cargo tree -e normal,build` contained 113 distinct
package/version identities for package-scoped Rust-only selection, 203 for
package-scoped defaults, and 220 for workspace selection with
`--no-default-features --features ripr/lang-rust`. These graph counts are not
compiler-artifact counts or measurements of time saved. The timings are one
prepared-registry sample, not cold onboarding, a default-build comparison,
all-language coverage, or a complete qualification result. A passing filtered
test does not establish that every internal fixture branch executed.

Focused iteration leaves required broad qualification intact: retain the
workspace/all-target checks, default and non-default feature obligations,
fixtures and goldens, and the [nextest plus doctest contract](#ci-test-results).
Select additional proof for the changed behavior before requesting qualification.

## Full validation inventory

The commands below inventory applicable checks. `cargo xtask ci-full` composes
the complete local review/evidence/package pass; choose the aggregate for the
intended proof without repeating its constituent commands.

```bash
cargo xtask shape
cargo xtask fix-pr
cargo xtask pr-summary
cargo xtask precommit
cargo xtask check-pr
cargo xtask fixtures
cargo xtask goldens check
cargo xtask golden-drift
cargo fmt --check
cargo check --workspace --all-targets
cargo nextest run --workspace
cargo test --workspace --doc
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
cargo xtask ci-fast
cargo xtask ci-full
cargo xtask check-static-language
cargo xtask check-no-panic-family
cargo xtask check-file-policy
cargo xtask check-covered-by
cargo xtask check-executable-files
cargo xtask check-workflows
cargo xtask check-spec-format
cargo xtask check-spec-numbering
cargo xtask check-fixture-contracts
cargo xtask check-traceability
cargo xtask check-capabilities
cargo xtask check-workspace-shape
cargo xtask check-architecture
cargo xtask check-public-api
cargo xtask check-output-contracts
cargo xtask check-doc-index
cargo xtask check-pr-shape
cargo xtask check-generated
cargo xtask check-badge-diff-policy
cargo xtask check-generated-clean
cargo xtask check-dependencies
cargo xtask check-process-policy
cargo xtask check-network-policy
```

Package check:

```bash
cargo package -p ripr --list
cargo publish -p ripr --dry-run
```

The current test suite covers:

- unified diff parsing
- Rust test/assertion extraction
- harness trial-subject evidence parity (helper callbacks, method
  oracles, dormant templates, shadow fail-closed boundaries)
- JSON escaping
- simple end-to-end diff analysis
- CLI smoke behavior
- property-based parser invariants (diff parser)

## Property-Based Tests

The diff parser (`crates/ripr/src/analysis/diff/parse.rs`) has property-based
tests using [`proptest`](https://crates.io/crates/proptest) (a dev-dependency).
These generalize the hand-rolled LCG fuzz tests with automatic shrinking and
broader input coverage.

Properties checked:

1. **Totality**: `parse_unified_diff` never panics on arbitrary string input.
2. **Structural invariants**: no empty paths, no newlines in line text, for
   generated diff-like text.
3. **Line number validity**: every added line's `new_side_line` is >= 1.

Convention: property tests use `Result<(), String>` bodies, no `unwrap`/`expect`,
per the workspace lint posture. See `docs/TEST_TAXONOMY.md` for the Property
and Fuzz test type definitions.

## Harness Trial-Subject Evidence Parity Scan

The repository-governed test-harness registry
(`crates/ripr/src/analysis/facts/harness_registry.rs`, [RIPR-SPEC-0173](specs/RIPR-SPEC-0173-trial-subject-evidence-parity.md))
derives evidence for registered harness trial subjects (for example
libtest_mimic `Trial::test` registrations). Its parity scan keeps a trial
subject's observation evidence aligned with what an ordinary `#[test]`
carrying the same code would show, and every widening is fail-closed.
The discriminating unit tests live in
`crates/ripr/src/analysis/facts/harness_registry/tests.rs`:

- helper-callback bodies contribute one level of parsed evidence
  (calls, oracles, literals) with real line attribution, only when the
  callback name provably binds the file-level fn (`shadowed_callback_...`
  and `..._const_or_static_shadow_...` pins block let/parameter/const/
  static/import/nested-module shadows);
- method-position `.unwrap()`/`.expect()` calls register smoke oracles
  with receiver-ful text across keyword, indexed, cast, operator, and
  negation receiver forms (`trial_method_oracle_receivers_...`);
- assertion macros in every delimiter Rust permits (`(...)`, `[...]`,
  `{...}`) keep their complete invocation text, classification, and
  observed tokens (`trial_alternative_delimiters_...`);
- dormant `macro_rules!` templates — in any delimiter, in a closure or
  a helper body — contribute no oracle, call, or literal evidence while
  live surrounding evidence still admits (`trial_dormant_...` and
  `given_dormant_template_in_helper_...` pins).

When you touch the trial evidence scan, work fixture-first: verify the
new discriminating test fails on the pre-fix head, then check the
ordinary-parser parity pins and the fail-closed negative controls
together, so a widened admit cannot land without its boundary.

## Error-Handling Bar

The target rule is:

```text
No panic, unwrap, expect, todo, or unimplemented in production or tests.
```

New tests should return `Result` when setup can fail and should use explicit
assertions. Existing panic-family usage is tracked engineering debt and should
be paid down in scoped PRs rather than copied into new tests.

## VS Code Extension Tests

The VS Code extension smoke tests run inside a real VS Code instance through
`@vscode/test-electron`:

```bash
cd editors/vscode
npm ci
npm run test:e2e
```

The test suite:

- opens a fixture Rust workspace (`test-fixtures/workspace/Cargo.toml`)
- activates the extension
- asserts commands are registered (`ripr.restartServer`, `ripr.selectWorkspaceRoot`,
  `ripr.showOutput`,
  `ripr.copyContext`, `ripr.copySuggestedAssertion`,
  `ripr.copyTargetedTestBrief`, `ripr.copyAgentPacketCommand`,
  `ripr.copyAgentBriefCommand`, `ripr.copyAfterSnapshotCommand`,
  `ripr.copyAgentVerifyCommand`, `ripr.copyAgentReceiptCommand`,
  `ripr.openRelatedTest`, `ripr.openSettings`)
- verifies the defaults-first editor check mode is `draft`
- pins the `contributes.menus` editor/context repair-loop entries (ripr
  groups, `resourceLangId` gating) and the two default
  `contributes.keybindings`, keeps the payload-bound targeted-test and
  agent-loop commands code-action-only, and fails closed when a menu or
  keybinding references an unregistered command
- verifies `copyContext` completes without crash when no editor is active
- verifies `copyContext` accepts a structured target with `finding_id` and
  `probe_id` without crashing
- verifies `copyContext` asks LSP `ripr.collectContext` first for `seam_id`
  targets and falls back to the CLI when LSP returns no packet or errors
- verifies suggested-assertion and targeted-test-brief commands copy valid
  payloads and ignore malformed arguments without throwing
- verifies agent-loop command copying writes command text and the contributed
  command handlers ignore malformed arguments without throwing
- verifies LSP agent-loop command payloads stay workspace-relative across
  platform-shaped roots and fail closed for stale seam diagnostics
- verifies the live real-server boundary-gap path publishes a seam diagnostic,
  renders hover evidence, exposes seam actions, copies seam packet and verify
  command payloads, and opens the best related test
- verifies explicit editor status states for disabled config, missing
  workspace, unavailable server, queued/running/complete/no-seam/failed
  refreshes, and stale dirty Rust buffers that stay stale until save or close
- verifies an existing workspace-matched `first-useful-action.json` is projected
  through status bar and `ripr: Show Status` without overriding stale evidence
  or accepting reports for another workspace
- verifies `openRelatedTest` opens URI/line targets and ignores malformed
  arguments without throwing
- verifies `restartServer` is callable even when server resolution fails

CI runs the suite headless with `xvfb-run -a npm run test:e2e`. The test
runner stores downloaded VS Code archives under
`target/ripr/vscode-test-cache` so generated editor-host state stays with the
rest of the repo-local build output. Isolated host user-data is
`target/ripr/vscode-test-user-data`, with VS Code logs under `logs/`. A
test-host failure copies a size-capped, redacted log subset into
`target/ripr/vscode-test-host-diagnostics/`; CI uploads that folder only when
the vscode job fails. Success runs do not retain the diagnostics copy, and
collection cannot change the original test exit status.

## Golden Output

When changing user-visible output, update or add golden coverage for:

- human output
- JSON output
- context packets
- LSP diagnostic shape, when applicable

Golden updates must preserve the static language boundary: draft static output
does not use mutation-runtime terms such as `killed` or `survived`.

## CI Test Results

The protected routed Rust lane has an intentional dual runner contract:

```bash
cargo nextest run --workspace --profile ci
cargo test --workspace --doc
```

Nextest owns the compiled test binaries it selects. Cargo and rustdoc own
workspace doctests. Both commands are required; success from one command is not
projected onto the other command's subject set.

The advisory Test Analytics workflow uploads Rust test results to Codecov from
JUnit XML generated by `cargo nextest`:

```bash
cargo nextest run --workspace --all-features --profile ci
```

That advisory lane repeats doc tests for its own receipt:

```bash
cargo test --workspace --doc
```

The JUnit output path is configured in `.config/nextest.toml` at
`target/nextest/ci/junit.xml`.

## Unused suppression-policy disclosure

The [repository configuration contract](specs/RIPR-SPEC-0007-repository-configuration.md#unused-default-suppression-disclosure-7759)
is exercised by a genuine changed Rust fixture and the built CLI:

```bash
cargo test --locked -p ripr --test default_suppression_disclosure -- --test-threads=1
```

The suite distinguishes unused presence from explicit application/refusal,
preserves complete stdout, and excludes foreign-CWD and directory false positives.

## MCP unknown-resource recovery

[RIPR-SPEC-0211](specs/RIPR-SPEC-0211-mcp-workspace-session-evidence.md#unknown-resource-recovery-7767)
defines the message-only client's recovery contract. The raw stdio control
pins literal prose and structured routes, preserves recognized-route failures,
and follows the rejection with a valid status read on the same connection.
Official SDK cases cover the negotiated lifecycle mapping independently.

```bash
cargo test --locked -p ripr --test mcp_stdio rejection_arms_survive_the_stdio_transport -- --exact
cargo test --locked -p ripr --test mcp_sdk
```

These tests qualify wire behavior, not installation or every protocol version.
