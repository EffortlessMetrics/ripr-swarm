# RIPR-SPEC-0009: Defaults-First Adoption

Status: proposed

## Problem

`ripr` now has useful static seam evidence, editor actions, SARIF, badges,
targeted-test receipts, and calibration reports. The product risk has shifted
from missing machinery to too much operator assembly.

A new user should not need to understand every report format, `cargo xtask`
helper, SARIF policy mode, badge count basis, LSP cockpit report, or
calibration artifact before getting value. The default experience should expose
one conservative loop:

```text
install ripr
run one pilot scan
write one focused test
compare before and after evidence
materialize repo policy when the team wants to review or tune defaults
optionally import runtime calibration data
```

Defaults-first adoption defines how each install surface behaves before teams
start tuning policy.

## Behavior

Every public surface should be useful with built-in defaults, while preserving
explicit configuration for teams that need policy control.

The default contract is:

- `ripr.toml` is optional for first value. Missing config uses built-in
  conservative defaults and is reported as such by `ripr doctor`.
- `ripr init` materializes repo policy. It does not unlock the useful
  experience.
- `ripr init --ci` accepts only `github`. Unknown providers use the shared
  CLI value-suggestion rules: nearby values such as `githb`, `githbu`, and
  `gitlab` name `github`; distant values such as `gitlab-ci` and
  `zzzzzzzzzzzz` list the accepted provider without a suggestion. A hint
  never accepts or executes the rejected spelling: unknown providers exit 2,
  write the error and accepted list to stderr, leave stdout empty, and create
  no files. A missing provider retains the missing-value refusal. Valid
  `github` parsing, preview, and generated file contents remain unchanged.
- With a lossless root, successful `ripr init --ci github` names the generated
  workflow and tells the adopter to review and commit it, then open or update
  a pull request and read the advisory job summary. Blocking gates remain opt-in through the
  `RIPR_GATE_MODE` repository variable. Config-only and dry-run output must
  not imply that a workflow was adopted.
- Successful `ripr init` follow-up commands for `doctor` and `check` bind
  the selected root, including a relative or implicit root, so copying them
  from another directory still inspects the repository that was configured.
  If neither the supplied absolute spelling nor the resolved filesystem path
  is lossless, completed writes and success
  status remain truthful while executable follow-ups are omitted with an
  actionable UTF-8-parent/alias remedy.
- Existing-config and existing-workflow `ripr init` refusals bind every
  executable retry (overwrite, dry-run preview, and config-preserving CI
  refresh) to the selected root. Pasting from another directory must not
  initialize or overwrite that directory. Real and dry runs retain the same
  refusal. If no lossless root spelling exists, omit executable retries and
  name the UTF-8-parent/alias remedy while leaving existing files intact.
- Workflow refresh advisories from `init` and `doctor` also bind executable
  refresh commands to the selected root, including after an upgrade and during
  an init dry run. Preserve config customizations and provide PowerShell quoting
  when needed. Commands remain byte-preserving after terminal sanitization,
  including roots with literal line breaks or tabs. If no lossless command root exists, retain the advisory but omit
  the refresh command with the same UTF-8-parent/alias remedy.
- A missing `ripr.toml` and a freshly generated `ripr.toml` should produce
  equivalent default policy behavior, except for filesystem artifacts
  intentionally created by `init` or other commands.
- Explicit CLI flags and LSP initialization options override repo config.
- Repo config overrides built-in defaults.
- Config discovery stops at a `.git` directory entry, including a dangling or
  cyclic symlink. The nearest config within that boundary still applies and
  explicit CLI settings still override it; enclosing repository policy cannot
  cross the boundary. This entry does not establish valid Git provenance.
- Implicit check and cache root selection also stop at dangling or cyclic
  `.git` entries before adopting an enclosing Cargo workspace. They retain
  the nearest local project rather than applying that workspace's policy.
- Malformed or unknown config fails closed with an actionable path and message.
- Static outputs must use static evidence vocabulary and must not claim runtime
  mutation confirmation.
- Runtime mutation vocabulary belongs only in explicit calibration/runtime
  reports supplied with runtime data.
- CI and SARIF behavior is advisory unless the user explicitly enables a
  baseline failure policy.
- Unsaved-buffer editor overlays are not enabled by default.
- Solved, intentional, and suppressed seams are hidden from default operator
  attention.
- Actionable weak, missing, reachable, and unknown seam classes remain visible
  with conservative severities.

## Surface Defaults

| Surface | Default should do | Default should not do |
| --- | --- | --- |
| CLI | Produce readable static evidence and next-step guidance from ordinary commands. | Require format knowledge or repo config before first value. |
| `ripr doctor` | Show config state, server/tooling availability, and whether defaults or repo policy are active. | Print config source text or silently ignore malformed policy. |
| `ripr init` | Materialize the conservative built-in defaults into `ripr.toml` when requested, and optionally generate advisory GitHub CI with report artifacts and optional SARIF rendering/upload through `--ci github`. | Unlock basic usefulness, overwrite repo policy or generated workflow files without `--force`, or create blocking CI by default. |
| `ripr pilot` | Generate a standard pilot packet and print the top actionable next step. | Run mutation testing, edit files, or require users to know internal report names. |
| `ripr outcome` | Compare two repo-exposure snapshots and explain whether evidence moved. | Require the `ripr` source repo or `cargo xtask`. |
| Operator cockpit | Join existing repo-local reports into one next-action view. | Rerun analysis implicitly, edit code, or replace the public `ripr pilot` path. |
| Calibration import | Join supplied runtime data to static seam evidence and explain agreement buckets. | Run mutation testing or change static classifications. |
| LSP / VS Code | Make bounded saved-workspace diagnostics, hovers, targeted briefs, context packets, best related test, and refresh status discoverable. | Stay inert until `ripr init`, surprise users with expensive live unsaved-buffer analysis, or run deep analysis by default. |
| SARIF / GitHub Actions | Upload advisory report artifacts and optionally upload code-scanning results from static evidence. | Fail CI without explicit baseline policy. |
| Badges | Report configured-visible unresolved seam counts. | Present counts as coverage, test completeness, or runtime mutation confirmation. |

## Default Config Profile

The generated conservative profile for `ripr init` should materialize the same
policy defaults that built-in missing-config behavior uses:

```toml
[analysis]
mode = "draft"
include_unchanged_tests = true

[oracles]
snapshot_strength = "medium"
mock_expectation_strength = "medium"
broad_error_strength = "weak"

[severity.seams]
strongly_gripped = "off"
weakly_gripped = "warning"
ungripped = "warning"
reachable_unrevealed = "warning"
activation_unknown = "info"
propagation_unknown = "info"
observation_unknown = "info"
discrimination_unknown = "info"
opaque = "info"
intentional = "off"
suppressed = "off"

[lsp]
seam_diagnostics = true

[reports]
max_related_tests = 5

[suppressions]
path = ".ripr/suppressions.toml"
```

Built-in defaults should provide the same conservative operator experience as
the generated init profile. Surfaces may bound cost with saved-workspace,
draft-mode behavior, diagnostic caps, lazy refresh, and clear status messaging,
but they should not be inert by default. Repository config can still disable
surfaces explicitly when a team wants a quieter policy.

## Default Scope And Mode Vocabulary

Defaults-first surfaces use operator vocabulary, but every user-facing word maps
to a concrete analysis mode:

| Operator stance | Concrete mode | Current scope |
| --- | --- | --- |
| Fastest feedback | `instant` | Changed Rust files only. |
| Normal/default | `draft` | Rust files in packages touched by the diff, including unchanged tests. |
| PR fast scan | `fast` | Same package-local scope as `draft` for now. |
| Deep static scan | `deep` | All Rust files in the workspace. |
| Ready preflight | `ready` | All Rust files in the workspace before separate mutation confirmation. |

Repo-scoped reports and badges must represent the published Rust package
surface. They exclude repository automation and non-production trees such as
`xtask/`, top-level fixtures, editor-extension sources, `target/`,
`node_modules/`, tests, examples, benches, and `src/tests.rs`. Passing a
fixture workspace as `--root` remains valid and analyzes that fixture normally.

## Implicit Check And Cache Roots

Without `--root`, `ripr check` first searches for a Cargo workspace, then the
nearest Cargo package or JavaScript/Python workspace declaration, then a
verified Git work-tree root. `ripr cache` shares this resolver. Explicit
`--root` remains authoritative.

Structural-marker hardening (#8126) narrows the former field-presence rule:
`package.json` marks a workspace only when its top-level `workspaces` value is
an array or object, including Yarn object forms. String, number, Boolean, and
`null` values do not select that directory. This shared rule also governs
TypeScript package-discovery evidence (RIPR-SPEC-0085). It does not validate
members, globs, or package-manager schemas or execute a package manager.

A `.git` entry alone is not proof of a Git root. An inert ancestor marker must
not move a standalone project's analysis away from its current directory or
lose marker-based Python detection (#5111). Valid repositories, linked-worktree
Git files, and nested repository boundaries remain supported. If Git establishes
that a candidate is inside an enclosing work tree, the walk may continue to the
actual root. A refused or unverified marker remains a traversal barrier without
being selected as a root; a Cargo package or workspace declaration at or below
that barrier can still choose the root. Otherwise the current directory stays
selected, including when analyzing a saved diff without Git installed.

Root verification has a bounded deadline and captured output and ignores
inherited Git repository selectors. Invocation failures other than missing Git
must surface rather than widen the walk: `check` names the explicit `--root`
recovery and `cache` names `RIPR_CACHE_DIR`. These rules change root selection
only, not analysis classification or exit policy.

## Pilot Packet

The first public pilot path should converge on these user-facing files:

```text
target/ripr/pilot/repo-exposure.json
target/ripr/pilot/repo-exposure.md
target/ripr/pilot/agent-seam-packets.json
target/ripr/pilot/pilot-summary.json
target/ripr/pilot/pilot-summary.md
```

The pilot terminal summary should answer:

```text
What is the top actionable seam or preview-language repair card?
Why did RIPR flag it?
What focused test should I write next?
Which file contains the structured packet?
How do I compare before and after after adding the test?
```

Pilot ranks Rust repo seams only. When that scan produces no Rust seams and
the workspace contains TypeScript, JavaScript, Python or Perl files, pilot must
not present the empty ranking as a clean result: the terminal and Markdown name
each such language, its diff-first `ripr check --root <root>` route (or, when
this binary cannot analyze the language, the unavailable-adapter notice), and
close with that route instead of the Rust before/after snapshot commands. When
an enabled language has that check route and no Rust seam or Python repair
card ranks, the Top recommendation names the route (`analyze the Python
diff: ripr check --root <root>`) instead of `none`; the "pilot ranks Rust
seams and found none" explanation stays secondary. Disabled preview languages
still list the route in the languages section. JSON `language_routes` and
`current_change.top_recommendation_in_change` are unchanged (#3906, #7665).
When Rust seams exist, the human output is unchanged and other languages are
listed only in `pilot-summary.json` `language_routes`.

Pilot's ranking is repo-wide, but a developer who just ran `ripr check` on a
branch reads its top recommendation as the next step for that change. Pilot
therefore loads the current change (the default base, resolved as `ripr check`
resolves it, against the live working tree when it has uncommitted tracked
changes, otherwise `<base>...HEAD`), ranks actionable seams on its changed
lines first, and says in the terminal, Markdown and
`pilot-summary.json` (`current_change`) whether the top recommendation is part
of the change. For match seams, changed-line membership uses the parser-owned
inclusive match head: the `match` keyword through the scrutinee, or the arm
pattern and guard through `=>`. Separate arm-body lines do not touch that head.
Token anchors, seam IDs and calibration spans stay unchanged. The coverage
survives cached facts and classifications. When the change has no ranked seam, pilot says the
recommendation is elsewhere in the repo, says why no seam on the change ranks
(pilot withholds its seams as static limitations, they are already gripped,
intentional or suppressed, the seam limit left seams unanalyzed, the change is
in a file pilot's repo-wide ranking leaves out by design (a Cargo build script,
repository automation under `xtask/`, or a crate source declared outside
`src`, which only diff analysis covers, also named after a reason drawn from
the change's analyzed seams; #6944), or no seam
pilot analyzed is on a changed line), and names `ripr check` for the change
itself, with `--worktree` when the change is uncommitted, since plain `ripr
check` reads committed history only. When the repo-exposure seam limit cuts
the inventory, pilot classifies the seams on the change's lines on their own,
within what is left of its deadline, and adds the ones the cut dropped, so they
still rank change-first and count as analyzed; the inventory seam-limit caveat
about the change then no longer applies. When Rust is enabled and the loaded
change names Rust source files, pilot records the shared source-admission
observation regardless of whether the inventory cap fired or supplemental
classification succeeded. It retains the producer's existing
`changed_file_absent_from_worktree` limitations for absent requested files.
The fallback and successful supplement apply the shared diff-aware
generated-source policy before admission, including handwritten opt-ins,
stronger explicit patterns and changed vendor checksum markers, so source known
to be excluded gains no restoration warning under either inventory cap. The
supplement retains all changed paths as policy context, including missing
markers, while its classification scope stays Rust-only. Its absence observation
precedes classification and is carried on success. Header-only exclusions still depend on available
worktree bytes or an active committed-source overlay; pilot creates no new
overlay to determine the header of a sparse missing file.
The terminal and Markdown name each path and its recovery action under
"Inspected", even when a present changed seam is the top recommendation.
`current_change.absent_changed_files` in the JSON summary records those portable
paths (or a null entry for a bounded path-less disclosure when a name is
refused). The field is omitted when no source-admission observation was
requested, including Rust-disabled scope; an empty array means that observation
found no absent requested files. Intentionally deleted files are excluded by
the shared diff parser. Neither an empty classification nor an empty absence
array establishes that every changed seam was classified. Restoring a regular
source file at the same HEAD replaces the absence list on the next run,
without stale cache disclosures.
The same loaded change selects the initial inventory and supplement's complete
source/evidence corpus before indexing and seam-limit accounting. Restoring a
vendored Rust file while its changed checksum marker remains sparse does not
re-admit that file or its evidence. Ordinary repo exposure retains its disk-based
corpus and selected-path/byte cache identity. Contextual skip guidance and whether
the selected corpus differs from ordinary repo exposure are recomputed on every
run, including cache hits. Pilot omits comparable artifact identity when these
source/evidence input sets differ, even if the contextual cached bytes stayed
identical across marker disappearance or restoration.
If that classification fails or runs out of time, pilot says so on stderr,
keeps the inventory caveat and still completes. When
pilot added any such seam, its `repo-exposure.json` is written without the
comparable `artifact` identity, because `ripr check` never classifies those
seams (#6943). The terminal and Markdown "Inspected" block names the scope:
change-first with a change, otherwise the whole repository, with a short reason
when the change could not be loaded. With no change, or when the diff cannot be
loaded, the ranking is unchanged, the human output differs only by that scope
line, and a failed load never fails pilot (#1169).

When a seam limit (the repo-exposure inventory limit or the pilot seam budget)
cut the classified seams before ranking, `pilot-summary.md` must say so under
"What Was Inspected": it names how many seams were ranked out of the outermost
total (when both limits cut, the budget's analyzed count over the inventory's
total), reads the actionable count as
"at least N", and reads each "Also in this function" count as "at least N",
because seams past the cut were never counted (#6602). Without a limit, the
wording is unchanged.

The pilot command must remain advisory. It should not edit source files,
generate tests, run mutation testing, or enable CI blocking policy.
It should also be bounded for interactive first runs: if analysis exceeds the
pilot budget, the command should write an explicit partial summary and a retry
command instead of waiting silently or presenting incomplete analysis as
complete. A timeout must not leave `repo-exposure.json`, `repo-exposure.md`, or
`agent-seam-packets.json` from an earlier complete run in the same packet
directory as if they belonged to this run. Those unwritten siblings are removed
before the partial summaries are written, rather than marked stale in place: a
missing file cannot be read as this run's evidence, and the summary schema stays
unchanged (`outputs` remains the packet path map; `outputs_written` names what
this run produced) (#7425).

The timeout JSON summary's `next.retry_command` must explicitly request
`--format json`, so an agent following it continues receiving structured stdout
on recovery (#8472). The shared root, output directory, mode, seam count and
retry-budget policy remain bound to the original request. Human Markdown and
terminal retry commands retain their default human format. This does not
promote the partial analysis: totals remain `null`, ranked seams and completed
phases remain empty, and only the two summaries are declared written.

## Outcome Receipt

The public before/after receipt command should expose the existing targeted-test
outcome behavior through the installed `ripr` binary. The user should not need
this repository checked out.

Minimum command shape:

```bash
ripr outcome --before before.repo-exposure.json --after after.repo-exposure.json
```

The receipt should report:

- before and after grip-class counts;
- moved seams;
- unchanged seams with evidence deltas;
- regressed seams;
- new seams;
- removed seams;
- input paths and advisory status.

The command should default to a human-readable Markdown or text surface, with a
JSON option for tools.

## Operator Cockpit

The repo-local operator cockpit should join existing report artifacts under
`target/ripr/reports/` into one "what do I do next?" surface:

```bash
cargo xtask operator-cockpit
```

The command writes:

```text
target/ripr/reports/operator-cockpit.json
target/ripr/reports/operator-cockpit.md
```

The cockpit should answer:

- top weak seams;
- why each listed seam matters;
- the suggested next targeted test;
- whether LSP, SARIF, badge, targeted-test receipt, and calibration surfaces
  are present and aligned with the operator loop;
- the next command to run.

It should not rerun analysis implicitly. Missing inputs should stay visible with
their generator command so operators can decide which report to refresh.
The cockpit should read repo exposure, LSP cockpit, SARIF policy, badge status,
targeted-test outcome, and optional mutation calibration artifacts. Required
inputs should be marked with a `required` boolean, while optional calibration
should remain visible without making the cockpit fail. When an input artifact is
present but its JSON has no top-level `status` field, the cockpit should report
that artifact status as `present` instead of treating the missing status as an
error. Markdown output should include a surface-alignment table with `Surface`,
`State`, `Status`, `Agreement`, and `Signal` columns.

## Calibration Import

Calibration import should expose the existing advisory static/runtime join
through the installed `ripr` binary:

```bash
ripr calibrate cargo-mutants \
  --mutants-json target/mutants/outcomes.json \
  --repo-exposure-json target/ripr/pilot/after.repo-exposure.json
```

The command should:

- accept already-produced runtime mutation data;
- join by `seam_id` first and unambiguous normalized file/line second;
- preserve ambiguous file/line matches without assigning them to the first
  static seam;
- summarize agreement buckets;
- keep static and runtime fields separate;
- keep runtime vocabulary out of static reports.

It must not run mutation testing, change static classifications, or become CI
blocking by default.

## Required Evidence

Defaults-first adoption evidence should cover:

- documentation for every default surface listed above;
- implicit-root saved-diff analysis beneath invalid Git markers matching the
  explicit-root and genuine-repository positives with nonzero changed files,
  probes, findings and related tests;
- implicit-root parity for the task-first commands (`repair`, `pilot`, `status`,
  `continue`, `agent stub`) from a workspace member directory, including
  member-relative `file:line` selectors that must not reach a sibling member;
- the same implicit-root parity for `agent start/repair/card/status/verify`,
  `rerun`, `diff` and `explain` from a workspace member directory, with
  member-relative `--before/--after`, `--changed-test` and `file:line` inputs
  bound to the invocation directory and `--root .` as the opt-out;
- shared cache-root parity, real nested and linked-worktree boundaries,
  inherited-selector isolation, and the gitless saved-diff route;
- built-in missing-config behavior matching the generated init profile's
  default policy behavior;
- fast/normal/deep operator mode vocabulary pinned to concrete analysis scopes;
- repo-scoped report and badge inputs excluding repository automation and
  non-production trees while fixture roots still work;
- default badge and report behavior staying advisory and configured-visible;
- `ripr init` generated config preserving conservative defaults;
- default repo discovery excluding generated, policy-only, fixture-only, and
  package-manager directories before mode-specific scope is applied;
- `ripr init --dry-run` printing without writing;
- `ripr init --force` being required to overwrite existing `ripr.toml`;
- `ripr init --ci github` generating a non-blocking advisory GitHub workflow
  that uploads pilot/report artifacts, documents repo badge artifacts, and keeps
  SARIF rendering/upload behind an explicit workflow setting;
- `ripr pilot` generating the pilot packet and top actionable next step;
- `ripr outcome` producing the same movement buckets as the xtask receipt;
- `cargo xtask operator-cockpit` joining repo exposure, LSP cockpit,
  SARIF policy, badge status, targeted-test outcome, and optional mutation
  calibration into one next-action report;
- calibration import producing the same agreement buckets as the xtask report;
- a public example corpus demonstrating boundary gaps, weak oracles, missing
  equality-boundary discriminators, exact error variants, opaque
  fixture/builder paths, targeted-test receipts, LSP action expectations, and
  optional calibration artifacts;
- installation verification covering public `cargo install`, local package
  install, GitHub Release server assets, VSIX packaging, and known limits;
- missing config remaining healthy and visible in `ripr doctor`;
- malformed config failing closed;
- generated GitHub Actions workflow being advisory by default;
- LSP saved-workspace behavior and no unsaved-buffer overlays by default;
- static output language checks preserving conservative vocabulary.

## Non-Goals

This spec does not require:

- analyzer behavior changes;
- new seam or finding classes;
- LSP protocol changes;
- SARIF renderer changes;
- badge semantic changes;
- CI blocking by default;
- hosted services;
- runtime mutation execution;
- unsaved-buffer overlays;
- automatic test generation;
- crate splitting.

## Acceptance Examples

### Missing config still works

```text
Given a Rust workspace with no ripr.toml,
when a user runs ripr check,
then ripr uses built-in conservative defaults and reports static evidence
without requiring configuration.
```

### Init writes conservative policy

```text
Given a Rust workspace with no ripr.toml,
when a user runs ripr init,
then ripr writes a conservative repo config that hides solved, intentional, and
suppressed seam classes while showing actionable weak and unknown classes.
```

### Init follow-ups retain the selected repository

```text
Given a Rust repository A and a different valid repository B,
when init selects A through an absolute, relative, or implicit root,
then its printed doctor and check commands still inspect A when replayed in B,
including when A's path contains spaces or an apostrophe.
A successful command that reports B's configuration is not accepted.
If the supplied spelling contains non-UTF-8 cwd bytes but the selected
filesystem target has a UTF-8 path, then usable follow-ups select that target,
including parent traversal and symlink/.. traversal.
If neither spelling is lossless, init still writes the intended config and
succeeds, but omits follow-ups that would target a replacement-character directory.
A valid UTF-8 absolute alias to the physical root still yields usable follow-ups.
```

### Init does not overwrite accidentally

```text
Given a workspace with an existing ripr.toml,
when a user runs ripr init without --force,
then ripr exits with an actionable message and leaves the file unchanged.
```

### Init provider recovery does not initialize a rejected value

```text
Given an empty selected repository,
when a user runs ripr init --ci githb,
then ripr exits 2 with a github suggestion and Accepted: github on stderr,
leaves stdout empty, and creates neither config nor workflow files.
With gitlab-ci instead, the same refusal has no suggestion.
With github, the existing preview and generated files retain their contract.
```

### Pilot points at one next action

```text
Given a workspace with repo seam evidence,
when a user runs ripr pilot,
then ripr writes a pilot packet and prints the top actionable seam, why it was
flagged, and how to compare before and after after a focused test is added.
```

### Pilot timeout does not present a previous run's artifacts

```text
Given a packet directory that still holds repo-exposure and agent-seam-packet
files from a previous complete run,
when ripr pilot times out,
then those leftover files are gone and the partial summaries do not present
them as this run's outputs.
```

### Pilot names languages its Rust scan did not rank

```text
Given a workspace whose code is TypeScript, JavaScript, Python or Perl and
whose Rust seam scan produces no seams,
when a user runs ripr pilot,
then pilot names each detected language and its diff-first ripr check route,
or says the language is not available from this ripr binary, and does not
report the empty ranking as a clean result.

Given a workspace whose code is an enabled supported language (Python on a
detected Python project, or TypeScript/JavaScript listed in `[languages]
enabled`) with a runnable ripr check route, no Rust seams, and no Python
repair card,
when a user runs ripr pilot,
then the Top recommendation names that language's check route rather than
"none", and still says that pilot ranks Rust seams and found none.

Given a workspace with TypeScript or JavaScript files, no Rust seams, and
that language not listed in `[languages] enabled`,
when a user runs ripr pilot,
then the Top recommendation stays "none" while the languages section still
names the check route.

Given a workspace with Rust seams and other-language files,
when a user runs ripr pilot,
then the terminal and Markdown output are unchanged and pilot-summary.json
lists the other languages under language_routes with state supplementary.
```

### Pilot discloses absent changed-file coverage and recovery

```text
Given a committed changed Rust file omitted by sparse checkout and an inventory
limit that triggers the current-change supplement,
when a user runs ripr pilot,
then the JSON summary names the absent file and terminal and Markdown name
changed_file_absent_from_worktree, the path, and how to restore coverage.
The disclosure remains when another present changed seam ranks first.

Given that same HEAD and committed diff after the regular source file is restored,
when a user runs ripr pilot with either a warm or isolated cold cache,
then the restored changed predicate ranks, absent_changed_files is empty, and
the missing-file disclosure clears. The supplement does not alter ordinary
repo-exposure results stored in the diff-independent classified cache.

Given sparse changed handwritten, convention-generated, explicitly excluded and
vendored files, with the changed vendor checksum marker also absent,
when pilot runs with an uncapped inventory or a cap that triggers a successful
supplement,
then all three summaries name the same analyzable absent paths. Exact handwritten
opt-ins admit convention-only paths; explicit exclusion and the changed vendor
marker still win. A missing file whose generated header is unavailable remains
named. Same-HEAD restoration clears absence and retains a nonempty changed
ranking without admitting excluded source or polluting the ordinary inventory.

Given the vendor source is restored while only its changed checksum remains sparse,
when pilot runs warm or cold under either cap,
then the vendor source stays outside ranking, absent_changed_files remains empty,
and no missing-source retry guidance appears. A changed handwritten predicate
still ranks. Ordinary inventory remains unchanged by pilot. For uncapped pilot,
comparable artifact identity is omitted only while the contextual source/evidence
set differs from ordinary inventory, even when the classified cache hits across
the sparse-marker transition.
```

### Pilot says whether its top recommendation is part of the current change

```text
Given a branch whose committed or uncommitted change touches a ranked seam,
when a user runs ripr pilot,
then that seam ranks ahead of better-classed seams elsewhere and pilot says the
top recommendation is part of the current change.

Given an edit to a later line of a multiline match scrutinee, pattern or guard,
when a user runs ripr pilot with cold or warm caches and either seam cap,
then the corresponding match seam counts as changed, retains its original
seam ID and ranks as part of the change. An edit confined to a separate arm
body does not mark that match head as changed.

Given a branch whose change touches no ranked seam,
when a user runs ripr pilot,
then pilot says the top recommendation is elsewhere in the repo, not part of
the current change, and names ripr check for the change itself.

Given a branch whose changed lines hold only seams pilot withholds as static
limitations (opaque or an unknown class),
when a user runs ripr pilot,
then pilot says it withholds the seams on the change and why, rather than that
no seam pilot analyzed is on a changed line, and `withheld_seams_in_change` counts them.

Given a branch whose only Rust change is in a Cargo build script,
when a user runs ripr pilot,
then pilot names the build script and says its repo-wide ranking leaves it
out, rather than that no seam pilot analyzed is on a changed line.

Given a repository past the inventory seam limit whose changed lines hold
seams the limit cut,
when a user runs ripr pilot,
then pilot classifies the change's files on their own, ranks those seams
change-first and counts them as analyzed.

Given a change that exists only as uncommitted edits in the working tree,
when a user runs ripr pilot and the top recommendation is not part of it,
then the ripr check command pilot names carries --worktree.

Given no current change, or a root where the diff cannot be loaded,
when a user runs ripr pilot,
then the ranking is unchanged, the terminal and Markdown name the whole
repository as pilot's scope, and pilot-summary.json records current_change
state no_change or unavailable.
```

### Outcome is public CLI

```text
Given before and after repo-exposure snapshots,
when a user runs ripr outcome --before before.json --after after.json,
then ripr reports moved, unchanged, regressed, new, and removed seams without
requiring cargo xtask.

When the snapshots include `seams[].evidence_record`, outcome movement prefers
that shared evidence spine for stage, observed-value, missing-discriminator,
oracle-strength, and related-test deltas. Older snapshots without the record
remain supported through legacy repo-exposure fields.
```

Outcome receipts must also include a reviewer-native packet derived from the
same before/after comparison. The packet answers:

- what static evidence changed between the supplied snapshots;
- what RIPR flagged before the focused repair attempt;
- which focused proof signals became visible outside RIPR;
- what moved after verification and what remained weak or unknown;
- what reviewers should inspect; and
- what reviewers should not infer, including runtime mutation results,
  coverage adequacy, general correctness, merge approval, source edits, or
  generated tests.

This packet is a projection of the compared static artifacts. It must not
create new analyzer facts, run analysis, run mutation testing, edit source,
generate tests, call providers, change policy, or become a CI gate.

### Public install exposes the loop

```text
Given a released defaults-first version,
when a user runs cargo install ripr,
then the installed binary exposes ripr pilot, ripr outcome, and optional
ripr calibrate cargo-mutants without requiring the ripr source checkout.
```

### Operator cockpit points at the next command

```text
Given repo-local report artifacts under target/ripr/reports,
when a user runs cargo xtask operator-cockpit,
then ripr writes operator-cockpit.md and operator-cockpit.json with top weak
seams, surface alignment, missing-input commands, and the next targeted-test
loop command.
```

### CI remains advisory

```text
Given a generated GitHub Actions workflow,
when the workflow runs on a pull request,
then it uploads review artifacts, keeps SARIF rendering/upload optional, and
does not fail the pull request unless the repository explicitly opts into a
baseline failure policy.
```

## Test Mapping

Current tests and reports that support the contract:

- `crates/ripr/tests/pilot_missing_changed_files.rs::pilot_discloses_sparse_missing_changed_file_and_recovers_same_head`
- `crates/ripr/tests/pilot_missing_changed_files.rs::pilot_discloses_mixed_missing_changed_file_even_with_changed_recommendation`
- `crates/ripr/tests/pilot_missing_changed_files.rs::pilot_capped_absence_matches_uncapped_generated_policy_and_recovers`
- `crates/ripr/tests/pilot_missing_changed_files.rs::pilot_partial_vendor_restoration_preserves_diff_policy_and_cache_identity`
- `crates/ripr/src/analysis/generated_rust_corpus.rs::tests::changed_vendor_context_excludes_evidence_and_refreshes_skip_strength`
- `crates/ripr/src/output/pilot/tests.rs::pilot_current_change_preserves_absent_file_coverage_through_empty_and_mixed_supplements`
- `crates/ripr/src/output/pilot/tests.rs::pilot_renderers_disclose_absent_changed_files_independently_of_top_recommendation`

- `crates/ripr/src/config.rs::tests::generated_init_config_is_conservative_and_parseable`
- `crates/ripr/src/config.rs::tests::generated_init_config_matches_checked_in_example`
- `crates/ripr/src/analysis/workspace/discover.rs::tests::discover_skips_default_excluded_directories`
- `crates/ripr/src/cli/commands.rs::tests::init_parses_root_dry_run_and_force`
- `crates/ripr/src/cli/commands.rs::tests::init_requires_root_value`
- `crates/ripr/src/cli/commands.rs::tests::init_rejects_unknown_arguments`
- `crates/ripr/tests/cli_help_hierarchy.rs::init_ci_provider_errors_recover_near_misses`
- `crates/ripr/tests/cli_help_hierarchy.rs::init_ci_provider_valid_preview_keeps_workflow_contract`
- `crates/ripr/src/cli/commands.rs::tests::init_generated_github_workflow_is_advisory`
- `crates/ripr/src/cli/commands.rs::tests::init_generated_github_workflow_uploads_reports_and_makes_sarif_optional`
- `crates/ripr/src/cli/commands.rs::tests::init_ci_github_writes_workflow_and_preserves_existing_config`
- `crates/ripr/src/cli/commands.rs::tests::init_ci_github_refuses_existing_workflow_without_force`
- `crates/ripr/tests/cli_smoke.rs::init_writes_conservative_config_and_doctor_loads_it`
- `crates/ripr/tests/cli_smoke/init_ci_guidance.rs::init_ci_followup_explains_adoption_for_created_and_refreshed_workflows`
- `crates/ripr/tests/cli_smoke/init_ci_guidance.rs::init_ci_followup_does_not_claim_adoption_for_config_only_or_preview`
- `crates/ripr/tests/cli_smoke/init_followups.rs::init_followups_replay_the_selected_root_from_foreign_cwd`
- `crates/ripr/tests/cli_smoke/init_followups.rs::init_refusal_retries_replay_the_selected_root`
- `crates/ripr/tests/cli_smoke/init_followups.rs::init_upgrade_warning_refresh_replays_selected_root`
- `crates/ripr/tests/cli_smoke/init_followups.rs::doctor_workflow_warning_refresh_replays_selected_root`
- `crates/ripr/tests/cli_smoke/init_followups.rs::workflow_warning_omits_refresh_for_unrepresentable_root`
- `crates/ripr/tests/cli_smoke/init_followups.rs::workflow_refresh_control_roots_replay_without_touching_escaped_siblings`
- `crates/ripr/tests/cli_smoke/init_followups.rs::init_refusal_omits_retries_for_an_unrepresentable_root`
- `crates/ripr/tests/cli_smoke/init_followups.rs::init_omits_followups_for_an_unrepresentable_implicit_root`
- `crates/ripr/tests/cli_smoke/init_followups.rs::init_followups_replay_a_utf8_alias_to_a_non_utf8_root`
- `crates/ripr/tests/cli_smoke/init_followups.rs::init_followups_replay_utf8_targets_from_a_non_utf8_cwd`
- `crates/ripr/tests/cli_smoke.rs::init_dry_run_prints_config_without_writing`
- `crates/ripr/tests/cli_smoke.rs::init_ci_github_dry_run_prints_config_and_workflow_without_writing`
- `crates/ripr/tests/cli_smoke.rs::init_ci_github_writes_non_blocking_report_workflow`
- `crates/ripr/tests/cli_smoke.rs::init_ci_github_refuses_existing_workflow_without_force`
- `crates/ripr/tests/cli_smoke.rs::init_refuses_existing_config_without_force`
- `crates/ripr/tests/cli_smoke.rs::init_force_overwrites_existing_config`
- `crates/ripr/src/config.rs::tests::generated_init_config_matches_builtin_defaults`
- `crates/ripr/src/config.rs::tests::missing_config_uses_behavior_preserving_defaults`
- `crates/ripr/src/config.rs::tests::malformed_or_unknown_config_is_actionable`
- `crates/ripr/tests/cli_smoke.rs::doctor_reports_missing_config_defaults`
- `crates/ripr/tests/cli_smoke.rs::doctor_reports_loaded_config_path`
- `crates/ripr/src/analysis/workspace/select.rs::tests::operator_mode_tiers_are_pinned_for_defaults_first_adoption`
- `crates/ripr/src/analysis/workspace/classify.rs::tests::production_path_excludes_repository_automation_fixture_and_non_production_trees`
- `crates/ripr/src/analysis/workspace/select.rs::tests::repo_discovery_skips_fixture_tree_but_fixture_roots_still_work`
- `crates/ripr/src/output/badge/tests.rs::seam_badge_summary_counts_visible_headline_eligible_seams`
- `crates/ripr/src/output/badge/tests.rs::seam_badge_summary_respects_configured_off_severity`
- `crates/ripr/tests/cli_smoke.rs::check_repo_badge_json_emits_repo_scope_metadata`
- `crates/ripr/tests/cli_smoke.rs::check_repo_badge_does_not_consult_diff_arg_when_supplied`
- `crates/ripr/tests/cli_smoke.rs::check_repo_badge_plus_json_emits_repo_scope_metadata`
- `crates/ripr/src/output/pilot/tests.rs::pilot_ranking_prefers_actionable_class_order_before_tie_breakers`
- `crates/ripr/src/output/pilot/tests.rs::pilot_ranking_uses_evidence_tie_breakers_then_stable_location`
- `crates/ripr/src/output/pilot/tests.rs::pilot_ranking_admits_gap_classes_only`
- `crates/ripr/src/output/pilot/tests.rs::pilot_summary_ranks_a_true_gap_ahead_of_withheld_limitations`
- `crates/ripr/src/output/pilot/tests.rs::pilot_summary_with_only_limitations_is_not_a_clean_result`
- `crates/ripr/tests/cli_smoke.rs::pilot_withholds_static_limitations_and_keeps_a_true_gap`
- `crates/ripr/src/output/pilot/tests.rs::pilot_ranking_excludes_solved_governed_classes`
- `crates/ripr/src/output/pilot/tests.rs::pilot_ranking_puts_seams_in_the_current_change_first`
- `crates/ripr/src/output/pilot/tests.rs::multiline_match_heads_follow_changed_lines_without_moving_seam_identity`
- `crates/ripr/src/output/pilot/tests.rs::multiline_match_guard_lines_exclude_the_arm_body`
- `crates/ripr/src/output/pilot/tests.rs::pilot_match_source_lines_preserve_identity_and_refuse_invalid_ranges`
- `crates/ripr/tests/cli_smoke.rs::pilot_ranks_multiline_match_head_edits_with_warm_caches_and_caps`
- `crates/ripr/src/output/pilot/tests.rs::pilot_current_change_matches_the_seam_span_and_new_side_lines`
- `crates/ripr/src/output/pilot/tests.rs::pilot_renderers_say_whether_the_top_recommendation_is_in_the_current_change`
- `crates/ripr/tests/cli_smoke.rs::pilot_ranks_and_labels_seams_in_the_current_change`
- `crates/ripr/src/output/pilot/tests.rs::pilot_ranking_takes_one_seam_per_owner_before_a_second`
- `crates/ripr/src/output/pilot/tests.rs::pilot_ranking_spreads_owners_without_crossing_class_order`
- `crates/ripr/src/output/pilot/tests.rs::pilot_summary_md_counts_an_owners_unlisted_seams_once`
- `crates/ripr/src/output/pilot/tests.rs::pilot_summary_md_marks_owner_counts_as_lower_bounds_after_a_seam_limit`
- `crates/ripr/src/output/pilot/tests.rs::pilot_ranking_counts_owner_rounds_across_classes`
- `crates/ripr/src/output/pilot/tests.rs::pilot_summary_md_names_unlisted_seams_on_an_owners_first_pick_only`
- `crates/ripr/src/output/pilot/tests.rs::pilot_summary_json_contains_config_state_artifacts_and_next_commands`
- `crates/ripr/src/output/pilot/tests.rs::pilot_summary_md_spells_out_first_screen_recommendation`
- `crates/ripr/src/output/pilot/tests.rs::pilot_terminal_prints_top_test_and_follow_up_commands`
- `crates/ripr/src/output/pilot/tests.rs::pilot_summary_json_projects_python_first_use_repair_card`
- `crates/ripr/src/output/pilot/tests.rs::pilot_markdown_and_terminal_use_python_repair_card_when_no_seam_ranked`
- `crates/ripr/src/output/pilot/tests.rs::pilot_language_routes_state_follows_rust_seams_and_discovered_languages`
- `crates/ripr/src/output/pilot/tests.rs::pilot_renderers_show_language_routes_only_without_rust_seams`
- `crates/ripr/src/output/pilot/tests.rs::pilot_headline_names_supported_language_check_route_when_no_rust_seams`
- `crates/ripr/tests/cli_smoke.rs::pilot_writes_default_packet_outputs_for_boundary_gap_fixture`
- `crates/ripr/src/cli/commands/pilot.rs::tests::timed_out_pilot_removes_prior_run_packet_siblings`
- `crates/ripr/src/cli/commands/pilot.rs::tests::remove_unwritten_timeout_packet_siblings_is_a_no_op_when_absent`
- `crates/ripr/src/cli/commands/pilot.rs::tests::remove_unwritten_timeout_packet_siblings_deletes_prior_run_files`
- `crates/ripr/src/cli/commands/pilot.rs::tests::remove_unwritten_timeout_packet_siblings_unlinks_a_symlink_and_keeps_its_target`
- `crates/ripr/src/cli/commands/pilot.rs::tests::remove_unwritten_timeout_packet_siblings_refuses_a_directory`
- `crates/ripr/src/output/pilot/tests.rs::timeout_summary_json_is_partial_and_points_to_retry`
- `crates/ripr/src/output/pilot/tests/timeout_retry.rs::timeout_json_retry_preserves_format_and_unknown_analysis`
- `crates/ripr/tests/cli_smoke/pilot_timeout_retry.rs::timeout_json_retry_replays_as_json_from_a_foreign_directory`
- `crates/ripr/src/output/pilot/tests.rs::timeout_summary_md_explains_partial_status_and_retry_command`
- `crates/ripr/tests/cli_smoke.rs::pilot_accepts_python_project_without_ripr_config`
- `crates/ripr/tests/cli_smoke.rs::pilot_projects_python_repair_card_for_git_diff`
- `crates/ripr/tests/cli_smoke.rs::pilot_uses_repo_config_mode_without_explicit_flag`
- `crates/ripr/tests/cli_smoke.rs::pilot_honors_explicit_mode_over_repo_config`
- `crates/ripr/tests/cli_smoke.rs::pilot_names_typescript_diff_first_route_when_repo_has_no_rust_seams`
- `crates/ripr/tests/cli_smoke.rs::pilot_names_python_check_route_when_repo_has_no_rust_seams`
- `crates/ripr/tests/cli_smoke.rs::pilot_says_perl_is_unavailable_when_repo_has_no_rust_seams`
- `crates/ripr/tests/cli_smoke.rs::pilot_keeps_rust_output_byte_identical_when_rust_seams_exist`
- `crates/ripr/src/output/outcome/mod.rs::tests::targeted_test_outcome_report_buckets_seam_movement`
- `crates/ripr/src/output/outcome/mod.rs::tests::targeted_test_outcome_json_and_markdown_are_structured`
- `crates/ripr/src/output/outcome/mod.rs::tests::targeted_test_outcome_from_repo_exposure_json_parses_static_evidence`
- `crates/ripr/src/output/outcome/mod.rs::tests::targeted_test_outcome_prefers_evidence_record_movement`
- `crates/ripr/src/output/outcome/mod.rs::tests::targeted_test_outcome_records_no_movement_reason`
- `crates/ripr/tests/cli_smoke.rs::outcome_prints_markdown_receipt_by_default`
- `crates/ripr/tests/cli_smoke.rs::outcome_writes_json_receipt_when_requested`
- `crates/ripr/src/output/mutation_calibration.rs::tests::mutation_calibration_summarizes_static_runtime_agreement`
- `crates/ripr/src/output/mutation_calibration.rs::tests::mutation_calibration_joins_by_seam_id_then_file_line_and_keeps_ambiguous`
- `crates/ripr/src/output/mutation_calibration.rs::tests::mutation_calibration_reports_are_advisory_and_structured`
- `crates/ripr/tests/cli_smoke.rs::calibrate_cargo_mutants_prints_markdown_by_default`
- `crates/ripr/tests/cli_smoke.rs::calibrate_cargo_mutants_writes_json_when_requested`
- `xtask/src/main.rs::tests::targeted_test_outcome_report_buckets_seam_movement`
- `xtask/src/main.rs::tests::targeted_test_outcome_json_and_markdown_are_structured`
- `xtask/src/main.rs::tests::mutation_calibration_summarizes_static_runtime_agreement`
- `xtask/src/main.rs::tests::mutation_calibration_reports_are_advisory_and_structured`
- `xtask/src/main.rs::tests::sarif_policy_missing_baseline_is_advisory_by_default`
- `xtask/src/main.rs::tests::lsp_cockpit_report_json_and_markdown_are_structured`
- `xtask/src/reports/operator.rs::tests::operator_cockpit_reports_missing_inputs_with_commands`
- `xtask/src/reports/operator.rs::tests::operator_cockpit_json_and_markdown_are_structured`
- `xtask/src/main.rs::tests::defaults_first_example_corpus_index_names_required_operator_artifacts`
- `editors/vscode/test/suite/extension.test.ts::defaults-first check mode is draft`
- `docs/INSTALLATION_VERIFICATION.md` release smoke checklist for package,
  public install, server archive, VSIX, and known limits

## Implementation Mapping

Current implementation pieces:

- `crates/ripr/src/config.rs` owns repo config defaults, validation, and
  precedence, plus the conservative generated `ripr init` config text.
- `crates/ripr/src/analysis/workspace/discover.rs` owns default repo discovery
  exclusions before mode-specific file selection.
- `crates/ripr/src/cli/commands.rs` exposes `init`, `pilot`, `outcome`, `check`,
  `explain`, `context`, `doctor`, and `lsp`; `init --ci github` writes the
  advisory GitHub Actions report workflow with optional SARIF rendering/upload.
- `crates/ripr/src/app.rs` orchestrates config-aware analysis entry points.
- `crates/ripr/src/output/agent_seam_packets.rs` renders targeted-test work
  orders.
- `crates/ripr/src/output/pilot/ranking.rs` ranks actionable seams and
  `crates/ripr/src/output/pilot/render.rs` renders the pilot summary files.
  A timeout in `crates/ripr/src/cli/commands/pilot.rs` removes leftover
  repo-exposure and seam-packet siblings before writing the partial summaries
  (#7425).
- `crates/ripr/src/output/outcome/mod.rs` compares before/after repo-exposure
  snapshots and renders the public targeted-test outcome receipt.
- `crates/ripr/src/output/mutation_calibration.rs` imports supplied
  cargo-mutants JSON, joins it to repo-exposure snapshots, and renders the
  public advisory calibration report.
- `editors/vscode/package.json` exposes `ripr.check.mode` with the same
  `draft` default as the CLI and LSP missing-config path.
- `.github/workflows/publish-extension.yml` packages the version-normalized VSIX
  and waits for matching server assets before marketplace publishing.
- `xtask/src/main.rs` currently owns repo-local mutation calibration, LSP
  cockpit, SARIF policy, badge artifact, and report-index automation.
- `xtask/src/reports/operator.rs` joins those report artifacts into
  `operator-cockpit.{json,md}` without changing analyzer behavior.
- `docs/TARGETED_TEST_WORKFLOW.md`, `docs/CI.md`, `docs/CONFIGURATION.md`,
  `docs/EDITOR_EXTENSION.md`, `docs/QUICKSTART.md`, `docs/RELEASE.md`,
  `docs/RELEASE_BINARIES.md`, and `docs/SERVER_PROVISIONING.md` document the
  current adoption and install path.
- `fixtures/EXAMPLE_CORPUS.md` indexes the defaults-first example corpus,
  including CLI goldens, LSP action expectations, targeted-test receipts, and
  optional calibration artifacts.
- `docs/INSTALLATION_VERIFICATION.md` defines the release proof path for
  `cargo install ripr`, checked package install, GitHub Release server assets,
  VSIX packaging, and known defaults-first limits.

## Later Amendment

RIPR-SPEC-0237 (2026-10-04) specifies the order that
`output/pilot/ranking.rs` produces (class order, rank key and owner
spread) and the Markdown "Also in this function" count. RIPR-SPEC-0236
owns which grip classes enter the ranked set.

## Metrics

- `time_to_first_actionable_seam`
- `pilot_packet_generated`
- `targeted_test_outcome_available`
- `calibration_import_available`
- `advisory_ci_workflow_generated`
- `operator_cockpit_available`
- `install_path_verified`
