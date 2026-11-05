# Contributing

`ripr` is built PR by PR. Each PR should have a narrow production delta and a
complete evidence package, so reviewers can evaluate behavior, risk, and
traceability without reconstructing intent from chat history.

## Local Setup

1. Install [`rustup`](https://rustup.rs/) if you don't already have it.
   `rust-toolchain.toml` pins the exact toolchain version and its
   `rustfmt` and `clippy` components with the minimal installation profile;
   `rustup` picks it up automatically the first time you run a `cargo` command
   in this repo. The optional prebuilt Rust HTML documentation is not installed.
   `cargo doc` still generates package documentation. For offline standard-library
   documentation, run `rustup component add rust-docs` from this repo while online.
2. Clone the repo:

   ```bash
   git clone <repo-url>
   cd ripr-swarm
   ```

   For a managed image, a restored task, or qualification with a newer stable
   compiler, use [repeatable Rust activation](#repeatable-rust-activation-in-bash)
   before invoking Cargo.

3. For workflow-policy, agent-skill or executable-file edits, get focused feedback
   before building the product:

   ```bash
   cargo policy check-workflows
   cargo policy check-agent-skills
   cargo policy check-executable-files
   ```

   Run the check that covers your edit. `cargo policy preflight` covers only
   workflow and agent-skill checks; executable-file policy always reads the Git
   index afresh and is not discharged by that receipt.
   These commands build the small, unpublished `repo-policy` package without
   compiling `xtask`, `ripr` or its parser dependencies. The first run downloads
   and compiles the helper's dependencies; subsequent unchanged runs reuse them.
   Run from the repository root with the pinned Rust toolchain on `PATH`.
   Failures point to reports under `target/ripr/reports/`.

   This is focused policy feedback, not all docs checks, product compilation,
   `precommit`, or required merge qualification. Other edits still need their
   applicable checks, and hosted required CI remains authoritative for merge.

4. For focused Rust product work, start with the
   [package, target, and feature selection examples](docs/TESTING.md#focused-local-iteration).
   For workspace-wide compilation feedback, run:

   ```bash
   cargo check --workspace --all-targets
   ```

   Once that passes, run the full test suite — it is slower and compiles
   more of the workspace, including test-only code:

   ```bash
   cargo test --workspace
   ```

5. Run a bounded first-use smoke check from the repository root:

   ```bash
   cargo run --locked -p ripr -- check --root fixtures/boundary_gap/input --diff fixtures/boundary_gap/diff.patch
   ```

   Expect one boundary finding for `amount >= discount_threshold`, not
   an all-clear. This exercises real diff loading, Rust analysis and
   default human guidance in a small, explicit workspace. It does not
   analyze this whole repository, other languages or an installed package.

   The reference smoke budget is **one second after the CLI is built**
   on the measured Linux x86-64 four-CPU runner. Downloads and compilation
   are separate; other hosts may differ. The command, repeated cold-RIPR-
   cache and warm measurements, and host identity are retained in
   [#8547](https://github.com/EffortlessMetrics/ripr-swarm/issues/8547).
   This is a bounded-fixture budget, not a whole-repository latency promise.

   The broader repository-root sample remains available:

   ```bash
   cargo run --locked -p ripr -- check --diff crates/ripr/examples/sample/example.diff
   ```

   That is a different workload. Its default human guidance can still
   build a repository-wide evidence index, so it is not the quick setup
   check and is not covered by the fixture budget.

### Repeatable Rust activation in Bash

On Linux or macOS, source the repository's activation boundary in the **same Bash
shell** that will run Cargo. No shell profile or earlier setup process is assumed
to have configured a later task:

```bash
source tools/activate-rust.sh --pinned &&
mkdir -p target
```

With `--pinned`, activation reads the exact repository pin from
`rust-toolchain.toml`. Bare sourcing is unsupported: Bash inherits the caller's
positional arguments when a file is sourced without arguments. The explicit
selector leaves those caller arguments unchanged. Activation selects already
installed tools and never downloads anything. If the requested toolchain is
absent, run the explicit
`rustup toolchain install ... --profile minimal` repair command it prints during
online image preparation, then source the file again. This keeps the optional
prebuilt Rust HTML documentation out of the installation.

Activation preserves absolute `CARGO_HOME` and `RUSTUP_HOME` overrides, defaulting
to `$HOME/.cargo` and `$HOME/.rustup` only when unset. Relative or empty homes are
refused because fixture commands change directories. It exports an exact
`RUSTUP_TOOLCHAIN`, the resolved `RUSTC`, and `RUSTUP_AUTO_INSTALL=0`; nested
fixtures inherit the selected compiler even when they contain their own pin.
Directories added to `PATH` must not contain colons, which separate PATH entries.
Activation refuses colons in the configured Cargo-home PATH entry and in the
physical sysroot before changing exports. The physical-sysroot check also catches
colons hidden by a symlink. Spaces and symlinked homes remain supported when the
selected PATH entries are representable.
Existing conflicting `RUSTC` / `CARGO_BUILD_RUSTC` or compiler-wrapper environment
variables cause a refusal with an explicit repair. Remove an override deliberately
when switching compilers. Shell functions or aliases named `cargo` or `rustc`
also cause a refusal and remain intact. Remove them explicitly or use a fresh
Bash shell. Variables activation exports must be ordinary scalars, optionally
exported; arrays, integer, nameref, case-transform and readonly attributes are
refused before their values are read or any exports are applied. Cargo
configuration files can also define wrappers; review those separately when
using a custom build environment.

Every tool probe finishes before activation changes the parent environment.
The selected absolute Cargo must also complete `cargo -vV`; an executable path
whose native version command fails is refused before any activation exports.
Repeated activation does not grow `PATH` or change shell options or the current
directory. Relative sourcing preserves an exported or readonly `CDPATH` and
does not use it to find the repository. Running `bash tools/activate-rust.sh` is
rejected because a child process cannot activate its caller. The existing policy
executable's compiler identity guard remains in force: rebuild through
`cargo policy` after changing compilers.

To select the installed current stable compiler for later qualification, source
with `stable` **during image preparation**:

```bash
source tools/activate-rust.sh stable
```

The helper resolves stable's exact release and host, requires that exact
toolchain to be installed, and compares the complete `rustc -vV` identities.
If only the rolling alias is installed, run the printed command to install the
exact release once and retry. Store the printed exact selector in the task's
startup command, for example
`source tools/activate-rust.sh 1.95.0-x86_64-unknown-linux-gnu` for that release
and host. Reuse the exact selector on restored tasks; using `stable` again
deliberately resolves the alias again. This does not change the repository's
toolchain pin or MSRV obligation.

For development that needs standard-library source and LLVM coverage tools,
install those components once while online, then activate again:

```bash
rustup component add --toolchain "$RUSTUP_TOOLCHAIN" rust-src llvm-tools-preview &&
source tools/activate-rust.sh "$RUSTUP_TOOLCHAIN"
```

`LLVM_COV` and `LLVM_PROFDATA` always name absolute paths under the selected
compiler's sysroot. When both tools are present, activation checks their native
version commands against the compiler's LLVM version. When they are absent,
ordinary Rust activation succeeds with **coverage tools unavailable**, and the
exports still name those missing paths so consumers of the exports cannot fall
back to an unrelated LLVM on `PATH`. This does not prove that coverage production
or merging succeeds.

Run the tools needed by your next lane before scheduling expensive builds:

```bash
cargo fmt --version
cargo clippy --version
cargo nextest --version
```

For coverage work, also probe both selected sysroot tools:

```bash
"$LLVM_COV" --version
"$LLVM_PROFDATA" --version
```

Nextest is a separate prerequisite; provision it in the selected Cargo home using
the version chosen for the environment, then retain `cargo nextest --version`
with the setup evidence. Activation does not install it. Continue with the
[focused testing commands](docs/TESTING.md#focused-local-iteration) and preserve
the [Nextest and doctest distinction](docs/TESTING.md#ci-test-results).
Use an already built `ripr doctor --profile source-build --root . --json` for the
existing typed setup diagnosis. Activation is compiler selection, not a second
doctor or a passing build/test result.

Create the checkout's `target/` even when `CARGO_TARGET_DIR` points elsewhere:
`.cargo/config.toml` redirects temporary files there. Keep a candidate's target
directory stable and serialize Cargo writers sharing it or a constrained host;
an earlier warm cache does not prove a later cold link will fit.

After restoration, repeat activation and the required capability probes in the
new task. Record the actual compiler identity, native command results, and
selected test scope. Until those commands finish, builds, full tests, coverage,
editor-host execution, and restoration proof remain **not run**. The Bash
boundary has a local behavioral check that needs no Rust installation:

```bash
bash tools/test-activate-rust.sh
```

That harness uses controlled tool fixtures in fresh Bash shells. It tests
activation and refusal behavior on the host and Bash version that execute it;
it does not compile Rust or establish native macOS/Bash 3.2 or restored-task
qualification. On Unix, `cargo test -p xtask --test rust_activation` runs the
same harness and a separate check against the run's already installed exact
Rustup SDK. The native check sources the real helper in a fresh Bash child and
compares compiler/Cargo identity at the repository root and in an isolated
nested directory with an opposing pin. It disables automatic installation and
runs version/identity probes only, with no workspace build. This requires the
exact release/host SDK to be installed already; missing prerequisites fail the
check. CI retains that native test's identity/probe output in the existing JUnit
report. Hosted fresh-shell evidence does not establish saved-cloud restoration.
Native Windows shells keep their existing setup route.

### Fixture preparation

Before fixture or golden checks, prepare the canonical locked Rust fixture:

```bash
cargo xtask goldens prepare
cargo xtask fixtures owner_return_pin_fallible_literal
```

Preparation prints the effective absolute `CARGO_HOME`, selected `RUSTUP_HOME`
when set, repository toolchain pin and actual Cargo/rustc versions. Keep that
same environment for subsequent checks. `--offline` supports a seeded registry
cache. The command fetches only the locked fixture manifest and verifies offline
metadata; it does not build the workspace or change expected outputs. Fixture
readiness and offline preparation also disable Rustup auto-installation, including
the Cargo/rustc version probes: a missing pin refuses instead of downloading a
toolchain. Ordinary online preparation preserves the caller's Rustup policy.
Fixture checks refuse missing prerequisites with the selected environment and repair
command, rather than treating availability drift as a reason to bless goldens.
A fetch in another Cargo home does not prepare the subprocess. Sanitized external
sensors need their own compatible preparation boundary; this local command does
not forward ambient homes into them or qualify advisory CI (#7920).

The first `cargo xtask ...` invocation compiles the `xtask` crate itself, so
it is slower than later runs. Once the workspace builds, `cargo xtask
worktree doctor` checks your working tree for common local hygiene issues
(dirty `main`, stale branches, generated-artifact residue) before you start
shaping a PR.

For the local collect-all policy command, [RIPR-SPEC-0253](docs/specs/RIPR-SPEC-0253-local-collect-all-policy.md)
defines the selected checks and failure/report contract. Its five real-driver
acceptance tests run with `cargo test -p xtask --test policy_all`.

## Product Contract

Before changing code, check the product question:

```text
For the behavior changed in this diff, do the current tests appear to contain
a discriminator that would notice if that behavior were wrong?
```

Changes that do not improve the precision, speed, usability, calibration, or
maintainability of that answer should be deferred.

## PR Shape

Prefer one PR per capability step from [Implementation plan](docs/IMPLEMENTATION_PLAN.md).

Each PR should include:

- scoped implementation or documentation changes
- tests or documented verification
- relevant docs updates
- changelog entry when behavior, workflow, or public docs change, as one new
  file under `changelog.d/` rather than an edit to `CHANGELOG.md`: name it
  `<pr-or-issue>-<short-slug>.md`, make its first line the section as an HTML
  comment (`<!-- section: Fixed -->`), and write the entry below it in the
  prose style of `CHANGELOG.md`, ending with the issue or PR reference. Two PRs
  that each add a file never conflict. The fragments are folded into
  `Unreleased` by hand at the release cut; existing `CHANGELOG.md` entries stay
  put. The format and fold step are in
  [changelog.d/README.md](changelog.d/README.md), and the sections in
  [Changelog policy](docs/CHANGELOG_POLICY.md)
- traceability from spec to tests to code for behavior changes

## Scoped Evidence-Heavy PRs

PR size is measured by production risk, not line count.

A scoped PR changes one production behavior, public contract, or architectural
seam, then includes the complete evidence package needed to make that change
reviewable: specs, fixtures, tests, golden outputs, docs, metrics, ADRs,
learnings, and traceability.

A large PR can be scoped when the production delta is narrow and most of the
diff is supporting evidence. A small PR can still be too large when it mixes
unrelated behaviors, changes multiple public contracts, or touches multiple
architectural seams without one shared acceptance criterion.

Every PR should make three things visible:

- production delta: what behavior, contract, or seam changed
- evidence delta: what specs, tests, fixtures, goldens, docs, metrics, ADRs, or
  learnings support it
- acceptance criterion: what single reviewable claim the PR proves

Prefer:

- narrow production delta
- large evidence delta when needed
- clear spec -> test -> code mapping
- deterministic golden output
- explicit metrics movement
- documented non-goals

Avoid:

- unrelated production behavior changes
- schema changes bundled with analyzer rewrites unless one acceptance criterion
  requires both
- LSP or UI changes bundled with classifier changes unless they share one
  finding contract
- cleanup mixed with behavior changes

## Review Checklist

Before requesting review:

- [ ] Scope matches one roadmap or implementation-plan item.
- [ ] Production delta and evidence delta are both explicit.
- [ ] New behavior has a spec entry or updates an existing spec.
- [ ] Tests use BDD-shaped names or fixture names that explain the behavior.
- [ ] Output changes update golden expectations and schema docs.
- [ ] Static output avoids mutation-runtime outcome language.
- [ ] Unknowns include stop reasons where applicable.
- [ ] No new `panic`, `unwrap`, `expect`, `todo`, or `unimplemented` in
      production or test code.
- [ ] CI-relevant docs or workflows were updated when gates changed.
- [ ] New non-Rust programming files are covered by the file policy allowlist.
- [ ] New generated files, dependency surfaces, process spawning, and network
      behavior are covered by policy allowlists.
- [ ] Generated evidence is not hand-authored: ordinary PRs do not carry
      `badges/*.json`, `target/ripr/**`, or
      `crates/ripr/examples/sample/target/**` residue.
- [ ] `cargo xtask shape` or `cargo xtask fix-pr` was run before final checks.

## PR Shaping Commands

The shape/check/guide model is documented in
[PR automation](docs/PR_AUTOMATION.md).
Generated artifact ownership is documented in
[Generated evidence discipline](docs/GENERATED_EVIDENCE.md).

Use `shape` when the tree needs safe normalization:

```bash
cargo xtask shape
```

It can run `cargo fmt`, sort policy allowlists, create
`target/ripr/reports`, and write `target/ripr/reports/shape.md`.

Use `fix-pr` as the current safe repair entrypoint:

```bash
cargo xtask fix-pr
```

It runs `shape`, refreshes `target/ripr/reports/pr-summary.md`, and writes
`target/ripr/reports/fix-pr.md`. Future automation also expands the repair
brief for judgment-required issues.

Generate the reviewer packet directly with:

```bash
cargo xtask pr-summary
```

The summary classifies changed paths into production, evidence, policy,
workflow, extension, and public-contract surfaces.

Run the cheap local guardrail with:

```bash
cargo xtask precommit
```

Run the review-ready non-release gate with:

```bash
cargo xtask check-pr
```

Validate fixture and golden scaffolding with:

```bash
cargo xtask fixtures
cargo xtask goldens check
cargo xtask test-oracle-report
cargo xtask dogfood
```

Long-running Codex Goals campaigns should follow
[Codex Goals](docs/CODEX_GOALS.md), [Implementation campaigns](docs/IMPLEMENTATION_CAMPAIGNS.md),
and the [Scoped PR contract](docs/SCOPED_PR_CONTRACT.md).

## Required Rust Gates

For one complete local review and package pass, run `cargo xtask ci-full`. It
runs the review-ready `check-pr` lane, the evidence gates (`fixtures`,
`goldens check`, `test-oracle-report`, `dogfood`, and `metrics`), then the
package listing and publish dry-run. The explicit commands below remain the
inventory for targeted reruns.

The following report commands are advisory and do not independently block a
merge: `cargo xtask pr-triage-report`, `cargo xtask metrics`,
`cargo xtask check-pr-shape`, and `cargo xtask module-health`.

```bash
cargo xtask shape
cargo xtask fix-pr
cargo xtask pr-summary
cargo xtask precommit
cargo xtask check-pr
cargo xtask fixtures
cargo xtask goldens check
cargo xtask test-oracle-report
cargo xtask dogfood
cargo xtask metrics # advisory
cargo fmt --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
cargo package -p ripr --list
cargo publish -p ripr --dry-run
cargo xtask check-static-language
cargo xtask check-no-panic-family
cargo xtask check-file-policy
cargo xtask check-covered-by
cargo xtask check-executable-files
cargo xtask check-workflows
cargo xtask check-spec-format
cargo xtask check-fixture-contracts
cargo xtask check-traceability
cargo xtask check-capabilities
cargo xtask check-workspace-shape
cargo xtask check-architecture
cargo xtask check-public-api
cargo xtask check-output-contracts
cargo xtask check-identity-registry
cargo xtask check-doc-index
cargo xtask check-pr-shape # advisory
cargo xtask check-generated
cargo xtask check-dependencies
cargo xtask check-process-policy
cargo xtask check-network-policy
```

When the worktree is intentionally dirty during local review, Cargo packaging
commands may require `--allow-dirty`. A branch is not ready to merge until the
plain commands pass on a committed tree.

## Required Extension Gates

For changes under `editors/vscode`:

```bash
cd editors/vscode
npm ci
npm run compile
npm run package
```

## Documentation

Use the documentation map in [Documentation system](docs/DOCUMENTATION.md).

For behavior changes, update:

- [Specs](docs/specs/README.md)
- [Spec-test-code traceability](docs/SPEC_TEST_CODE.md)
- [Testing](docs/TESTING.md)
- output or config reference docs when public shapes change

For decisions, add or update an ADR.

For repo knowledge, update [Learnings](docs/LEARNINGS.md).
## Optional Linux LLD builds

The normal workspace build uses the platform default linker and does not
require LLVM. In a controlled Linux environment with `clang` and `ld.lld`
installed, use `cargo build-lld --release` to opt into the faster linker path.
