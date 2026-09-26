# Quickstart

Run `ripr check` in a Git repository with a committed change:

```bash
ripr check
```

A useful first result is not necessarily a finding. It is one bounded answer:

- one changed behavior whose current test check appears too weak;
- one safe next action; or
- an explicit no-action or limited state when ripr cannot justify repair advice.

A no-action result is not a clean bill of health. It means the current static
evidence did not earn a bounded repair route. Use `--format human-full` for the
complete evidence or `--format json` for machine-readable output.

`ripr.toml` is optional. The zero-config path is the intended first interface.

The public command roles are deliberately separate:

```text
check         analyze one current change
pilot         guide repository adoption and select one work item
agent repair  retain one before/edit/after transaction
first-pr      compose existing artifacts for reviewers
```

See [Command hierarchy](COMMAND_HIERARCHY.md) for the full contract.

## Choose a first-hour path

| Path | Start with | First useful result |
| --- | --- | --- |
| CLI | `ripr check` | One selected gap or an honest no-action/limited state. |
| VS Code | Install `EffortlessMetrics.ripr`; run **ripr: Show Status** | Saved-workspace diagnostics, hover evidence, and bounded actions. |
| GitHub Actions | `ripr init --ci github` | Advisory PR summary and retained artifacts. |
| Coding agent | `ripr pilot --root .` | One repo-scoped work item and the exact before command. |
| MCP client | `ripr mcp --stdio` | Read-only workspace status. |

Public docs use plain language first. Specs and machine output use terms such as
*seam*, *discriminator*, *oracle*, and *canonical gap*; the
[Terminology bridge](TERMINOLOGY.md) maps those terms to the user job.

## Install

### CLI

```bash
cargo install ripr
```

Installing from crates.io builds ripr locally and requires **Rust 1.95 or
newer** with the Rust 2024 edition toolchain.

That is RIPR's build MSRV, not a minimum compiler version for the repository
being analyzed. An already-built ripr binary can statically inspect a
repository that pins an older Rust toolchain. Project verification commands
still use the repository's own selected toolchain and can succeed, fail, or be
unavailable independently of static analysis.

For development from this checkout:

```bash
cargo install --path crates/ripr
```

### VS Code

Install `EffortlessMetrics.ripr` from VS Marketplace or Open VSX. The extension
resolves its server from the configured path, bundled or cached assets,
verified GitHub Release assets, or `PATH`. A separate `cargo install ripr` is
not required for the normal editor path.

### Repository requirements

- Git must be available on `PATH`.
- Analysis must target a Git repository.
- Committed history is the default input. Add `--worktree` to include staged
  and unstaged edits.
- `ripr check` resolves the remote default branch (`origin/HEAD`) and then
  common remote/local fallbacks. Pass `--base <ref>` when you need an explicit
  comparison.

## CLI first hour

### 1. Analyze one change

```bash
ripr check
```

For uncommitted work:

```bash
ripr check --worktree
```

For an explicit base:

```bash
ripr check --base <base-ref>
```

The default human output shows one bounded `Start here:` state. It either names
the top supported gap, reports no current action, or names the limitation that
prevents safe guidance.

Use the other projections only when needed:

```bash
ripr check --format human-full
ripr check --format json > target/ripr/check.json
```

### 2. Select one repo-scoped repair

```bash
ripr pilot --root .
```

`pilot` is the guided repository-adoption path. It prints the work item it
recommends and the exact `ripr agent repair` before command. It also retains the
selected IDs and packet under `target/ripr/pilot/`.

The seam ID used by `agent repair` is repo-scoped. Probe IDs printed by
`ripr check` are diff-scoped and are used by commands such as `explain` and
`context`; the two identifiers are not interchangeable.

### 3. Retain the before state

```bash
ripr agent repair --root . --seam-id <seam-id> --phase before
```

The before phase validates currentness, writes the pre-edit evidence, and
prints the exact `--attempt` command for the after phase. Keep that command.
The repair-attempt ID identifies the prepared transaction.

### 4. Edit one focused test

Make the smallest coherent test or fixture change outside RIPR. Respect the
packet's allowed edit surface, forbidden files, stop conditions, and named
limitations. RIPR does not generate or apply the test edit.

### 5. Finish the same transaction

```bash
ripr agent repair --root . --attempt <repair-attempt-id> --phase after
```

The after phase writes the post-edit static evidence and a receipt. Static
movement and executed project verification remain separate facts. For a
trust-bound Python attempt, a third, separately authorized `--phase verify`
can run the packet's verification route; see
[Repair attempt identity](REPAIR_ATTEMPT.md) and
[Command hierarchy](COMMAND_HIERARCHY.md#repair-transaction).

Keep one selected repository root throughout the transaction. The root,
before/after artifacts, selected work item, verification subject, and receipt
must all describe the same repository and comparable revisions.

### 6. Compose reviewer-facing evidence

After the analyzer and repair artifacts exist:

```bash
ripr first-pr --root . --base <base-ref> --head HEAD
```

Use the repository's actual base ref. `origin/main` is common but not
universal. `first-pr` composes existing artifacts into
`target/ripr/reports/start-here.{json,md}`; it does not analyze the change or
strengthen the evidence.

### Optional low-level control path

The ordinary two-phase repair flow hides most artifact plumbing. The underlying
commands remain useful for debugging and contract checks:

```bash
ripr check --root . --mode draft --format repo-exposure-json \
  > target/ripr/pilot/after.repo-exposure.json

ripr outcome \
  --before target/ripr/pilot/repo-exposure.json \
  --after target/ripr/pilot/after.repo-exposure.json
```

To evaluate an explicit local policy decision, keep the same base identity
through each producer:

```bash
mkdir -p target/ripr
ripr check --base <base-ref> --format json > target/ripr/check.json
ripr review-comments \
  --base <base-ref> \
  --head HEAD \
  --check-output target/ripr/check.json
ripr gate evaluate \
  --pr-guidance target/ripr/review/comments.json \
  --mode acknowledgeable
```

`ripr check` itself is advisory and does not block. `gate evaluate` exits
non-zero only under the explicitly selected gate policy. See
[Calibrated gate policy](CALIBRATED_GATE_POLICY.md).

## VS Code first hour

1. Install `EffortlessMetrics.ripr`.
2. Open one repository or workspace.
3. Run **ripr: Show Status**.
4. Save the changed file.
5. Open the Problems panel and hover a RIPR diagnostic.
6. Use the bounded actions to inspect the related test, copy a repair route, or
   start the current repair.

The editor analyzes saved workspace state. Unsaved-buffer overlays are not the
normal authority. Save or refresh before acting on evidence that may be stale.

If diagnostics do not appear, use:

```text
ripr: Show Status
ripr: Show Output
ripr: Restart Server
```

A missing, ambiguous, stale, untrusted, or removed workspace root must not be
replaced by the language-server process working directory.

Deep links:
[Editor extension](EDITOR_EXTENSION.md),
[Editor evidence workflow](EDITOR_EVIDENCE_WORKFLOW.md), and
[Server provisioning](SERVER_PROVISIONING.md).

## CI first hour

Generate the advisory GitHub workflow:

```bash
ripr init --ci github
```

On a pull request, read the job summary before downloading artifacts. The
summary should name the first-run state, selected gap or limitation, safe next
action, artifact links, and gate-authority boundary. The uploaded packet keeps
the full machine evidence.

Do not make generated CI blocking until the repository has reviewed its first
advisory baseline and explicitly adopted a gate policy.

See [CI strategy](CI.md), [PR review guidance](PR_REVIEW_GUIDANCE.md), and
[Blocking readiness](BLOCKING_READINESS.md).

## Preview languages

TypeScript, JavaScript, and Python findings remain preview/advisory. They do not
become Rust-parity evidence merely because their adapters ship in the normal
binary.

The TypeScript-family adapter recognizes:

```text
.ts  .tsx  .mts  .cts  .js  .jsx  .mjs  .cjs
```

Modern module extensions are first-class diff-analysis inputs. Some downstream
repair and targeted-rerun surfaces can still under-emit for `.mts`, `.cts`,
`.mjs`, and `.cjs`; an absent packet or rerun route is therefore not evidence
that the file was ignored or that the change is safe.

Python static facts are preview. A scoped repair route is `usable alpha` only
for selected pytest/unittest findings that carry every required fact: current
owner, related test, missing boundary/value/effect, test location, verify
command, and bounded edit surface. Other findings remain inspect-only or
limited.

Read [Language adapter preview](LANGUAGE_ADAPTER_PREVIEW.md) and
[Support tiers](status/SUPPORT_TIERS.md) before adopting preview evidence as
policy.

## Doctor and troubleshooting

Use `ripr doctor --root .` to inspect:

- repository and configuration discovery;
- enabled and unavailable language adapters;
- Git and relevant tool state;
- cache and first-use artifact state; and
- exact recovery for missing prerequisites.

`doctor` is a diagnostic command, not the first-value analysis path. Keep these
capabilities separate:

```text
run an already-built ripr binary
statically analyze the repository
run the repository's project verification
build or install ripr from source
```

RIPR's Rust 1.95 build MSRV belongs only to the last capability. A repository's
own toolchain governs its project verification; it does not determine whether
static analysis by an already-built binary is conceptually available.

Common evidence states:

- **missing artifact** — regenerate it through the named producer;
- **stale evidence** — rerun against the current root and revisions;
- **wrong root** — stop rather than copying artifacts between repositories;
- **malformed artifact** — reject it and regenerate;
- **preview limitation** — inspect the named static boundary; do not invent a
  repair target; and
- **failed verification** — retain the failure; do not issue a successful
  receipt.

## Trust boundary

RIPR is static mutation-exposure analysis. It does not run mutants, edit
production code, generate whole tests, replace coverage, or prove correctness
or test adequacy.

The Rust repair transaction is `usable alpha`: package, editor, bounded packet,
and before/after paths exist when RIPR emits a complete route, while governed
real-repository route yield and ordinary-user success are still being measured.
Preview language evidence remains advisory. A skipped, partial, stale,
wrong-subject, zero-subject, or unavailable required result is not a pass.

## Next references

- [Terminology](TERMINOLOGY.md)
- [Static exposure model](STATIC_EXPOSURE_MODEL.md)
- [Targeted test workflow](TARGETED_TEST_WORKFLOW.md)
- [First successful PR workflow](FIRST_PR_WORKFLOW.md)
- [Output schema](OUTPUT_SCHEMA.md)
- [Support tiers](status/SUPPORT_TIERS.md)
- [Documentation index](README.md)
