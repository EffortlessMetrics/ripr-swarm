# Quickstart

Inspect a change, read the testing gap, and decide which focused test to add.
ripr uses static analysis; it does not run mutants or write the test for you.

## Choose Your Path

[CLI](#cli-first-hour) · [VS Code](#vs-code-first-hour) ·
[GitHub CI](#ci-first-hour) · [Agent or reviewer](#agent-or-reviewer-first-hour)

## Installation

Fastest, with no Rust toolchain: download the checksummed prebuilt 0.10.0
archive (about a second on Linux). Pick your platform; the
[release](https://github.com/EffortlessMetrics/ripr/releases/tag/v0.10.0) has
archives for Linux and macOS on x86-64 and ARM64 and for Windows on x86-64.
`.github/workflows/first-run-archive.yml` runs exactly these two snippets on
hosted runners and records the timings (#5311).

Linux and macOS (Bash or zsh):

<!-- prebuilt-snippet:unix -->
```bash
v=0.10.0; case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) t=x86_64-unknown-linux-gnu;; Linux-aarch64) t=aarch64-unknown-linux-gnu;;
  Darwin-x86_64) t=x86_64-apple-darwin;; Darwin-arm64) t=aarch64-apple-darwin;;
  *) echo "no prebuilt archive for this platform; install with cargo install ripr or build from source" >&2; exit 1;; esac
f=ripr-server-v$v-$t.tar.gz
b=https://github.com/EffortlessMetrics/ripr/releases/download/v$v
curl -fsSLO $b/$f && curl -fsSLO $b/$f.sha256
echo "$(cat $f.sha256)  $f" | { sha256sum -c - || shasum -a 256 -c -; } \
  && tar xzf $f && export PATH="$PWD:$PATH"   # ./ripr is now first on PATH for this shell
```

Windows (PowerShell):

<!-- prebuilt-snippet:powershell -->
```powershell
$v='0.10.0'; $t='x86_64-pc-windows-msvc'; $f="ripr-server-v$v-$t.zip"
$b="https://github.com/EffortlessMetrics/ripr/releases/download/v$v"
Invoke-WebRequest "$b/$f" -OutFile $f; Invoke-WebRequest "$b/$f.sha256" -OutFile "$f.sha256"
if ((Get-FileHash $f -Algorithm SHA256).Hash -ne ((Get-Content "$f.sha256") -split '\s+')[0].ToUpper()) { throw 'checksum mismatch' }
Expand-Archive $f -DestinationPath ripr-bin; $env:PATH="$PWD\ripr-bin;$env:PATH"   # ripr.exe is now first on PATH for this session
```

The PATH change in either snippet lasts only for the current shell (PowerShell:
the current session). To keep `ripr` available across sessions, add the
extracted directory to your permanent PATH — for example move `ripr` to
`~/.local/bin/` (adding it to PATH in your shell profile if needed) on Linux or
macOS, or add the directory under Settings → Environment Variables on Windows.

The macOS archives are not signed or notarized. A `curl` and `tar` download like
the one above normally carries no quarantine attribute, so `./ripr` runs
without a prompt. A browser, AirDrop or mail download of the archive is
quarantined, and Gatekeeper then blocks the extracted `ripr` with a dialog such
as "Apple could not verify "ripr" is free of malware" or ""ripr" cannot be
opened because the developer cannot be verified". Verify the checksum first,
then clear the attribute with `xattr -d com.apple.quarantine ./ripr` (or
`xattr -dr com.apple.quarantine <extracted-dir>`). Do that only for an archive
whose `.sha256` matched.

Or install the published CLI with Cargo, which compiles from source:

```bash
cargo install ripr
```

Cargo requires Rust 1.95 or newer. Git must be available for the diff workflow.
The editor installation below normally does not require Cargo.

If Cargo refuses because ripr requires rustc 1.95, run `rustup update stable`.
Inside a repository whose `rust-toolchain.toml` pins an older Rust, Cargo uses
that pin; install from another directory or with `cargo +stable install ripr`.
Cargo may name an older ripr that supports your compiler; that release predates
this guide.

Rust 1.95 is needed to build ripr, not by the repository you analyze: ripr's
static analysis does not run your compiler, so a repository pinned to an older
toolchain can still be analyzed. Your own tests still run with that repository's toolchain.
`ripr doctor --profile source-build` checks what building ripr needs and names
an install route if cargo or rustc is missing. It does not recommend `ripr check`.

The latest GitHub release is [0.10.0](https://github.com/EffortlessMetrics/ripr/releases/tag/v0.10.0).
This guide describes **0.11 development**, including `--worktree`, bounded
`Start here:` output, and durable repair attempts. Those instructions are not a
claim that the published package has these features. For a released install,
use the [versioned README](https://github.com/EffortlessMetrics/ripr/blob/v0.10.0/README.md)
and that binary's help. The 0.10 CLI resolves its default base by trying
`origin/HEAD`, then `origin/main`, `origin/master`, `main`, and `master`; use
`--base REF` with another existing branch or commit when needed.

To use this guide's development features, install the development build
(Rust 1.95 or newer):

```bash
cargo install --locked --git https://github.com/EffortlessMetrics/ripr-swarm ripr
```

From a `ripr-swarm` checkout, `cargo install --path crates/ripr` does the same.
Either route compiles from source. A cold `cargo build --release` of the
workspace (thin LTO, 16 codegen units) took about 6 minutes (366 s) on a 4-core
Linux container in October 2026, against 17 minutes (1037 s) for the earlier
fat-LTO, single-codegen-unit profile; `cargo install ripr` for 0.10.0 took about
2 minutes on an earlier run. The packaged 0.11.0 crate carries no profile
section, so `cargo install ripr --locked` should build in about 4.6 minutes
(275 s) once 0.11.0 is on crates.io (measured by building the packaged crate with
`cargo build --release` on the same container, not a download). In that walk,
`doctor` and `check` each took under 3 seconds on small crates; `ripr pilot`
takes longer on large repositories. The prebuilt 0.10.0 archive downloads in
about a second, against about two minutes to compile 0.10.0 from source. There is
no prebuilt 0.11 archive until 0.11.0 is published.
`cargo xtask first-run --install-archive 0.10.0` replays the archive route end
to end and times it; the [first-run scoreboard](../benchmarks/dx_scoreboard/README.md)
tracks the install time so a rise shows.

With [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) installed,
`cargo binstall ripr` uses the prebuilt 0.11.0 release archive when that asset is
published, and compiles from source when it is not. Releases before 0.11.0 carry
no binstall metadata. See [Install channels](INSTALL_CHANNELS.md) for what
each channel serves today.

For another installation method or a pinned server, see
[Server provisioning](SERVER_PROVISIONING.md).

## CLI First Hour

From a Rust repository with a change to inspect:

```bash
ripr check
```

In this development build, `check` uses uncommitted tracked edits when present,
otherwise committed history against the default base. On a clean checkout of
the base branch, edit a tracked source file and run `ripr check` again. For a new
file, first use `git add -N PATH` so it enters the tracked diff. See
[Choose the change](#choose-the-change) for explicit diff selection.

Use `ripr doctor` to diagnose setup if analysis cannot complete; it is not a
required first step. (doctor exits 3 when any check fails, so run it
standalone — it does not chain with `&&`.) To explore a clean repository without making a change,
use the separate [repository exploration](#explore-the-repository) path,
`ripr pilot --root .`.

No `ripr.toml` is required. The development CLI prints a summary and a bounded
`Start here:` section with a finding, a no-action result, or the limitation
preventing useful guidance. Read the changed behavior, related test, and
suggested next test. No findings does not mean the tests are complete.

While analysis is running, `ripr check` writes producer stages to **stderr**
as `ripr progress: <stage> [<scope>]`, with throttled heartbeats if a stage
stays active. JSON, SARIF, and other machine stdout stay byte-clean. Unknown
totals never become a percentage or ETA. Progress does not mean analysis is
faster or that the command will succeed. `--quiet` suppresses the progress
stream; it does not change findings or exit codes. `ripr explain` and
`ripr context` print the same `ripr progress:` lines on stderr and accept
`--quiet`, as does `ripr pilot`. With a machine format (`--json`, `--format
json`, `sarif`, `badge-*`, `agent`) or `ripr context` and a stderr that is not a
terminal, progress is off by default (set `RIPR_PROGRESS=1` to restore it);
`RIPR_QUIET` and `--quiet` silence it everywhere, including `ripr repair` and
`ripr continue`. Stdout is the output contract, so for machine capture (for
example the JSON from `context`) keep stderr separate rather than `2>&1`.

A short glossary for the terms below:

| Term | Meaning |
| --- | --- |
| seam | A place in the code where behavior could change; a repair targets one seam ID. |
| discriminator | The check that would tell the old behavior from the new. |
| oracle / assertion | What a test compares against; a weak oracle (for example `> 0`) reaches the code but does not notice the change. |
| grip | How firmly tests hold a behavior, for example `weakly_gripped` or `strongly_gripped`. |
| `probe:` ID | Names one finding in one diff. `explain` and `context` take it; `ripr repair` resolves it to its seam, then may refuse if that seam is not repair-eligible. |
| edit cage | The repair attempt's rule: only new test functions inside the allowed test surface (for inline tests, the existing `#[cfg(test)] mod tests`) may be added; production code and existing tests stay unchanged. |
| receipt | The static before/after evidence record from the after phase; not a test result. |

Human output uses plain words for the schema values: `appear noticed` is
`exposed`, `partly noticed` is `weakly_exposed`, `reached but not noticed` is
`reachable_unrevealed`, and `no test path` is `no_static_path`. Use
`--format human-full` or JSON to see the schema value. See
[Terminology](TERMINOLOGY.md) for the seam-level words and
[RIPR-SPEC-0122](specs/RIPR-SPEC-0122-bounded-human-check-output.md) for the human output wording.

Agents and scripts reading `ripr check --json` get a stderr free of
`ripr progress:` lines when stderr is a pipe or file; pass `--quiet` (or set
`RIPR_QUIET=1`) to make that unconditional, including on a terminal. The JSON is the full versioned report and
is large: read `summary` and `canonical_next_action` (when present) first, then fetch one
finding's evidence with `ripr context --at <finding-id> --json --quiet`.

When a finding is selected, copy its printed `ripr explain` command to inspect
the evidence, or its `ripr context` command for an agent handoff. Those commands
retain the root, diff selection, mode, and finding ID. Do not substitute an ID
from a documentation example.

To expand the output:

```bash
ripr check --format human-full
ripr check --format json
```

If `check` offers a finding but no `ripr repair` start, run
`ripr pilot --root .`: it ranks the whole repository differently and may name a
supported repair that `check` did not.

A finding can guide an ordinary test improvement through the
[daily loop](#check-test-and-refresh-the-same-change) below. For a governed
repair attempt with before/after artifacts, the
[repair workflow](#agent-or-reviewer-first-hour) prepares its supported route
before the test edit.

### Choose the change

By default, `check` analyzes the working tree when the repository has
uncommitted tracked changes, and committed history when the tree is clean.
This applies with or without `--base`. Force the working-tree read, or force
committed history even when the tree is dirty, with:

```bash
ripr check --worktree
ripr check --committed
```

`--worktree` always reads staged and unstaged tracked edits. `--committed`
always diffs `<base>...HEAD`, even when the tree is dirty.
Untracked files never select the working-tree default and stay out of the
diff until staged (`git add -N`).

The CLI resolves its default base by trying `origin/HEAD`, then `origin/main`,
`origin/master`, `main`, and `master`. To choose another comparison, replace
`REF` with an existing branch or commit:

```bash
ripr check --base REF
```

On an unchanged base branch, an empty diff is expected. To analyze a saved patch,
use `ripr check --diff PATH`, replacing `PATH` with your patch file. The common
options are in `ripr help check`; `ripr help check --all` lists every option,
output format, and environment variable.

### Check, test, and refresh the same change

Use this loop while developing a change or reviewing a test improvement. It
uses the installed development CLI described above and the project's own test
runner; it does not require a repair attempt.

**Keep the production comparison fixed.** From the repository root, choose a
base before the production change you want to examine. For a branch targeting
`origin/main`, this POSIX-shell example resolves that base once:

```bash
ripr --version
RIPR_BASE=$(git merge-base HEAD origin/main) &&
  ripr check --root . --base "$RIPR_BASE" --worktree
```

Replace `origin/main` with your review base when needed. A repository with no
`origin` remote (for example a local demo) fails here with exit 128
(`Not a valid object name origin/main`); use `main`, or your real base branch,
instead: `git merge-base HEAD main`. Confirm that the
output names the intended production file and changed behavior. Keep the same
root, resolved base, source mode, and analysis mode throughout the loop.
`--worktree` includes the production change since that base plus the current
tracked test edits, even when the production change is already committed.
For a new file, first use `git add -N PATH` as described above.

**Inspect the current finding.** Follow its printed `ripr explain` or
`ripr context` command. Read the missing discriminator and related test, then
check the expected behavior against the project's requirements. A context
packet is advisory: an unavailable assertion template or missing native test
command is not permission to invent one. Make a justified test edit using the
project's existing test style, preserving the intended assertion and input
domain. Leave the production behavior unchanged during this test-only loop.

**Run the native test.** Use the project's documented runner, package, target,
and filter for the edited test. For example, in a Rust crate with
`tests/pricing.rs` containing `equality_boundary_discounts`:

```bash
cargo test --test pricing equality_boundary_discounts -- --exact
```

For an inline `#[cfg(test)] mod tests` in `src/lib.rs`, the exact name includes
the module path:

```bash
cargo test --lib tests::equal -- --exact
```

`cargo test --lib equal -- --exact` matches nothing there: it runs 0 tests and
still exits 0. Confirm the summary says `1 passed`. Use your actual test name
and target. Check the native exit status and test
summary: the intended test must execute and pass. A zero exit with zero tests
selected, an ignored test, a compile failure, or a timeout does not verify the
edit. `ripr check` does not run that command for you.

**Refresh the original comparison.** After the test edit and native run:

```bash
ripr check --root . --base "$RIPR_BASE" --worktree
```

Match the original production file, owner, changed behavior, and canonical gap
ID when present; `--format json` exposes these details. Test edits change the
analysis input, so a new input identity is expected. Do not switch to
`--base HEAD` after committing the production change: that can leave only the
test edit in the comparison. A quiet `no_behavioral_candidates` result then
says nothing about the original production gap.

Keep the two results separate:

| Native test result | Refreshed result for the original behavior | What to do |
| --- | --- | --- |
| Intended test executed and passed | `exposed` | Record the focused native pass and improved static evidence. This is not a runtime mutation result or proof of overall correctness. |
| Intended test executed and passed | Gap remains weak or unrevealed | Inspect the remaining discriminator in the current output; passing the test alone did not establish stronger static evidence. |
| Failed, unavailable, timed out, did not compile, or did not execute the intended test | Any classification, including `exposed` | Keep the native failure or missing verification visible and resolve it before calling the test improvement verified. |
| Any | Partial/unsupported analysis, or the original production subject is absent | Record the limitation, check the comparison and configuration, and follow the current recovery guidance. Do not count an absent subject as an improvement. |

`check` exits 0 for completed analysis, including weak and exposed findings.
Read its analysis outcome and findings; that exit code is not a test result or
a CI gate. Refresh again after further edits rather than relying on an older
context packet. If you need an attempt receipt and its edit constraints, use
the separate [repair workflow](#agent-or-reviewer-first-hour).

### Explore the repository

For broader repository analysis and a suggested Rust repair, run:

```bash
ripr pilot --root .
```

This is broader than checking one change. It writes reports under
`target/ripr/pilot/` and either names a supported next action or explains why
no repair is ready. Follow its retry guidance when analysis is partial.
Non-Rust workspaces are routed to the appropriate diff-analysis or
unavailable-adapter guidance, not the Rust repair sequence.

### Python first hour (preview)

Python analysis is `preview`: its evidence is advisory, not Rust-grade. A
Python project needs no `ripr.toml` — when the repository root has a Python
project marker (`pyproject.toml`, `setup.py`, `setup.cfg`,
`requirements.txt`, `pytest.ini`, `tox.ini`, or `noxfile.py`) or Python source
under `src/` or `tests/`, the CLI auto-enables the preview. Work from a Python repository on a branch with a change
to inspect.

Before the first run, add `/target/ripr/` (or `/target/`) to `.gitignore`:
ripr writes its artifacts under `target/ripr`, and `check`, `pilot` and
`agent repair` all create it, so ignoring it keeps `git add -A` from
committing ripr's own reports. Then run:

```bash
ripr check
```

On a project with markers, `check` analyzes the changed Python files and
reports the boundary gap with a pytest (or unittest) verify command; its
top finding is labeled `State: preview language, advisory only`. For the
repository-wide ranking and a repair recommendation, run:

```bash
ripr pilot --root .
```

When the Rust ranking is empty, pilot's top recommendation is the Python
repair card (`language: python (preview)`) and its `Next, in order:` route;
run the steps it prints. Both commands write under `target/ripr/` (see the `.gitignore` note above;
doctor and pilot also say so on the first run when no ignore rule covers it).

A repository with no marker and no Python source under `src/` or `tests/`
leaves the preview disabled; `check` then skips
Python files and prints:

```text
Note: this diff contains 1 Python file. The Python adapter is preview and not enabled, so these files were not analyzed — this is NOT a clean Rust-grade result. Enable it in ripr.toml [languages] to analyze them.

To enable, add to ripr.toml:

[languages]
enabled = ["python"]
```

Enable it exactly as printed when the preview is not auto-enabled. Python
repair routes and their evidence limits are described in
[Known Limits](#known-limits) and
[Language adapters](LANGUAGE_ADAPTER_PREVIEW.md).

For Python and TypeScript the repair route is: `ripr check` -> `ripr agent
repair --gap-id <gap> --base <base> --phase before` (check prints the exact
command) -> edit the one named test file -> commit it -> `ripr continue
--attempt <id>`. `ripr repair` (seam-based) and `ripr agent stub` are
Rust-only.

### TypeScript first hour (preview)

TypeScript analysis is `preview`: its evidence is advisory, not Rust-grade. A
TypeScript project needs no `ripr.toml` — when the repository root has a
`tsconfig.json` or `jsconfig.json` (or TypeScript source under `src/` or
`tests/`), or a package one level down or under `packages/*`, `apps/*`,
`libs/*` or `libraries/*` has one, the CLI auto-enables the preview. Work from a TypeScript repository on
a branch with a change to inspect.

Before the first run, add `/target/ripr/` (or `/target/`) to `.gitignore`:
ripr writes its artifacts under `target/ripr`, and `check`, `pilot` and
`agent repair` all create it. Then run:

```bash
ripr check
```

`check` analyzes the changed TypeScript files; its top finding is labeled
`State: preview language, advisory only` and, when the repair packet is
complete, names the test file, the missing case and a verify command such as
`npx --no-install vitest run tests/pricing.test.ts`. It also prints the route
under `Next: drill into the top finding:`. Start the repair with the
`ripr agent repair` line it prints:

```bash
ripr agent repair --gap-id gap:typescript:... --base main --phase before
```

Edit only the one test file the packet names (production code stays
unchanged), **commit** the test, then run the `ripr continue` line the
command printed:

```bash
ripr continue --attempt repair-attempt-...
```

`continue` runs the after phase: when the gap reads `exposed` it writes the
receipt under `target/ripr/reports/`. It compares static evidence only and
does not run your tests, so run the verify command from `check` yourself after
`continue` succeeds (some TypeScript runners write cache files that the attempt
would otherwise observe) and keep the test only if it passes.

`ripr repair` (seam-based) and `ripr agent stub` are Rust-only: on a
TypeScript path `ripr repair` points back to `ripr check`, and `ripr agent
stub --at src/file.ts:N` exits `3` with `stub generation supports Rust files
only`. A repository with a `ripr.toml` that sets `enabled = ["rust"]` keeps
TypeScript off. Evidence limits are described in
[Known Limits](#known-limits) and
[Language adapters](LANGUAGE_ADAPTER_PREVIEW.md).

## VS Code First Hour

Install [EffortlessMetrics.ripr](https://marketplace.visualstudio.com/items?itemName=EffortlessMetrics.ripr)
from VS Code Marketplace or [Open VSX](https://open-vsx.org/extension/EffortlessMetrics/ripr),
then open a Rust/Cargo workspace. The released extension normally resolves a
matching native server without `cargo install ripr`.

Open Problems and hover a ripr diagnostic to inspect the changed behavior,
related test, and missing assertion. Use the available focused-test actions to
open the test or copy a brief for an agent. You write the test in your editor;
ripr does not apply it.

The development extension can start a supported attempt with
`ripr: Start Current Repair`. A source-built extension needs a compatible
server; do not assume an unreleased matching server is downloadable. Follow
[Editor extension](EDITOR_EXTENSION.md) and [Server provisioning](SERVER_PROVISIONING.md)
for local builds and explicit server paths.

If nothing appears, run `ripr: Show Status`, then `ripr: Show Output` to inspect
setup or analysis errors. Save files before refreshing: the editor analyzes the
saved workspace only; unsaved buffer content is never analyzed.

See [the editor repair walkthrough](EDITOR_FIRST_RUN_TO_FIRST_RECEIPT.md) for
verification and receipt steps. For Remote-SSH, containers, or WSL, install the
extension on the workspace's host. Browser-only VS Code is not supported.

## CI First Hour

Generate the advisory GitHub workflow:

```bash
ripr init --ci github
```

Review the generated files before committing them: `init` writes a `ripr.toml`
beside the workflow. That file pins `[languages] enabled = ["rust"]`, which
turns off the Python and TypeScript previews for the repository. `init` warns
when the root looks like a Python or TypeScript project; add that language to
`enabled` or delete the `[languages]` section. The workflow
installs the ripr version that generated it. An unreleased development build
instead pins the latest release and says so in a note; after
upgrading ripr, refresh with `ripr init --ci github --force`. Preview the
files first with `--dry-run`. On a PR, read the summary comment it posts on the
conversation (one comment, updated on every push; same-repository PRs only), then the job summary
and the linked artifacts for the evidence and suggested test.
The generated workflow is non-blocking by default; a policy gate is a separate,
explicit adoption decision. It also uploads SARIF to the Security tab; that needs
code scanning (free on public repositories, GitHub Code Security on private
ones). Where it is unavailable the upload fails without failing the job, and
the run page shows a `RIPR SARIF upload failed` warning; set
`RIPR_UPLOAD_SARIF` to `"false"` in the workflow to stop trying.
The workflow runs on `ubuntu-latest` and pins `shell: bash`, so its steps
still parse if you move the job to another runner; only the Linux runner is
covered by ripr's own tests.

Upgrading from 0.10? See the
[0.10 to 0.11 upgrade guide](INSTALL_CHANNELS.md#upgrading-from-010-to-011).

The [copyable CI recipe](CI.md#copyable-ripr-advisory-workflow) is the downstream
usage section of the repository's CI guide. See [PR review guidance](PR_REVIEW_GUIDANCE.md)
for reading results and [Blocking readiness](BLOCKING_READINESS.md) before
choosing a gate.

## Agent Or Reviewer First Hour

This is the **development Rust repair path**. Start from a current supported
route, not an arbitrary finding ID:

```bash
ripr pilot --root .
```

**Before you start:** commit your change and make sure `/target/` is in
`.gitignore` (details below). The before phase refuses a seam whose
edit-target file has uncommitted changes. When pilot recommends a repair, run the exact
`ripr agent repair ... --phase before` command it prints. That printed command
is the advanced spelling of the ordinary start `ripr repair <seam-id>`; both
begin the same before/edit/after attempt, and the ordinary start additionally
prints the repair card for the seam. The card is long; the command to run next
is on the `Next, after the test edit:` line, printed just before the card.
The printed command carries pilot's seam ID, which names a code
location across the repository. A `probe:...` ID printed by `ripr check`
names a finding in one diff; `ripr repair` resolves it to its seam but may then
refuse if that seam is not repair-eligible.

Before starting a Rust repair, ensure Git ignores Cargo's entire `target/`
directory, for example with `/target/` in `.gitignore`. Effective local or global
Git exclude rules also work. The before phase checks this even before a build
exists and stops with recovery guidance if it is missing; ignoring only
`target/debug/` does not cover Cargo's root-level build files. Set the rule before
the attempt so running the focused test cannot turn build output into an edit.

The before phase prepares the packet and prints an `--attempt` continuation
command. Read the allowed test files, proposed assertion, verification command,
and stop conditions. Make the focused test edit yourself or delegate that packet
to an external coding agent, and change only test files. Run the test yourself:
ripr does not run it. The attempt refuses any new file outside the allowed test
files, whether Git ignores it or not, except build output under a `target/`
that was already gitignored when the before phase ran; a `.gitignore` change
made mid-attempt is itself refused. Redirect ripr output under `target/ripr/` or
outside the repository.

Then run the after-phase command the before phase printed:
`ripr continue --root ... --attempt <id>` (the advanced spelling is
`ripr agent repair ... --attempt ... --phase after`). Plain `ripr continue`
selects the attempt when the choice is unambiguous. Run before you have edited
the test, it exits 3 (recoverable): the attempt stays `awaiting_edit`, and you
can rerun it after the edit. Each premature run counts as one of the attempt's
5 after-phase refusals, so edit the test first.
It records the after snapshot and a receipt of static evidence movement.
Keep the test execution result separately: a static improvement is not proof
that a test ran or that a mutant was caught.

No complete route, partial analysis, stale evidence, or an unsupported target
means stop and inspect the guidance, not guess a repair command.

To resume an interrupted attempt:

```bash
ripr status --root .
```

(`ripr agent status --root .` is the advanced spelling of the same report.)
Before you have edited the test, `Status: incomplete` with state
`awaiting_edit` is normal. The command to run is in the fenced `Next Command`
block of the report.

Use the reported continuation or recovery step. The full phase and identity
reference is [Repair attempt identity](REPAIR_ATTEMPT.md).

When pilot's top recommendation is a Python repair card, it prints a different
route under `Next, in order:`: `ripr first-pr` (run any regeneration command it
prints), the focused test edit, the card's verify command, and the receipt
command `first-pr` named. Run those as printed. A trust-bound Python repair
through `ripr agent repair` is a separate governed route that needs a selection
manifest and adds a `verify` phase; see
[the Python sequence](REPAIR_ATTEMPT.md#governed-python-sequence).

For lower-level control, see [Agent workflows](AGENT_WORKFLOWS.md) and
[the LLM operator guide](LLM_OPERATOR_GUIDE.md). No LLM provider is called by
these commands.

### Agent repair details

Reference details for driving ripr from a coding agent:

- `ripr check` prints the `ripr explain` and `ripr context` commands for the top
  finding. With `--format json` the result embeds the same bounded
  `canonical_next_action` (class, selected item, root, diff source, and followable
  route). `ripr doctor --json` carries the same `canonical_next_action` when setup
  needs a precise recovery, matching the human Safe-next-action line.
- Run the follow-up commands ripr prints (`ripr explain`, `ripr context`,
  `ripr agent repair ...`) exactly as printed: their IDs belong to that run and
  cannot be copied from documentation. `agent repair --seam-id` and `ripr repair`
  accept the seam ID that `pilot` prints. They also resolve a `probe:...`,
  `repo-probe:...` or `gap:...` finding ID from `ripr check` to its one seam. If
  the ID is stale, ambiguous, or its seam is not repair-eligible they refuse and
  start no attempt. Run `ripr pilot --root .` to list current seam IDs.
- The compact repair card is the default bounded handoff to a coding agent:
  `ripr agent card --root . --seam-id <seam-id> --json` projects the
  fix-instruction summary, repair-route readiness, and canonical next action, with
  the full packet behind the card's explicit detail route. A successful
  `ripr repair` start prints the same card.
- Between the repair phases, the attempt refuses any new file outside the allowed
  test files, whether Git ignores it or not. When your tests live in an inline
  `#[cfg(test)] mod tests` inside the source file (the `cargo new --lib` layout),
  the repair edits that file, but only by adding new test functions inside that
  one module; any other change to the file fails the attempt. Build output under a
  gitignored `target/` is the exception. Before starting a repair, make sure
  `target/` is in a committed `.gitignore`; changing `.gitignore` mid-attempt is
  itself refused. Redirect ripr output under `target/ripr/` or outside the
  repository.
- Exit `0` means the command completed and `2` means it could not, which includes
  the file refusal above; `3` means ripr declined: a gate blocked, or it refused a
  request, such as a repair after phase refused for a named cause. See
  [exit codes](EXIT_CODES.md). The [LLM operator guide](LLM_OPERATOR_GUIDE.md)
  covers the full repair loop.

### Prepare a PR summary

After generating the needed evidence, use `ripr first-pr` to compose the
PR-facing packet. By default it does not run analysis (`--run-chain` opts in,
below), and it never repairs the code. See
[First successful PR workflow](FIRST_PR_WORKFLOW.md) for the inputs and
`ripr help first-pr` for its options.

On a fresh checkout the first `first-pr` run prints one next command (the
`Missing artifact` regeneration command for repo exposure) and names the later
steps (gap ledger, then review cards) on a `Later steps` line without their
commands. Run the printed command, then rerun `first-pr`; it prints the next
command each time. It reads artifact presence only and runs none of them. To
have `first-pr` run that missing-artifact chain itself, pass `--run-chain`:
`ripr first-pr --run-chain` runs the repo-exposure report, the gap ledger and
the review cards in order, each only when its artifact is missing (the cards
only when the gap ledger names no repair start) and each exactly the command
`first-pr` would have printed (with the resolved `--base` and `--head`), then
composes the packet in the same invocation. It is opt-in because it runs
analysis (cost scales with the repository, minutes on a large one), writes the
missing artifacts under `target/ripr/` and uses no network. The first failing
step, or a step that exits 0 without leaving a non-empty, parseable `.json`
artifact, stops it with exit `2` and names the step; no packet is written from
partial artifacts. It cannot be combined with `--check` or `--check-output`.
The packet JSON (`selected.also_missing`) still carries every later step with
its literal command. Commit your test first, because the
committed diff does not include uncommitted edits. After a successful repair
the final state `no actionable gap` is expected: the gap is already addressed.
It also appears when no repairable PR-local gap was selected; read the printed
`Reason:` line to tell which.

## Troubleshooting

| Symptom | Next step |
| --- | --- |
| Cargo installation fails. | Check the first Cargo error and `rustc --version` (Rust 1.95 or newer). Fix the reported build or download problem before retrying. |
| ripr is installed, but repository setup fails. | Run `ripr doctor` and inspect its individual capability results. |
| `check` sees no change after an edit. | Uncommitted tracked edits are included by default in this development build. Confirm the resolved base (`--base REF`). Untracked files stay out of the diff until `git add` or `git add -N`. Pass `--committed` to analyze committed history only. A published 0.10 install still needs `--worktree`. |
| The wrong base is selected. | Use `--base REF` with an existing reference in this repository. |
| Configuration is rejected. | Run `ripr config validate` and fix the named setting. Configuration is optional, but an invalid file is not ignored. |
| Editor diagnostics are missing or stale. | Check `ripr: Show Status`, save the file, and use the extension's refresh action. |
| The editor cannot start its server. | Check [Server provisioning](SERVER_PROVISIONING.md), especially the version and remote host. |
| An attempt cannot continue. | Run `ripr status --root .` and follow [repair recovery](REPAIR_ATTEMPT.md). |
| Analysis is partial or limited. | Read the limitation and suggested retry. Missing evidence is not a successful empty result. |

Still stuck, or ripr gave a wrong or confusing answer? Open a [first run report](https://github.com/EffortlessMetrics/ripr-swarm/issues/new?template=first_run_report.yml) or a [wrong finding report](https://github.com/EffortlessMetrics/ripr-swarm/issues/new?template=wrong_verdict.yml) with your `ripr --version`.

## Known Limits

The Rust analysis and bounded repair workflows are `usable alpha`; a valid
route is required, and ordinary real-repository route yield and success remain
unestablished. Python analysis is `preview`, with selected pytest/unittest
repair routes at `usable alpha`. Only complete weak-evidence routes get repair
cards; zero cards is normal for unsupported or non-repairable findings.
`ripr repair` and `ripr agent stub` support Rust files only.

TypeScript is enabled automatically for TypeScript-shaped repositories without a
`ripr.toml`; JavaScript is an opt-in preview. Diff analysis recognizes:

```text
.ts  .tsx  .mts  .cts  .js  .jsx  .mjs  .cjs
```

In the development build, these extensions also share the language
classification used by repair eligibility, related-test packet labels, and
targeted reruns. Recognizing a file does not make its findings repairable:
a complete supported route and sufficient evidence are still required. Read
the reported limitation or next action; an absent repair packet is not evidence
that the change is safe.

Perl needs a `lang-perl` build and the unpublished `perl-ripr-facts` exporter,
so it is not usable from a released build/exporter combination.

The CLI does not generate tests, apply code edits, run mutants, or turn an empty
result into proof of correctness. Generated CI is advisory by default.
See [Support tiers](status/SUPPORT_TIERS.md), [Static limits](STATIC_LIMITS.md),
and [Language adapters](LANGUAGE_ADAPTER_PREVIEW.md) for supported cases.

## Next Docs

[Command guide](COMMAND_HIERARCHY.md) · [Configuration](CONFIGURATION.md) ·
[Targeted tests](TARGETED_TEST_WORKFLOW.md) · [Output formats](OUTPUT_SCHEMA.md) ·
[Terminology](TERMINOLOGY.md)
