# MCP workspace session server

`ripr mcp` serves RIPR's bounded, read-only workspace session over the
[Model Context Protocol](https://modelcontextprotocol.io/): newline-delimited
JSON-RPC on stdin and stdout. It tells an MCP client which repository root
RIPR would use, what RIPR is and is not allowed to do there, and — after an
explicit `ripr_refresh` — the committed analysis snapshot: a deterministically
bounded working set of canonical items and one complete evidence document per
item. It never edits source, runs tests or mutation, or executes verification
commands; `ripr_prepare_repair` only creates an in-memory transaction that
authorizes nothing (see [What it does not do](#what-it-does-not-do)).

The design boundary is [ADR 0022](../adr/0022-mcp-is-a-bounded-projection.md).

## Launch

```bash
ripr mcp --stdio [--root PATH]
```

`--stdio` is the default and only transport. With `--root`, RIPR uses that
exact directory. Without it, RIPR starts at the current directory and walks up
to the nearest directory containing `.git`, falling back to the nearest one
with a project file such as `Cargo.toml`, `package.json`, or `pyproject.toml`.
A directory containing only `.git` (a directory, or a gitfile as in worktrees
and submodules) is a valid repository root: `.git` itself counts as the
repository marker (#3927). Clients often start servers outside the repository,
so pass an absolute `--root` when yours does.

A generic MCP client entry:

```json
{
  "mcpServers": {
    "ripr": {
      "command": "ripr",
      "args": ["mcp", "--stdio", "--root", "/path/to/repo"]
    }
  }
}
```

### Register with a coding agent

`ripr` must be on `PATH` (or use its absolute path as `command`). Agents start
stdio servers from the project directory, so omitting `--root` resolves the
repository the agent is working in; pass an absolute `--root` if your client
starts servers elsewhere.

```bash
# Claude Code: writes a shareable .mcp.json in the project
claude mcp add --scope project ripr -- ripr mcp --stdio
# or let ripr write it (keeps your other servers): ripr init --mcp

# Codex: adds an entry to its user config
codex mcp add ripr -- ripr mcp --stdio
```

Equivalent Codex `config.toml` entry:

```toml
[mcp_servers.ripr]
command = "ripr"
args = ["mcp", "--stdio"]
```

A registered server still needs the client's own approval step before its
tools reach a session. These snippets use each client's documented MCP
registration; the repository has not yet retained cold-session proof that
either client completes the flow (tracked in #3092), so treat them as
configuration, not a support claim.

Every `ripr_refresh` result carries a top-level `fresh_for_working_tree`
(`true` for a saved-worktree snapshot or a committed one with no uncommitted
edits outside the analysis; `false` after a failed attempt or with edits the
analysis cannot see; `null` with no snapshot) and a `verify` block naming
`ripr_refresh {"source": "worktree"}` as the re-check after editing tests. ripr
runs no tests; run them yourself. A refresh whose attempt failed returns
`isError: true`; the document keeps the typed `attempt.failure` and the
`last_known_good` snapshot. `ripr_get_gap` bounds
`discriminator_availability.observed_values` to 12 entries (those at or just
after a related test's line first) and reports `observed_values_total` and
`observed_values_omitted`.

An agent edits the worktree, but by default `ripr_refresh` analyzes only the
committed default-branch diff. To analyze saved staged and unstaged tracked
edits without committing them, call `ripr_refresh` with `{"source":
"worktree"}`; omitting `source` (or `"committed"`) keeps the committed
default. A worktree snapshot is tagged `source: "worktree"` and discloses that
unsaved editor buffers are outside. The findings producer and the seam
inventory each record the raw Rust bytes they read; when they agree
(`scope.consumed_source_commitment: "established"`, `repair_cards:
"available"`) the same repair readiness, `ripr_get_repair_card` and
`ripr_prepare_repair` the committed refresh offers are available, behind their
usual per-item gates, and the producer commitment is part of the snapshot
identity. When a saved file or `HEAD` moved between the two producers, or any
input was not observed (`not_established`, `withheld`), every item is
`repair_packet_ready: false` with `worktree_source_not_committed` naming the
cause and `ripr_get_repair_card` refuses with `incomplete_inventory`: refresh
again once the edit settles. Untracked files stay excluded
until `git add -N <paths>`. The server does not watch the worktree, so
refresh again after edits. Requested and captured source are separate facts:
a failed worktree request keeps the last committed snapshot tagged
committed, and the reverse. A commit on the default branch itself leaves an
empty committed diff; `ripr check --worktree --format json` remains the
outside-MCP route.

When the repository's default branch is not `main` or `master` (for example
`trunk`) and there is no `origin/main`, the default base cannot be resolved and
`ripr_refresh` fails with `analysis_failed` naming candidate branches. Retry
`ripr_refresh` with its optional `base` argument (`{"base": "trunk"}`): it is
the only way to choose a base from inside MCP, since `ripr mcp` has no `--base`
flag. Each refresh states its own base, so omitting `base` returns to the
default branch. The additions are explicit-base-only so a base-less caller
sees the unchanged v1 shapes: `ripr_refresh` adds `requested_base` and
`ripr_workspace_status` reports `session.current_desired_input.base:
explicit` with `base_revision` only after a refresh named a base, and the
committed snapshot's `session.analysis_outcome.identity.base_revision` names
the base it analyzed even when the worktree is clean and `scope` is null. A
directory with a project marker but no usable Git work tree is a validated
root, but refresh needs Git; status then adds `workspace.root.git_work_tree`
(`not_detected`, or `marker_invalid` when the nearest `.git` is neither a
directory nor a gitfile whose first line is `gitdir: <path>`) with a
limitation instead of leaving that to the first failed refresh. The field is a
bounded marker probe, not a Git invocation, and is absent when a plausible
marker exists: absence does not mean Git accepted it.

When a committed refresh analyzed nothing (`no_scope`, `no_changed_lines` or
`no_behavioral_candidates` with zero items), `ripr_refresh` adds an
`empty_scope_hint` (`why`, `pass_base`, `pass_source`, #11213): the default base
may have resolved to the analyzed commit (detached HEAD, `origin/HEAD` at HEAD),
and `base` or `source: "worktree"` widen the diff. The hint never changes the
outcome kind and an empty result is not an all-clear.

`ripr_refresh` is read-only with respect to source, but the shared analysis
writes its warm cache under the root's `target/ripr/`. Cargo projects normally
gitignore `target/`; in a repository that does not, those files show as
untracked in `git status` and are never source.

## Progressive flow

```text
ripr_workspace_status   → root, authority, session facts (no evidence)
ripr_refresh            → one bounded static analysis, commits the snapshot
ripr_list_gaps          → deterministic bounded working set (summaries only)
ripr_get_gap            → one item's complete bounded evidence
ripr_prepare_repair     → readiness-gated in-memory repair transaction
ripr_get_repair_attempt → one session or durable attempt, read-only
ripr_get_receipt_status → one attempt's receipt state, read-only
ripr_get_repair_card    → one item's bounded repair card, read-only
ripr_list_repair_attempts → durable attempt discovery, before or after refresh
```

Status and list calls never return the full evidence graph; read one item at
a time through the tool or the resource.

## Durable attempt discovery

`ripr_list_repair_attempts` reads the default durable CLI attempt store through
its canonical inventory/validation authority; it works before `ripr_refresh`.
It never creates a missing store. MCP in-memory transactions are excluded:
their IDs come from `ripr_prepare_repair`.
The shared inventory includes manifest-bearing directories; an unpublished
reservation without `attempt.json` is excluded. A manifest-bearing directory
whose name cannot be represented as UTF-8 refuses the entire inventory with
`attempt_invalid`, preserving identity rather than returning lossy or partial rows.

The structured `ripr-mcp-repair-attempt-list-v1` document returns `inventory_id`,
root identity, observed repository HEAD, `total`, `valid`, `invalid`, `omitted`,
`items`, `page`, and continuation routes. Valid rows carry `attempt_id`, state,
seam, repository HEAD, shared live currentness and receipt status, and links to
the existing detail reads. Invalid rows carry only directory identity and a
bounded `attempt_invalid` refusal; unvalidated manifest fields are never facts.
`omitted` counts all rows outside this page, including earlier pages.

Rows sort by identity/directory name, never modification time. `offset` defaults
to zero; optional positive `limit` is capped at 256 rows and the full structured
MCP envelope byte budget. Follow `continuation.next_page.arguments`, which pins
`inventory_id` and advances by the number actually returned. The identity binds
root, observed HEAD and all summary rows. Changed observations return typed
`stale_inventory` with `current_inventory_id`; restart at offset zero without
the old identity. A missing store is an empty inventory, an unavailable root is
`workspace_unavailable`, and a store-level read refusal is `attempt_invalid`.

This is a live observation, not an atomic filesystem snapshot. Output is bounded;
canonical validation still reads the whole store. Receipt status retains the
same currentness and completeness rules as `ripr_get_receipt_status`. Discovery
runs no tests or verification and grants no execution or edit authority.

## What it exposes

| Surface | Name |
| --- | --- |
| Tool (no arguments) | `ripr_workspace_status` |
| Tool (`base?`, `source?`) | `ripr_refresh` |
| Tool (`snapshot_id?`, `offset?`, `omitted_offset?`, `limit?`, `file?`, `class?`, `language?`, `repair_ready?`) | `ripr_list_gaps` |
| Tool (`canonical_id`, `snapshot_id?`) | `ripr_get_gap` |
| Tool (`canonical_id`, `snapshot_id?`) | `ripr_prepare_repair` |
| Tool (`attempt_id`) | `ripr_get_repair_attempt` |
| Tool (`receipt_id`) | `ripr_get_receipt_status` |
| Tool (`canonical_id`, `snapshot_id?`) | `ripr_get_repair_card` |
| Tool (`inventory_id?`, `offset?`, `limit?`) | `ripr_list_repair_attempts` |
| Resource (`application/json`) | `ripr://workspace/status` |
| Resource template | `ripr://snapshot/{snapshot_id}` |
| Resource template | `ripr://gap/{canonical_id}` |
| Resource template | `ripr://repair-attempt/{attempt_id}` |
| Resource template | `ripr://receipt/{receipt_id}` |
| Resource template | `ripr://repair-card/{canonical_id}` |

`ripr_workspace_status` and `ripr://workspace/status` return the same JSON
document, schema `ripr-mcp-workspace-status-v1`. It wraps:

- the `ripr-workspace-status-v1` workspace block (resolved once at startup,
  never re-resolved): root validation state, discovery source, repository
  markers, a root `error_code` when unavailable, and a hashed host-local
  `identity` — the absolute path is never returned; the configuration
  posture (`loaded` with its `project_config_identity` when the root's
  `ripr.toml` resolves, `detected_not_loaded` when one is present but
  cannot be read or parsed, `built_in_defaults_only` when none does);
  `trust` and `authority` facts with
  source edit, verification execution, mutation execution, and model provider
  all `none`;
- a `ripr-mcp-session-v1` session block: the current desired input (workspace
  diff against the resolved default branch or the explicit `ripr_refresh`
  `base` with `base_revision` when present, draft mode, the workspace's own
  configuration), the current
  attempt state (`no_snapshot`, `in_flight`, `completed`, `failed`), the last
  completed snapshot identity, last-known-good state, freshness relative to
  the last completed refresh (`current_at_last_refresh`, or
  `stale_after_failed_attempt` once a later attempt fails),
  the committed snapshot's typed `AnalysisOutcome`, an additive
  `language_completeness` list of `{language, status}` rows from the same
  analysis run (present only once a snapshot is committed, on the status tool
  and the status resource; supported readers use the advertised `outputSchema`
  or tolerate unknown keys), and
  profile and support facts. Complete-zero and incomplete-zero outcomes stay distinct
  through the typed outcome; a partial outcome keeps its typed limitations;
- the transport, tool, resource, resource-template, and byte bounds under
  `mcp`.

`ripr_refresh` analyzes the committed default-branch diff through the shared
check authority, then commits the completed snapshot into the session.
Staged and unstaged tracked edits and untracked files are outside this scope.
The initialize/discover instructions, refresh tool description, and MCP help
disclose this before invocation. `ripr check` defaults to the working tree
when staged or unstaged tracked edits exist; sharing the authority does not
mean identical input. `ripr check --committed` selects committed input;
`ripr check --worktree --format json` analyzes staged and unstaged tracked
edits outside MCP, or inside MCP with the opt-in `source: "worktree"`
(RIPR-SPEC-0211, #7357: repair cards and readiness only with an
established producer-custody agreement, withheld with the cause
otherwise). `ripr_get_repair_card` re-verifies, on every read, the captured
digests of the files the card's evidence is bound to: an edit or commit after
`ripr_refresh` (the server does not watch the worktree) refuses with
`stale_snapshot`, names the path and says to run `ripr_refresh` again. A committed refresh whose findings and checkout Rust files were not
shown to agree (a consumed file edited, staged, deleted or changed during the
refresh, or any uncommitted or untracked Rust source or test file, including
one outside the diff) likewise withholds repair cards (`incomplete_inventory`)
and turns repair-ready items not ready (`worktree_source_not_committed`; an
item already not ready keeps its own reason) and says why, because its
findings read committed content while the cards would read the checkout;
commit the uncommitted Rust source or test files (untracked ones included) and
refresh again. A dirty non-Rust file does not withhold, and gitignored Rust files are not claimed. The
existing response `scope` note discloses detected excluded edits. The call blocks until the attempt reaches a terminal state and reports it: `completed` with the new
snapshot identity (a `snapshot:sha256:` digest over the typed outcome, the
canonical item identities, and each item's evidence digest — equivalent
roots at equivalent inputs share one portable identity, while the concrete
root evidence stays a separate
host-local hash), `failed` with a typed failure code and bounded detail (the
last-known-good snapshot is kept), `in_flight`, or `workspace_unavailable`.
An attempt runs to a terminal state; cancelling the MCP request never rolls
an attempt back or manufactures a snapshot. A cancelled attempt still commits
as a completed snapshot when it finishes and only transport teardown abandons
one before it commits, while a superseded attempt is never committed.
The workspace's own `ripr.toml` is honored (#6825): refresh resolves it
through the same `load_for_root` the CLI uses — a loaded config runs (the
session profile discloses `loaded` with its `config_identity` and enabled
languages), a config-less root runs built-in defaults including the
zero-config language auto-enable, and a present-but-unreadable or unparseable
config fails the attempt closed with `config_invalid`. Provider
configuration stays unloaded.

Gap-list responses use `ripr-mcp-gap-list-v4`, including typed refusals.
The prior v1 success schema forbids unknown top-level properties, so the new
`filtered` and `filters` fields required the v2 change; v3 adds the snapshot's
typed `outcome` block (`kind`, a plain label, and the outcome limitation
kinds), so a `partial_with_limitations` run never reads as clean; v4 adds the
independently windowed omission disclosure (`omitted_page`, `omitted_offset`). Strict v1
readers must rediscover `tools/list`, accept v4 explicitly and validate
against the advertised `outputSchema`; callers may still omit all filter
arguments.

All MCP document `schema_version` identifiers, so a client can route and
validate every document the server emits:

| Document | `schema_version` | Producer |
| --- | --- | --- |
| Workspace status | `ripr-mcp-workspace-status-v1` | `ripr_workspace_status` result |
| Session block | `ripr-mcp-session-v1` | workspace status/session documents |
| Snapshot block | `ripr-mcp-snapshot-v1` | snapshot resource |
| Refresh-time attempt document | `ripr-mcp-refresh-v1` | `ripr_refresh` tool result |
| Gap list | `ripr-mcp-gap-list-v4` | `ripr_list_gaps` result |
| Single gap | `ripr-mcp-gap-v1` | `ripr_get_gap` result |
| Repair attempt list | `ripr-mcp-repair-attempt-list-v1` | durable attempt discovery |
| Repair packet | `ripr-mcp-repair-packet-v1` | `ripr_prepare_repair` result |
| Repair attempt | `ripr-mcp-repair-attempt-v1` | attempt documents |
| Repair card | `ripr-mcp-repair-card-v1` | `ripr_get_repair_card` result |
| Receipt status | `ripr-mcp-receipt-status-v1` | receipt status documents |

`ripr_list_gaps` returns the snapshot's deterministic bounded working set:
`total` / `eligible` / `selected` / `filtered` counts, selected and complete
serialized bytes, every omitted identity with its reason, the
snapshot/profile/budget identity and selection basis, the echoed `filters`,
and one small summary per selected item. Each summary carries
`canonical_id`, exposure `class`, `language`, `file`, `line`, plus the
commit-time repair-readiness projection (`repair_packet_ready` with its typed
`repair_ineligibility` reason, `null` when ready) — the same
`repair_packet_ready` fact the item's `ripr_get_gap` readiness block reports,
so actionable items sort without one evidence call per candidate. Selection
is the shared CLI/LSP diagnostic-budget authority over the snapshot's
canonical items; eligibility is the producer's candidate-actionability
predicate captured at projection time, so non-actionable items appear as
disclosed profile-filtered omissions. The adapter never re-ranks,
never truncates silently, and infers no business risk. Overflow is disclosed
with reasons and the continuation route (`ripr_get_gap`). Pass `snapshot_id`
to bind the read to a specific snapshot: a mismatched identity fails closed
with `stale_snapshot` and the current identity.

Optional `file`, `class`, `language`, and `repair_ready` narrow the stored
selection over already-projected commit-time facts before paging
(conjunctive): `file` matches exact, prefix, or trailing-segment paths after
ignoring one leading `./` on both sides, `class` takes an exposure-class wire
token (for example `exposed`), `language` takes a language wire string (for
example `rust`), and `repair_ready: true` (or `false`) selects the
commit-time readiness projection. `selected` stays the full stored-selection
count while `filtered` counts the narrowed set the page walks. An unknown
`file` value is an empty disclosed page; an unknown `class` or `language` is
an `invalid_argument` tool failure (`isError: true`, see "Argument errors" below)
naming the accepted tokens in `failure.data.allowed` (the echoed token is
truncated to stay bounded). Empty filter strings and non-boolean `repair_ready` values are `invalid_argument` failures too.

Paging (#6021): `offset` (default 0) and `limit` (minimum 1) page over the
narrowed items in snapshot order, disclosed through the `page` window
(`offset`, `limit` when caller-set, `returned`, `has_more`, `next_offset`).
Optional tool fields must be omitted to use their defaults: explicit `null`
is invalid for `snapshot_id`, `offset`, `limit`, `file`, `class`,
`language`, and `repair_ready` (#7314). The server
returns an `invalid_argument` tool failure naming the field (`failure.data.argument`);
correct the argument or omit it and retry on the same connection.
`offset` must be a whole JSON integer (no fraction or exponent
form) in `0..=M` where `M` is the platform's `usize` width, and `limit` a whole
integer in `1..=500`, the working-set bound `budget.max_items_per_workspace_response`
(#11621; it was `usize::MAX`, which bounded nothing). `data.maximum`, the message
and `data.recovery` all name that one effective
bound; a larger `limit` is refused rather than clamped, so `page.limit` always
echoes a limit the server honors. The `ripr_list_repair_attempts` limit is capped at 256. Each refusal
names why the value missed (negative, not a number, fractional, larger than the largest
accepted integer) and carries `data.argument`, `data.minimum`, `data.maximum`
and `data.recovery` with the accepted range (#9895).
When the whole selection cannot fit one wire response, the default call
returns the first wire-fitting page and `continuation.next_page` names the
`ripr_list_gaps` call that resumes the walk — pinned to the snapshot
identity and the caller filters, so a refresh between pages fails closed with `stale_snapshot`
instead of silently mixing snapshots; walking `next_offset` covers the
selection exactly once. Pages are sized so the complete structured result
(`structuredContent`, per the tool's advertised `outputSchema`) measures
under the 128-KiB response bound, including empty pages whose echoed filters
can overflow the structured envelope while the raw document still fits.
Those pages return the existing typed `result_too_large` refusal with the
current snapshot identity. The omission disclosure no longer makes a listing
unservable by itself (#9360): `omitted_items` carries at most 200 identities
and at most 16 KiB per response (identities embed paths, so the byte budget
is the real bound; the first entry always ships), `omitted_page` (`offset`,
`returned`, `has_more`, `next_offset`) discloses the window, and
`continuation.next_omitted_page` names the `ripr_list_gaps` call that reads
the next window. Each continuation route replays the whole request, advancing
only its own axis: `next_omitted_page` keeps the snapshot pin, filters, item
`offset` and `limit`; `next_page` keeps the requested `omitted_offset`.
Walking `omitted_page.next_offset` covers every omitted identity exactly
once; a past-end `omitted_offset` is an empty disclosed window. Only a listing whose remaining non-pageable disclosure
(for example an oversized filter echo) cannot fit even one page fails closed
with `result_too_large` naming `ripr://snapshot/{snapshot_id}` as the
identity route. Other tool responses carry the document once at full fidelity:
by default `content[0].text` is the compact JSON serialization, and
`structuredContent` repeats it in every successful response. Each tool
advertises an `outputSchema`, so a success never ships without its
structured copy: when the complete double-carry envelope does not fit the
id-aware response budget, the call fails closed with the typed
`result_too_large` refusal and that tool's recovery route (#7768) instead
of silently dropping `structuredContent`.

### Text content mode (compatibility decision, #9360)

`structuredContent` is the single complete result and is identical in every
mode. The MCP specification asks a tool that returns structured content to
also return its serialization as a text block so hosts without structured
support keep working, and that is why the default doubles the payload.
Hosts differ in which copy they hand to the model, and a host that feeds only
text content to its model would silently lose the result if the text were a
digest — a worse failure than duplicated tokens — so the lossless copy stays
the default and the token saver is an explicit opt-in per server process:

| `RIPR_MCP_TEXT_CONTENT` | `content[0].text` on a successful result |
|---|---|
| unset, `full`, or any unrecognized value | the compact JSON copy of the document (default) |
| `summary` (ASCII case-insensitive) | a digest, not a copy |
| `brief` (ASCII case-insensitive) | for `ripr_list_gaps`, one line per gap; every other tool falls back to the `summary` digest (#11621) |

`brief` exists because `summary` shows only counts and identities for a list, so
a text-only host cannot tell which gap is which. A `brief` list is a header
(`ripr gaps: N shown, E eligible, O omitted`), then one line per item,
`<class> <file>:<line> <canonical_id> repair_ready=<true|false:<reason>>`, then,
when `page.has_more`, a `more: call ripr_list_gaps with offset=<next>` line, for
example `weakly_exposed src/lib.rs:259 gap:rust:... repair_ready=false:seam_already_gripped`.
Every value is a projection of `structuredContent` with control characters
escaped, so a document string cannot forge a line. It is opt-in, not the
default, for the compatibility reason below: a host that feeds only text to its
model would lose the detail that is not in a line (evidence, discriminators).

Set it in the host's server `env` (for example `"env": {"RIPR_MCP_TEXT_CONTENT":
"summary"}` beside `command` and `args`) only for a host known to read
`structuredContent`. The digest starts with a header naming the schema
version, saying it is not a copy and pointing at `structuredContent` and at
`RIPR_MCP_TEXT_CONTENT=full` for text-only hosts. It then has one line per
top-level field: scalars verbatim (strings longer than 200 characters are
clipped with a marker that points back to `structuredContent`), arrays as
entry counts with the first five `canonical_id` values so a text-only agent can
still call `ripr_get_gap`, and objects as their field names. It adds and
rewords no fact. Measured over stdio on a real repository (wire bytes of
the whole JSON-RPC line, default then `summary`): `ripr_get_gap` 19187 to 9864
(about half), `ripr_workspace_status` 5475 to 3362, `ripr_list_gaps` (6 items)
7237 to 5607, `ripr_refresh` 2365 to 1916. The saving is largest for
single-item evidence documents and grows with document size; a list page keeps
its leading identities in the digest. An
unrecognized value keeps the lossless default so a typo never shrinks what a
text-only host receives.

The mode changes only the text companion. Size admission is measured on the
complete double-carry envelope in both modes, so paging, `result_too_large`
and every other refusal fire at the same boundary whichever mode is set. The
typed-failure result (`isError: true`) and the extra recovery sentence the
status tool adds for an unavailable root are unchanged. The default is
revisited once a host's handling of `structuredContent` is observed rather
than assumed; changing it is a compatibility change for text-only hosts. #11621
considered making `brief` the default and kept `full`: no host-behavior
observation exists in this repository, and a wrong default silently removes
evidence from a text-only agent, while an opt-in costs one `env` line.

### Slim `tools/list`

`tools/list` is paid at every session start. About two thirds of it is the nine
`outputSchema` documents, which most coding agents never read. By default they
stay, because a host that validates results against them would lose its
contract. `RIPR_MCP_TOOLS_LIST=slim` (ASCII case-insensitive; any other value
keeps the default) omits every `outputSchema` and changes nothing else: results
still carry `structuredContent` with a `schema_version`, and the document shapes
are the ones this page and `docs/OUTPUT_SCHEMA.md` describe. Set it in the host's
server `env` beside `RIPR_MCP_TEXT_CONTENT`. Independent of the mode, the
repair-card description no longer repeats the failure-code list that the
failure `code` enum already carries.

Single-item gap overflow uses one captured-input recovery across the raw
document, tool envelope and resource envelope. Its CLI command retains the
captured repository root, effective base and explicit `--committed` or
`--worktree` source; a later failed request does not relabel that route. The
command analyzes current repository state, not immutable snapshot bytes. CLI
JSON remains bounded: related tests are capped and a large finding array may
omit findings. If the complete recovery exceeds its 4096-byte recovery-text budget,
the refusal explains that literal recovery is unavailable; it never emits a
truncated command.

Failed worktree refreshes preserve `source: "worktree"` in their MCP retry
examples, including default-base, explicit invalid-base and configuration-repair recovery. Correct
the base without silently changing the source. Ordinary committed requests
retain omission semantics.

`ripr_get_gap` (and the equivalent resource `ripr://gap/{canonical_id}`)
returns one canonical item's complete bounded evidence bound to its snapshot
identity: identity and location, the changed behavior (expression,
before/after, delta kind, probe family), causal attribution (canonical gap
owner, behavior kind, probe kind, normalized discriminator), discriminator
availability and the producer's observed/missing evidence, related tests with
oracle kind and strength, the nonnegative `related_tests_total` before the
producer's retained-row cap, and typed limitation states for anything the
producer did not establish. The readiness block reports the committed
producer repair-readiness facts (candidate actionability, an established
discriminator, and a strong directly-related test fix site on a test
surface) with the typed first-failing-gate reason when not ready — this
evidence never authorizes an edit by itself — and the repair boundary is
`none_declared` until `ripr_prepare_repair` binds a session transaction;
then the repair-attempt link names that transaction instead of staying an
explicit `null`. A missing field stays a typed state; MCP never fills it
from prose.

`ripr_prepare_repair` (`canonical_id`, optional `snapshot_id`) evaluates those
readiness facts for one canonical item and, only when every gate is
established, creates — or replays — one bounded in-memory repair transaction
bound to the current snapshot, the item, and the root identity. The packet
carries a deterministic `repair-attempt-` identity, the fix site (test file,
line, oracle), an `allowed_edit_surface` limited to that test file, the
edit-cage `must_not_change` statements, stop conditions, the before-evidence
identity, an empty `command_routes` list with the typed limitation (concrete
typed `CommandSpec` routes are published only by the durable CLI before
phase), the shared non-claims, and the resource links. An ineligible item
returns `isError: true` with `repair_packet_ready: false`, the typed
ineligibility reason, `attempt: null` and a machine-readable `next` object
(`kind` one of `nothing_to_do`, `write_test_by_hand`, `change_policy`,
`commit_then_refresh`, `await_producer_fact`; the `reason`; an `mcp` route or
`null`; and, only for `write_test_by_hand`, `after_editing_tests`) — no attempt
is created and no field is guessed, and `next` adds no verdict the shared gate
did not produce. `isError` is true exactly when no transaction exists. The reasons
the shared seam-level repair gate behind `ripr repair` contributes are
`seam_already_gripped` (the seam's existing tests already check it),
`seam_gate_refused` (route not ready, cross-language limits, policy
omission or an uncommitted inline test target), `seam_ambiguous` (several seams name the item), `seam_not_found`
(no classified seam names it) and `seam_inventory_truncated` (the repo-wide
seam inventory is truncated by the seam limit, or its completeness could not
be established, so `ripr repair` refuses every item; widen the budget with
`RIPR_REPO_EXPOSURE_SEAM_LIMIT`).
When that gate refuses an item, its reason is the item's first failing gate,
ahead of the producer-side reasons such as `fix_site_not_established` (the one
exception is a saved-worktree snapshot, #7357, whose withheld verdict is a
snapshot-level fact, so a finding-level gate stays first there), so
`ripr_prepare_repair` and `ripr repair` name the same cause for the same item
(#10718). The producer-side reasons remain for items the gate admits or when
no seam evaluation ran.

The truncation refusal covers repair readiness (`ripr_list_gaps`,
`ripr_get_gap`, `ripr_prepare_repair`). `ripr_get_repair_card` reads the same
completeness fact and, like `ripr agent card`, fails with the typed
`incomplete_inventory` code (and no card) while the inventory is truncated or
its completeness could not be established, so it never reports
`readiness.repair_ready` true over a truncated inventory (#9231).

`ripr_get_repair_attempt` (and `ripr://repair-attempt/{attempt_id}`) reads
one transaction by identity: session transactions answer first; otherwise
the durable attempt store of this workspace root is inventoried through the
shared repair-attempt authority and a valid manifest projects its state,
artifact digest bindings, after-phase bindings, and typed `CommandSpec`
routes when the retained packet carries valid ones — each projected exactly,
with the human display string marked as never execution authority. The
host-local root path is intentionally not projected.

Durable attempt and receipt reads report live HEAD applicability in
`currentness.state` (`current`, `historical`, or `unknown`), `head_current`
and `evidence_head`. This is the shared CLI attempt reading: awaiting ordinary
attempts admit descendant commits; finished evidence requires its exact after
HEAD. Historical or unknown applicability suppresses `next_command` and
`command_routes`. The operational manifest state and retained receipt bytes
remain readable. `after.current` and receipt `currentness.after_current` are
recorded finish-time admission, not live freshness or a test result. This HEAD
check does not establish that dirty working-tree bytes still match the evidence.
At a current HEAD, `next_command` consumes the same selected-attempt action
as `ripr agent status --attempt`: an awaiting attempt offers its after phase,
a finished current result offers none, and a failed or open-gap result offers
the shared new before-attempt command. HEAD equality alone never resumes an
already finished attempt. Retained typed packet routes are projected only for
a current after continuation; they are suppressed for terminal results and
restarts, and are never reconstructed by parsing the restart display string.
Historical and unknown reads retain the existing null-command/empty-route
boundary even when CLI recovery prose can describe starting new work.
Session transactions and supersession resolve under the session lock; durable
filesystem and Git fallback reads run on a blocking worker after releasing it.
The supported stdio transport admits one request until its reply is flushed,
so a refresh queued behind a durable read starts after that reply. The session
`analysis_in_flight` guard is evaluated at read admission. Offloading preserves
the async executor for timers and teardown; it does not establish a concurrent
public transport or cancellation of an already running Git read before its
existing deadline.

`ripr_get_receipt_status` (and `ripr://receipt/{receipt_id}`) projects the
current receipt state for one attempt identity (receipt ids are
attempt-bound) onto the vocabulary `awaiting_edit`, `after_pending`,
`verification_pending`, `improved`, `closed`, `unchanged`, `regressed`,
`limited`, `stale`, `invalid`. Session transactions report `awaiting_edit`
with an explicit `null` receipt; a finished durable attempt with a
digest-bound terminal receipt projects the receipt document with its exact
byte bindings. Terminal status uses the same receipt reading as CLI agent
status: an advisory receipt with improved static grip reports `improved`;
complete unchanged and regressed receipts preserve those movements. A
complete `changed` receipt leaves the gap open and reports `limited`.
Producer-invalid status reports `invalid` first. Recorded stale and
gap-mismatch lifecycle states retain `stale` and `invalid`, including with
incomplete producer status. Other producer-incomplete or unavailable
completeness reports `limited`, even when its recorded static movement is
improved. Advisory regression retains the trimmed, case-insensitive legacy
`static_movement.state` fallback after `provenance.movement` and before
`seam.change`; regression cannot bypass the completeness gate.
Receipt presence alone never reports `closed`; the
existing wire vocabulary remains unchanged. The nested producer document
preserves completeness and movement independently. A historical HEAD weakens
an otherwise actionable status to `stale`; unknown HEAD weakens it to `limited`.
Existing invalid, stale or limited producer states retain their refusal.
RIPR performs no verification and issues no receipt: the external client owns
the edit, the verification execution, and the receipt under its own authority.

The `ripr://snapshot/{snapshot_id}` resource returns bounded snapshot
evidence: the snapshot identity, the typed `AnalysisOutcome`, the full
canonical item index (identities and locations, not evidence), and the stored
bounded-selection summary.

`ripr_get_repair_card` (and the equivalent resource
`ripr://repair-card/{canonical_id}`) projects the bounded repair card
for one canonical item: the same versioned `repair_card.v1` document `ripr
agent card` and the standard language server project, assembled by the shared
application authority from the committed snapshot — the adapter never
re-derives readiness, identity, currentness, or route state. The analyzed
repository head and the diff-scoped classified seam inventory (each retained
seam owner-discriminated bound to a snapshot item, with its evidence-scope
dirty-state currentness probe) are bound when `ripr_refresh` commits the
snapshot — the adapter never launches git per read — so the card binds the
analyzed head and the commit-time currentness of its snapshot, and edits
after the refresh are visible only after the next one. The durable attempt
store is plain filesystem state and is re-read live at card-read time, so the
card's attempt block matches `ripr agent status` at that moment; in-memory
session transactions never ride a card. The next-action display binds the
portable root `.` and is presentation only, never execution authority.
The card's `subject.canonical_gap_id` is the bound finding's own id, the same
readable id `ripr_list_gaps` and `ripr_get_gap` return (#8638), so it can be passed back
to those tools (they do not accept the seam-side hash); that hash rides additively as
`subject.seam_content_gap_id`.

For a card whose `readiness.repair_ready` is not true, the result also carries
`finding_guidance` (#11213): the item's `missing` and `recommended_next_step`,
read from the same evidence `ripr_get_gap` serves, plus the `ripr_get_gap`
route. `ripr_prepare_repair` refusals carry the same `finding_guidance`. When
the card names CLI routes (`ripr agent packet`, `ripr explain`, `ripr check`),
`mcp_detail_routes` pairs each unchanged CLI string with `ripr_get_gap` and the
`canonical_id`, which carries finding-level evidence over MCP (not the
unbounded CLI packet). None of these fields changes readiness or any verdict.

## Typed failures

The evidence tools fail closed with structured content (`isError: true` and a
`failure` block carrying `code`, bounded `detail`, `recovery`, and small
structured `data`), never with a partial document. The shared vocabulary:

```text
workspace_unavailable  analysis_failed     unsupported_profile
no_snapshot            analysis_in_flight  stale_snapshot
item_not_found         result_too_large    attempt_not_found
config_invalid         workspace_ambiguous static_limitation
cancelled              superseded          attempt_invalid
seam_not_found         policy_omitted      witness_unavailable
identity_unnameable    budget_overflow     incomplete_inventory
```

Before the first successful refresh the evidence tools fail with
`no_snapshot` and the repair-attempt / receipt reads fail with
`attempt_not_found`; `superseded` is reachable for a session transaction
bound to a snapshot that is no longer current, while superseded tombstones
older than the 64-entry session bound read `attempt_not_found` instead
(oldest-first eviction); `attempt_invalid` reports a
durable manifest that fails canonical validation; `seam_not_found`,
`identity_unnameable`, `budget_overflow`, and `incomplete_inventory` are
reachable on the repair-card read (an item no seam owner-discriminated binds,
an unnameable portable workspace identity, a card over its detail budget, a
truncated or unverified repo-wide seam inventory). The card uses the CLI-pinned
`incomplete_inventory` code, while the list/get/prepare readiness fields keep
`seam_inventory_truncated`. `policy_omitted` is reachable on the same read
(#10718): the card applies the repository's agent policy to the seam's class
exactly like `ripr agent card`, so an already-gripped seam the policy omits
refuses with `policy_omitted`, and an item the repair-subject resolver maps to
no seam (for example a Python finding, since the seam inventory covers Rust
files only) refuses with `seam_not_found` carrying the CLI's own cause text.
The MCP policy gate uses the configuration captured at `ripr_refresh`, while
the CLI reads configuration at call time. `witness_unavailable` stays named for
wire stability even though this adapter cannot reach it; the rest stay reserved for the slices that own those
states (they are named now so the wire contract
stays stable). Argument problems on a known tool are `invalid_argument` tool
failures (see "Argument errors"); unknown tools use Invalid Params (MCP: the
`tools/call` method exists; the name argument is wrong) with the available names
in `error.data`, because no tool ran and no tool result exists. The
`tools/call` `arguments` member may be omitted; when present it must be an
object. Null, arrays and scalar values receive a correlated Invalid Params
(`-32602`) response before tool dispatch, including for `ripr_refresh` and
the no-argument status tool. Unknown
methods use Method Not Found (`Method not found: <method>`). `ripr_list_gaps`
rejects a `language` filter that is not a language wire string (`rust`,
`typescript`, `javascript`, `python`, `perl`) with an `invalid_argument` failure listing the
allowed values, instead of returning an indistinguishable empty page.

### Argument errors are tool results

A model can fix a bad argument only if it sees the error. Many hosts flatten a
JSON-RPC error to an opaque failure and drop `error.data`, so every argument
problem on a known tool (unknown argument name, wrong type, out-of-range
integer, unknown `class` or `language` token, missing or empty
`canonical_id`/`attempt_id`/`receipt_id`, an over-long `base`, a bad `source`)
is returned as a tool result with `isError: true` and the same typed failure
envelope as every other refusal (#11621):

```json
{"isError": true, "structuredContent": {"schema_version": "ripr-mcp-gap-list-v4",
 "failure": {"code": "invalid_argument",
             "detail": "unknown class \"weak\"; allowed: [\"exposed\", ...]",
             "recovery": "correct the named argument as described in `detail` and `data` ...",
             "data": {"argument": "class", "allowed": ["exposed", "..."]}}}}
```

`failure.detail` is the validator's sentence, `failure.data` keeps its
machine-readable fields (`argument`, `expected`, `allowed`, `minimum`,
`maximum`), and `failure.recovery` is the validator's `data.recovery` when it
names one. Still JSON-RPC errors: an unknown tool name, a `tools/call` whose
`arguments` is not an object or whose `name` is missing, a missing `_meta`, a
catalog `cursor`, and resource-read errors, because in those cases no tool ran.

### Reading errors: the JSON-RPC code is coarse by contract

A `tools/call` typed failure is a successful JSON-RPC response whose result has
`isError: true` and the `failure` block above; it is not a JSON-RPC error. A
`resources/read` typed failure is a JSON-RPC error whose `error.data` carries the
same typed state. In both, the JSON-RPC `error.code` is deliberately coarse
(#9854): it separates only "the id you asked for does not exist" from "the
server is in a state or bound that this request cannot satisfy". Do not branch
retry, refresh, or give-up behavior on `error.code`. Branch on `error.data.code`
(the stable typed kind) and follow `error.data.recovery` (one sentence naming
the next step). `error.data.detail` is bounded human text, and
`error.data.current_snapshot_id` is present on snapshot-aware reads. The
`error.message` sentence is `unavailable <kind> resource: <data.code>`.

The server owns this mapping in one place (`resource_failure`); the table below
lists every `data.code` kind in the shared vocabulary and the JSON-RPC code a
`resources/read` of that kind answers. A test pins the set of `data.code` kinds (equal to the vocabulary) and each
kind's JSON-RPC code (equal to the mapping), so a new or removed kind or a
changed code fails the build until this table is updated. The Class and
recovery columns are guidance and are not test-pinned. The code column is what
`resource_failure` would answer for that kind; for the rows marked tool-path
only it is the mapping, not an observed `resources/read` response.

| `data.code` | resources/read `error.code` | Class | Typical recovery |
| --- | --- | --- | --- |
| `invalid_argument` | `-32603` | caller error; tool-path only, not produced by `resources/read` | fix the argument named in `data.argument` (`data.allowed` lists accepted tokens) and call again |
| `item_not_found` | `-32002` | identity miss (`-32602` for 2026-07-28 peers) | the id is not in the current snapshot; re-list with `ripr_list_gaps` |
| `attempt_not_found` | `-32002` | identity miss (`-32602` for 2026-07-28 peers) | the attempt/receipt id does not exist; list attempts |
| `no_snapshot` | `-32603` | retryable after refresh | call `ripr_refresh`, then retry |
| `stale_snapshot` | `-32603` | retryable after refresh | re-read with the `current_snapshot_id` in `error.data` or refresh; a card refused because a bound source file changed since refresh needs `ripr_refresh` (the id alone still refuses) |
| `analysis_in_flight` | `-32603` | retryable after wait | another attempt is running; wait and retry |
| `superseded` | `-32603` | retryable after refresh | the transaction's snapshot is no longer current; refresh and re-prepare |
| `result_too_large` | `-32603` | bound; narrow the request where a narrower read exists | pageable lists: page or filter. Single-item resources and status/repair documents can have no narrower MCP read; follow `error.data.recovery`, which is authoritative. Retrying unchanged cannot succeed |
| `budget_overflow` | `-32603` | bound; narrow the request | the card exceeds its detail budget; use `ripr agent card` |
| `incomplete_inventory` | `-32603` | state; not retryable as-is | the repo-wide seam inventory is truncated or unverified |
| `seam_not_found` | `-32603` | state; not an id miss | no owner-discriminated seam binds the item |
| `id_ambiguous` | `-32603` | state; change the id form | the raw id resolves two ways; use the canonical id |
| `identity_unnameable` | `-32603` | state; not retryable as-is | no portable workspace identity can be named |
| `attempt_invalid` | `-32603` | state; not retryable as-is | a durable manifest fails canonical validation |
| `workspace_unavailable` | `-32603` | operator action | restart with `ripr mcp --stdio --root <repository>` |
| `analysis_failed` | `-32603` | operator action | fix the named cause or pass `base`, then `ripr_refresh` |
| `unsupported_profile` | `-32603` | operator action; tool-path only, not produced by `resources/read` (`ripr_refresh`) | the workspace input is unsupported; narrow the diff and retry `ripr_refresh` |
| `config_invalid` | `-32603` | operator action; tool-path only, not produced by `resources/read` (`ripr_refresh`) | repair `ripr.toml`, then `ripr_refresh` |
| `stale_inventory` | `-32603` | retryable after refresh; tool-path only, not produced by `resources/read` (`ripr_list_repair_attempts`) | re-list the attempt inventory |
| `policy_omitted` | `-32603` | operator action; reachable on the repair-card read (#10718) | the agent policy omits this seam's class; inspect it with `ripr agent brief --seam-id <seam> --json` |
| `witness_unavailable` | `-32603` | named, unreachable here | reserved for wire stability |
| `workspace_ambiguous` | `-32603` | reserved | reserved for a later slice |
| `static_limitation` | `-32603` | reserved | reserved for a later slice |
| `cancelled` | `-32603` | reserved | reserved for a later slice |

`analysis_failed` is also the code for a contained internal handler panic in
any tool, including a glue panic in `ripr_refresh` (#11116). It names the tool and is retryable:
retry the tool, then `ripr_refresh`; if it repeats, restart `ripr mcp` and
report the stderr panic message. A glue panic in `resources/read` or
`tools/list` answers a `-32603` JSON-RPC error with `error.data.code`
`analysis_failed` and the session keeps serving. The `initialize` handshake
runs inside the SDK before the service loop and is not covered.

Genuine invariant failures are different: the server could not build or
serialize a result it should always be able to build (`serialize workspace
status`, `serialize resource`, `serialize typed failure`, `invalid status
projection`) or a response exceeded the transport byte cap. These answer
`-32603` with no `error.data.code` (the byte-cap fallback carries only
`maxResponseBytes`). Within `-32603`, the absence of `error.data.code` is how a consumer tells an
invariant failure from a typed state (an unknown resource URI at `-32002` and
protocol-level argument errors at `-32602` also carry no `data.code`); it is not retryable by refreshing, and is
a server defect worth reporting. Argument problems on a known tool are
`invalid_argument` tool failures, not `-32602`.

## What it does not do

It does not edit source, execute verify commands or mutation testing, run
verification or issue receipts, or create durable repair attempts (durable
attempt creation stays CLI-owned; the adapter's session transactions are
in-memory and its durable-store reads are read-only). A prepared transaction
never authorizes an edit, a command, or a merge by itself — the external
client's approval and sandbox policy remains authoritative. It does not
load project-local provider configuration, embed a model, or offer a
remote transport (the workspace's own `ripr.toml` is honored by
`ripr_refresh`, as described above). It does not watch the worktree: the snapshot is current as
of its completed `ripr_refresh`, so refresh again after edits. The session
is in-memory; restarting the server drops the snapshot and every session
transaction unless a new refresh commits one.

An invalid root does not stop the server. Status reports
`workspace_state: "unavailable"` with a `root.error_code`, and the tool result
adds a second text content item that names the cause and the recovery
(restart with `--root <repository>`). An unknown resource URI is
rejected with the valid static URI and all five resource templates in the
human-readable message. The same routes remain available separately in
`error.data.available` and `error.data.resource_templates`. Templates require
valid resource identities; listing a template does not promise its resource
currently exists. Recognized snapshot, gap and repair resource routes retain
their typed missing/stale-identity errors without the generic route list.
Identity misses on those routes (`item_not_found`, `attempt_not_found`) keep
resource-not-found: a client that negotiated an older protocol version gets
`-32002`; current clients get Invalid Params (`-32602`). Every other typed
resource failure is a server state or a bound, not an identity claim —
`no_snapshot`, `stale_snapshot`, `analysis_in_flight`,
`workspace_unavailable` and `result_too_large` answer Internal Error
(`-32603`) on every protocol version, with the same human sentence, the
same structured recovery routes, and the stable typed state in
`error.data.code` in both lifecycles.
The instructions (returned by both `initialize` and `server/discover`) and the
tool descriptions state positively what each tool returns, its bounds, and
what it does not do. Protocol errors keep standard JSON-RPC codes. The pinned
official Rust SDK owns negotiation, dispatch, correlation and cancellation.
Every message that cannot be typed is answered, so a client never waits on
silence: syntax-invalid JSON (including nesting past the parser limit) receives
Parse error (`-32700`, null id) and well-formed JSON that is not a request the
SDK can type (null or fractional id, wrong `jsonrpc` version, missing method)
receives Invalid Request (`-32600`); the transport then reads the next frame.
The reply carries the request id