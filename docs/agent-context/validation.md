# Validation Commands

Use the smallest validation set that proves the changed behavior, then inspect
the required hosted checks before merge. Do not equate a command inventory with
a requirement to run every command locally.

## Environment and shell

Identify the repository root, worktree, HEAD, actual shell, host/target platform
and toolchain before composing commands. Read `rust-toolchain.toml` rather than
copying a version from a retained session. Detect installed tools instead of
assuming Unix utilities or PowerShell cmdlets exist.

A PATH lookup miss alone does not establish that a tool is unavailable. Before
claiming it missing, inspect permitted configured and local toolchain locations
appropriate to the host, including process-local Cargo and rustup homes when
applicable. Verify the resolved executable runs and reports its version with a
matching toolchain configuration. Record the attempted location and result;
distinguish absent, present but unusable, and access denied. Do not read a denied
path or retry a denied source operation by another route. Use process-local
configuration when needed; do not install tools automatically or alter persistent
settings. A usable tool does not itself establish native-platform proof or a pass.

The shell determines grammar; the OS does not. Use PowerShell syntax when the
active shell is PowerShell, including PowerShell on Linux. Use the actual Bash
or POSIX grammar in Bash/WSL/Linux/macOS. A native Windows qualification still
needs a native Windows execution environment; WSL is not equivalent evidence.

Cargo writes normal progress to stderr. Neither stderr output nor a wrapper's
red icon establishes native command failure. Conversely, a success-looking test
line does not override a nonzero exit status. Read both the native status and
the terminal report; retain conflicting or missing evidence as an instrument
failure until reconciled.

PowerShell example, without a pipeline that obscures the native status:

```powershell
& cargo xtask check-agent-skills
$commandExit = $LASTEXITCODE
if ($null -eq $commandExit) { throw 'Native command exit status missing' }
if ($commandExit -ne 0) { throw "Cargo exited $commandExit" }
```

In Bash, capture `$?` immediately after the command. If a pipeline is genuinely
necessary, capture its native command status before another command overwrites
it; `echo done` is not a test result. Do not discard a failed step because a
later command returned zero.

## Workspace linker temp redirect

`.cargo/config.toml` in this repository force-redirects the linker temp
variables `TEMP`, `TMP`, and `TMPDIR` into the workspace-relative `target/`
directory (`force = true`; PR #397: keep MSVC linker temp files off a full
system temp drive). An exported `TEMP`/`TMP` does not override it.

In a fresh `git worktree add` checkout that builds with an isolated
`CARGO_TARGET_DIR`, the workspace-local `target/` does not exist, and MSVC
`link.exe` fails with `LNK1104: cannot open file '...\target\lnk{GUID}.tmp'`
while linking the first build scripts — even though objects and `/OUT`
correctly land in the isolated target. Create the directory before building;
it is git-ignored and holds only transient linker temps:

```bash
mkdir <worktree>/target
```

`ripr doctor --profile source-build` warns when this redirect is present and
the workspace-local directory is absent (#5280).

The same missing `target/` directory makes `std::env::temp_dir()` point at a
path that does not exist. Tests that create scratch crates under `temp_dir()`
must `create_dir_all` that root before an exclusive `create_dir` of the child;
a bare `create_dir` fails with ENOENT and names no path (`agent_stub_compiles`,
#6712).

## Owned background work

Bind a background command to its task/session handle, candidate identity and
log destination. An unchanged report, missing final binary, process-name filter
or quiet interval does not show that the driver died or that children are
orphans. Check the original task's terminal result before retrying. Do not
label a process harmless without ownership, parent/driver and cleanup evidence.

Serialize Cargo operations that share a candidate worktree, target lock or
memory bottleneck. Do not start another broad run against a tree while its
verification is still consuming mutable files. Use separate workers/worktrees
for independent claims when available. Do not kill unrelated Cargo processes
or invent worker access.

A waiting command blocks its transition, not the parent goal. Advance another
ready claim or do independent read-only work. If every useful transition is
waiting, report it as in flight instead of repeatedly polling unchanged state.

## Local and hosted proof

Local proof is for fast discrimination and candidate shaping:

```bash
cargo xtask check-fast
cargo xtask precommit
```

Add the focused test, narrow compile and changed-surface checks required by the
claim. For `check-fast`, independently verify the diff selector and base before
using it as evidence. An empty or failed selector is not proof that nothing
needs testing.

### Local mirror of the required hosted gate

Hosted CI is requested late (merge-ready), so the coding session carries the
proof until then. The required Rust producer (`.github/workflows/rust-gates.yml`)
runs these steps (plus `cargo policy preflight`, a locked fixture `cargo fetch` and dependency-policy steps); `precommit` covers only the first row. Run the rows the change
selects, in this order, and record each command and native exit status in the
PR-native continuation record's `Validation:` line (see
[PR automation](../PR_AUTOMATION.md#pr-native-continuation-record)):

| Hosted step | Local command | Notes |
| --- | --- | --- |
| fmt, policy, dependency/process/network, selected Clippy | `cargo xtask precommit` | Selects Clippy packages from the diff. |
| Evidence promotion honesty | `cargo xtask check-evidence-promotion-honesty` | Not part of `precommit`; run when evidence promotion changes. |
| All-features check | `cargo check -p ripr --all-targets --all-features` | Run for `ripr` changes. The hosted run has no separate `cargo check --workspace --all-targets` step; Strict Clippy builds the same units (#10341). |
| Strict workspace Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | Hosted always runs the full workspace. |
| Required Rust tests | `cargo nextest run -p <impacted package>` | Hosted runs `cargo nextest run --workspace --profile ci`. A `-p` run is focused proof, not that run; `cargo-nextest` may need installing, and `.config/nextest.toml` lists `--skip` filters for the slow LSP tests. |
| Doctests | `cargo test --workspace --doc` | Nextest does not run doctests. |
| Golden, fixture, verdict contracts | `cargo xtask goldens check`, `cargo xtask fixtures`, `cargo xtask verdict-corpus check-all` | Run when output, fixtures or analysis change. |

Do not report a row as passed unless it ran to a zero native exit status on the
exact head; list the rows not run and why.

PR CI supplies the required merge matrix. Publishing a coherent candidate with
`REVIEW_INCOMPLETE` allows hosted proof and review to run; it is not permission
to merge without them. When local execution is unavailable, record that limit,
publish the candidate for the available hosted route, and inspect its exact-head
results. Do not claim that a check ran locally or use unavailable local proof as
a permanent reason to keep the branch unpublished.

Read the actual job steps and reports: selected/executed subjects, skips,
features, target, runner, command status and candidate identity. A green
aggregate or zero unresolved review threads does not replace semantic review.
An unavailable review provider is missing evidence for that provider, not an
obligation to wait forever if an adequate permitted review path is available.

Release-readiness, package/install rehearsal, immutable-candidate qualification,
source integration and publication are separate fixed-object proof planes. A
pass in one plane must not become another plane's pass in a summary. Historical
local receipts remain evidence for their original object, not current release
authority.

## Golden fixtures and the blessing platform

Golden fixtures are the arbiter of output contracts: `goldens check` decides
whether rendered output matches the committed contract, and its drift reports
(`target/ripr/reports/golden-drift.md` and `.json`) are what a reviewer reads
before any blessing. Linux is the arbiter of goldens: this repository is
developed on Windows and Linux but continuously validated on Linux only, and
the advisory Windows lane deliberately runs no golden gate and no blessing —
a Windows lane that blessed or checked goldens would institutionalize the
#2337 drift it exists to catch (`docs/CI.md`, "Advisory Windows Lane").
Never bless or hand-edit golden expected-files from a Windows host.

A Windows-rendered bless fails on the arbiter platform in recorded shapes:
the same tree produced 43 local Windows drift entries and 35 hosted Linux
entries, and the eight Windows-only entries must not be blessed from a
Windows run (#8437); Windows renders a base-side evidence suffix the
committed Linux goldens do not carry (#8525); and a bless from a CRLF working
tree embedded `input_identity` hashes that no `eol=lf` checkout reproduces,
failing hosted qualification on PR #8559 (run 39072364336) until the fixtures
were re-blessed from an LF tree (#8614; that shape is now refused at bless
time, `xtask/src/reports/fixtures.rs`). The guard cannot catch every platform
difference; the route below is the discipline.

The sanctioned local route on a Windows host:

1. Read the pin from `rust-toolchain.toml` (channel `1.95.0` as of this
   writing; cargo follows the channel).
2. Run the failing `cargo xtask goldens check` under WSL Ubuntu with the
   pinned toolchain, so the arbiter platform — not the host platform —
   observes the drift.
3. Verify the drift is `formatting_only`, or semantically justified under the
   fixture's output contract.
4. Apply the re-bless from the WSL checkout
   (`cargo xtask goldens bless <fixture> --reason "RIPR-SPEC-NNNN: ..."`,
   per [PR automation](../PR_AUTOMATION.md)), so the expected-file bytes come
   from the Linux renderer.
5. Diff the classification content (`expected/check.json`) of every blessed
   fixture: a renderer-wording re-bless records classifications unchanged.

When WSL is unavailable, the hosted route decides instead: publish the
candidate and let the hosted Linux golden gate qualify it; do not substitute
a Windows bless. This plane stays separate from native-Windows evidence —
WSL cannot prove Windows behavior (see [Environment and
shell](#environment-and-shell)); it exists here to produce Linux bytes from a
Windows machine.

Formatting-only versus semantic drift. After a renderer merge (for example
#9657, #10886), prose lines drift across many fixtures at once; the re-bless
keeps `check.json` unchanged and cites the landed renderer change as the
blessing reason (#11301 records "formatting-only, check.json unchanged").
A `check.json` change is semantic drift and needs the landed change's own
justification — for example, #10919 taught TypeScript preview findings the
`canonical_gap_id` field and #10927 re-blessed the affected fixtures to it —
and the honesty invariant
(`cargo xtask check-evidence-promotion-honesty`, RIPR-SPEC-0108) still
rejects a re-bless that promotes evidence without that discriminator.

## Rust

Broad reference commands, not a per-edit checklist:

```bash
cargo check --workspace --all-targets
cargo test --workspace
```

Use the checked test-runner contract for required CI and release qualification.
Do not assert Cargo test/nextest equivalence without comparing subject inventory,
features, doctests, ignored tests, configuration, retries and execution behavior.

## Workflow policy

```bash
cargo xtask check-workflows
```

Run this for changes to `.github/workflows/**`, `policy/workflow_allowlist.txt`,
release workflows or CI/security workflows.

## Droid workflow policy

```bash
cargo xtask check-droid-review-config
```

Run this with `check-workflows` for changes to:

- `.github/workflows/droid.yml` or `droid-security-scan.yml`;
- `docs/agent-context/review-invariants.md` or `droid-smoke-tests.md`;
- `.factory/skills/review-guidelines/SKILL.md` or `.factory/rules/droid-review.md`.

## Security-sensitive review

For secrets, workflows, dependency policy, release scripts or command execution,
inspect permissions, event triggers, fork behavior, artifact/log exposure and
whether secrets can be printed or written. Routine PR repair does not need to
read branch-protection or ruleset settings and never authorizes weakening them.
