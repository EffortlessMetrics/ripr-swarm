# RIPR-SPEC-0211: MCP workspace session — status, refresh, bounded gap lists, and evidence resources

Status: proposed

Owner: mcp

Created: 2026-10-06

Linked issues:

- #8847 (retain producer matched totals in bounded gap evidence and identity)
- #7767 (unknown-resource recovery remains useful to message-only clients)

- #7314 (optional tool arguments reject explicit null consistently with
  their advertised non-null input types)
- #9895 (a SIGTERM (or SIGINT) arriving mid-refresh must not leave the
  in-flight git child behind: the server entry installs cooperative
  process-lifetime interrupts on Unix)
- #9895 (a client-supplied pagination cursor on the static catalog lists
  fails closed instead of being accepted and silently ignored)
- #3089 (this slice: status, refresh, bounded gap list, and evidence
  resources)
- #5267 (every pre-initialize typed-shape violation recovers with the
  SDK's Invalid Request answer on the same connection instead of
  terminating the server process)
- #6022 (post-initialize `ping` with handshake `params._meta` returns an
  empty result; pre-init ping+_meta and discovery-lifecycle ping
  rejection stay with #5267 / ADR 0022)
- #9360 (the text companion of `structuredContent` is selectable: lossless
  JSON copy by default, opt-in digest via `RIPR_MCP_TEXT_CONTENT=summary`)
- #6021 (the tool envelope must not fail a budget-approved listing on the
  wire: one calibrated envelope, paging on `ripr_list_gaps`)
- #3087 (parent standard MCP server epic)
- #3088 (transport/discovery/read-only boundary slice; this slice keeps its
  SDK-owned dispatch and bounded framing)
- #3090 (sibling repair slice; this slice leaves the repair-attempt and
  receipt resources as explicit nulls reserved for it)
- #1602 (shared completed-analysis lifecycle and currentness authority;
  consumed through `app::check_workspace`)
- #1582 (bounded selection and overflow authority; consumed through
  `lsp::diagnostic_budget`)
- #1895 (shared fix-instruction meaning; not projected in this slice)

Support-tier impact:

- None. The MCP adapter runs the same read-only static analysis the CLI and
  LSP run in-process; it adds no language support claim. Preview-language
  findings keep their existing support posture through the shared producer.
  [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md)

Policy impact:

- None. No new output format, gate, badge, or report file is added; the MCP
  documents are wire payloads bounded by the existing MCP byte caps.

## Problem

Agents need RIPR-specific discovery and evidence through standard MCP
tools/resources, not custom LSP requests or direct report-file archaeology.
The status-only MCP slice (#3088) deliberately answered "is the root usable"
and nothing more; an agent that wants the current bounded evidence working
set had to leave the protocol and reconstruct state from CLI output.

## Behavior

`ripr mcp --stdio` exposes one read-only workspace session over the pinned
official SDK transport:

- Tools `ripr_workspace_status`, `ripr_refresh`, `ripr_list_gaps`,
  `ripr_get_gap`; static resource `ripr://workspace/status`; resource
  templates `ripr://snapshot/{snapshot_id}` and
  `ripr://gap/{canonical_id}`. Tool names and templates follow the
  registry established in #3088's slice.
- Optional tool arguments are omittable, not nullable. `snapshot_id` must
  be a string when present; `ripr_list_gaps`'s `offset` and `limit` must
  be integers within their advertised ranges. Explicit null returns a
  correlated `-32602` Invalid Params error naming the field before session
  lookup. Omitted fields retain their defaults, valid values reach session
  lookup, and a subsequent request can recover on the same connection.
- The `tools/call` `arguments` member is also omittable, but when present
  must be an object. Explicit null is Invalid Params (`-32602`), including
  for no-argument tools and `ripr_refresh`, before tool dispatch. Both
  lifecycles retain metadata and request-ID validation precedence; valid
  omission and an empty object keep their existing tool-specific behavior.
- The catalog lists (`tools/list`, `prompts/list`, `resources/list`,
  `resources/templates/list`) are static and complete: the server returns
  every entry in one response and never issues a `nextCursor`. A
  client-supplied `cursor` can therefore only be stale or fabricated;
  instead of being accepted and silently ignored (which would let an agent
  believe paging worked while it re-read page one), it fails closed with a
  `-32602` Invalid Params error naming `cursor` and the recovery — drop the
  argument (#9895). Every non-null cursor is refused, including malformed
  boolean, number, array, or object values that the SDK optional-param decoder
  would otherwise discard. Null retains the SDK omission semantics. The raw
  input guard preserves metadata, protocol and request-ID validation precedence.
  The SDK's default `prompts/list` answers an empty catalog, so the override pins the same rule there instead of inheriting
  the silent drop. Tool-level paging stays on `ripr_list_gaps`'s explicit
  `offset`/`limit`, which is disclosed through `page`.
- Pre-initialize typed-shape violations recover on the wire (#5267): a
  request that arrives before `initialize` and lacks the SDK's required
  `_meta` fields — and any other pre-initialize message the SDK refuses —
  receives the SDK's typed Invalid Request error and ends only that
  handshake attempt. The bounded adapter restarts the handshake on the same
  connection (the frame reader, its already-buffered pipelined bytes, the
  shared writer, and admission state survive across attempts), so every
  violation is answered idempotently and the server never trades the whole
  session for one message; a normal `initialize` still completes afterwards,
  and stdin EOF remains the clean exit. Each attempt consumes at least one
  inbound frame, so recovery always makes progress and never spins.
- External termination requests settle instead of orphaning children
  (#9895, Unix). The server entry installs process-lifetime cooperative
  interrupts for SIGINT and SIGTERM before serving: the first signal of
  either kind requests cancellation of the in-flight bounded attempt — the
  child waits (git probes, the Perl exporter, cat-file batch reads)
  observe the flag at their poll cadence, terminate the owned process
  tree, and unwind, so no git child outlives an interrupted `ripr_refresh`
  — and the refresh settles on the same session with the typed failure in
  its `attempt` block (a completed document, `isError` false per the #5254
  commit contract).
  A sequential repeated signal before attempt settlement restores native
  termination as the escape; process exit itself stays transport-owned
  (stdin EOF). One atomic terminal exchange consumes the attempt's signal:
  a signal observed during final projection converts an otherwise successful
  result to the typed cancellation, and a signal after that boundary belongs
  to the next attempt. Configuration refusals also settle the pending signal.
  Repeated native death can bypass cooperative cleanup and leave active children;
  it is an escape, not a cleanup guarantee. Process-group containment covers
  descendants that remain in the owned group, not deliberate group/session escape.
  Later refreshes remain usable. Installation is serialized and idempotent.
  Only MCP attempt tokens opt into Unix process groups; captured analysis
  workers retain that choice and other callers preserve their prepared groups.
  Cleanup observes leader exit without reaping, signals the owned group, then
  reaps and disarms further group signals. This also settles pipe holders when
  a primary exits early. Targets lacking the safe non-reaping observation
  refuse contained spawn before creating a child. On non-Unix targets the
  install is a no-op: containment there is the job-object ownership in
  `process_owner`, whose std API cannot express kill-on-job-close, so an
  abrupt external termination can still outlive a git child (recorded
  residual).
- After a successful `initialize` handshake, `ping` is routed by method
  name. A request whose `params._meta` carries the handshake fields this
  server requires pre-init (`io.modelcontextprotocol/protocolVersion`,
  `io.modelcontextprotocol/clientCapabilities`,
  `io.modelcontextprotocol/clientInfo`) is answered with an empty result
  `{}`, the same as `ping` with empty `params` or omitted `params`. That
  `_meta` shape must not be read as a `2026-07-28` discovery-lifecycle
  request and must not produce `-32601` method-not-found. Pre-initialize
  ping with the same `_meta` remains the handshake path (#5267). After
  `server/discover`, `ping` stays method-not-found.
- `ripr_workspace_status` returns the startup workspace block (root
  discovery, configuration presence, trust, authority — all unchanged from
  #3088) plus a `ripr-mcp-session-v1` session block: current desired input
  (workspace diff against the resolved default branch or the explicit
  refresh `base` with `base_revision` when present, draft mode, built-in
  defaults), current attempt state (`no_snapshot`, `in_flight`,
  `completed`, `failed`), last completed snapshot identity,
  last-known-good, freshness relative to the last completed refresh
  (`current_at_last_refresh`, or `stale_after_failed_attempt` once a later
  attempt fails, leaving the retained snapshot unverified for that attempt),
  the committed
  snapshot's typed `AnalysisOutcome`, and profile/support facts. No
  evidence detail is returned.
- The session block additively carries `language_completeness`
  (`[{language, status}]`, #6475): one row per language the committed
  snapshot's analysis recorded, in canonical label order, with the closed
  status spelling the producer's aggregate owns (`complete`,
  `complete_zero`, `partial`, `unavailable`, `invalid`, `failed`,
  `unsupported`, `not_selected`, `no_current_scope`). It is read from the
  producer's crate-private completeness view bound at commit time, not
  re-derived from limitation kinds. It is emitted only when a committed
  snapshot carries a view. The presence is intentionally conditional: the
  field appears only with a committed view, and omission preserves the
  previous shape only when there is no committed view (a committed snapshot's
  status now carries it). The `ripr-mcp-session-v1` version is kept, as for
  the conditional `base_revision`; the output schema declares the property
  optional. Supported readers are those that validate against the advertised
  `outputSchema` (`tools/list`) and readers that tolerate unknown keys; a
  reader pinned to a frozen strict-closed session shape is outside that
  support (MCP behavior is Unstable in `docs/SEMVER.md`). No deployed-client
  failure is claimed. The `ripr://workspace/status` resource renders the same
  session block and carries the field too; the snapshot, gap and other
  evidence resources do not. The field is not part of the snapshot identity,
  and no existing field changes.
- `ripr_refresh` runs one bounded static analysis through the shared
  `app::check_workspace` authority on the committed default-branch diff
  and commits the completed snapshot. Initialize/discover instructions,
  the refresh tool description, and MCP help disclose before invocation
  that staged and unstaged tracked edits and untracked files are excluded.
  Shared authority does not mean identical input: `ripr check` defaults
  to the working tree when staged or unstaged tracked edits exist, while
  `ripr check --committed` selects committed input. The response scope
  disclosure and MCP's runtime input remain unchanged. Status always emits
  `session.scope`, and refresh always emits top-level `scope`; both output
  schemas require this nullable field. It is null without a snapshot or for a
  clean committed snapshot. A dirty snapshot discloses `analyzed`,
  `uncommitted_edits`, and `note`; untracked edits additionally disclose
  `untracked_edits` with one to three named `files`, positive `total`,
  nonnegative `more`, `listing_example`, and `remedy`. Scope objects reject
  undeclared fields. A failed refresh retains the last-known-good scope.
  The call
  blocks to a terminal state and reports it. An attempt runs to a terminal
  state: cancelling the MCP request never rolls an attempt back or
  manufactures a snapshot, and a cancelled or superseded attempt is never
  committed. A failed attempt keeps the last-known-good snapshot.
  Closed incomplete outcomes (`unsupported_input`, `analysis_failed`) stay
  typed failures and never become snapshots; `partial_with_limitations`
  commits with its typed limitations so complete-zero and incomplete-zero
  remain distinct.
- `ripr_refresh` takes one more optional argument, `source` (#7357), with
  the closed values `committed` and `worktree`. Omission and `committed`
  are the same request and keep every behavior and identity above. A value
  outside the closed set, including an empty string, a different case, a
  non-string or an explicit null, is Invalid Params (`-32602`); nothing is
  coerced. `worktree` routes the same attempt through the shared
  `check_workspace_worktree_with_config` producer: the diff from the
  resolved (or explicit) base to the saved working tree, covering staged
  and unstaged tracked edits. Unsaved editor buffers are outside, and
  untracked files stay excluded unless the existing intent-to-add
  mechanism (`git add -N <paths>`) makes them visible; the scope
  disclosure names them with that remedy. The server adds no Git or
  worktree digest of its own and still does not watch the worktree.
  - Requested and captured source are distinct facts. The session records
    the requested source when an attempt starts
    (`session.current_desired_input.source: "worktree"`, emitted only for a
    worktree request); each snapshot records the source it was captured
    under. The refresh result adds `requested_source` and `captured_source`
    (`null` before any snapshot) once a worktree source is requested or
    captured, and snapshot summaries (`snapshot`, `last_completed_snapshot`,
    `last_known_good`) add `source: "worktree"` for a worktree capture.
    A failed attempt keeps the last-known-good snapshot with its own tag: a
    failed worktree request never relabels a retained committed snapshot,
    and a failed committed request never relabels a retained worktree one.
    Source-less callers see the unchanged v1 shapes.
  - The source kind is part of a worktree snapshot's identity (the committed
    identity payload is unchanged, so existing committed identities do not
    move). Equal findings captured under a committed and a worktree source
    are different snapshots, so a snapshot id pinned before a switch fails
    `stale_snapshot` after it and a committed -> worktree -> committed
    sequence cannot reuse the other kind's result. The identity is as strong
    as the producer's outcome identity (a diff-byte commitment) plus the
    canonical items; it is not a complete consumed-source commitment.
  - Producer custody (#7357). A worktree refresh runs the shared findings
    producer and then the same diff-scoped seam inventory a committed refresh
    runs, over the live saved files. The findings producer already records a
    private SHA-256 of each raw Rust buffer it loads, before parsing or fact
    cache access; the seam inventory records the same observation for each
    corpus buffer while it is live in its hashing pass, which its index pass
    must reproduce or the run fails. On a memory or persisted result reuse the
    carrier is this invocation's validated observation, never an earlier
    one. The two carriers are compared by the one shared source authority
    (`ConsumedRustSources::establish_agreement`): every path the findings
    producer consumed must be captured by both with the same digest, and every
    inventory observation must be a captured one. A missing, unavailable,
    conflicting, out-of-root or moved observation, a findings producer that
    recorded nothing, or a `HEAD` that differs from the head the findings
    producer observed, leaves custody not established. The server adds no Git
    or worktree digest and never rescans the tree to mint one.
  - Established custody enables exactly the committed path: the shared
    seam-level repair validator, the owner-discriminated card bindings, card
    reads and `ripr_prepare_repair`, all behind their existing per-item
    gates. The producer's conservative Rust-context commitment (a digest over
    the sorted per-path digests of everything the inventory read, so an edit
    to any consumed helper or test changes it even when findings and the diff
    are equal) is bound into the worktree snapshot identity. It is not a
    complete portable dependency identity: it covers Rust sources only and
    nothing outside the inventory corpus.
  - Not-established custody (a source or head that moved between the two
    producers, or any unobserved input) withholds readiness and cards instead:
    every item's `repair_packet_ready` is false with the typed
    `worktree_source_not_committed` reason (existing finding-level refusal
    reasons retain precedence) whose detail names the cause,
    `ripr_prepare_repair` creates no attempt, and `ripr_get_repair_card`
    refuses with the existing `incomplete_inventory` code. Findings, list and
    gap evidence are still served. A fresh refresh over the settled bytes
    recovers.
  - A worktree snapshot's `scope` always exists, with `analyzed` naming the
    saved tracked-worktree diff and `uncommitted_edits: "saved tracked edits
    included"`. `consumed_source_commitment` is `established` with
    `repair_cards: "available"` when custody holds, and `not_established`
    with `repair_cards: "withheld"` otherwise; the note names the cause.
    Unsaved buffers and gitignored files stay outside either way, and
    untracked files are never analyzed as findings; the seam inventory walks
    disk, so an untracked Rust file it reads is bound into the commitment (see
    the residuals). The server still does not watch the worktree. If the seam
    inventory cannot be gathered after the findings producer ran, the refresh
    still serves the findings with custody `not_established` and the failure
    named, rather than failing the whole refresh (a cancelled attempt still
    fails). A committed snapshot whose producers agree
    makes no custody claim and keeps its unchanged contract.
  - A committed refresh with findings reads committed bytes for the findings
    and live checkout bytes for the inventory. On every such refresh the
    shared `establish_agreement` owner compares what the findings producer
    consumed with what the inventory read, whether or not the checkout was
    dirty when the findings ran (an edit landing after the findings is caught
    too), and a head the findings producer observed that differs from the
    inventory's also withholds (an unobserved head is not compared on the
    committed path, an existing residual). Because the agreement only covers
    paths the findings consumed while the inventory reads its own selected Rust
    files, any dirty tracked or untracked Rust-routed path (a `.rs` file,
    including a helper or test outside the findings scope) also withholds when
    there are Rust findings; if that list cannot be established the refresh
    withholds (and its remedy is to refresh again). A dirty non-Rust file
    (README, `.py`) does not withhold Rust cards, and with no findings no
    custody claim is made. Withholding refuses `ripr_get_repair_card` with
    `incomplete_inventory` and the `scope` adds
    `consumed_source_commitment: "not_established"` and
    `repair_cards: "withheld"`. Per item, an item that was repair-ready
    becomes `repair_packet_ready: false` with the typed
    `worktree_source_not_committed` reason; an item already not ready keeps
    its first-failing finding-level reason. The cause names the moved path
    when the owner can (a consumed file edited, staged or deleted, or an
    untracked Rust file the committed findings could not capture) and
    otherwise says an uncommitted Rust source or test file is read by the
    inventory. Findings, list and gap evidence are still served; the remedy is
    to commit the uncommitted Rust source or test files (untracked ones
    included) and refresh again, or refresh with source `worktree`. No
    commitment is bound into a committed identity.
  - Card currentness at request time (#7357). The refresh-time git-status
    probe and custody are not re-run per read, and the server does not watch
    the worktree, so `ripr_get_repair_card` re-verifies, on every read, the
    captured SHA-256 of only the files the card's evidence is bound to (the
    seam file, its related tests and the recommended test file) through the one
    shared `ConsumedRustSources::first_drift` owner. A captured file whose
    bytes now differ, or that can no longer be read, refuses with the existing
    `stale_snapshot` code (carrying `current_snapshot_id`), names the path, and
    says to run `ripr_refresh` again; this covers an uncommitted edit, a commit
    made after the refresh, and a deletion, for committed and worktree
    snapshots alike. Files with no captured commitment (non-Rust, outside the
    inventory corpus) make no claim, edits to unrelated files keep the card,
    and restored bytes keep it. The clean path costs one read-and-hash of those
    few files and runs no git child. Residual: only the evidence-bound Rust
    files are re-verified; a `HEAD` move that changes no bound file is not
    detected per read, and the card's `currentness.basis` remains the
    refresh-time statement.
  - Residuals under #7357 / #1765 / #1602, worktree mode: a Rust file
    that only the inventory read and the findings producer did not (a new file
    or one the findings scope did not select) is bound into the commitment but
    cannot contradict the findings; an untracked (not intent-to-add) Rust file
    is not analyzed as a finding but is read by the inventory, so editing it
    changes the commitment and snapshot identity. In committed mode those same
    tracked-dirty and untracked files withhold instead (see above). Both
    modes: gitignored Rust files can be read by the inventory (workspace
    discovery does not apply `.gitignore`) but are not claimed; committed mode
    does not list them because their enumeration is capped and would
    over-withhold clean checkouts. Unsupported, generated,
    `include!` and mixed-language dependencies are not claimed; a deleted or
    renamed tracked Rust file in the findings scope has no captured bytes and
    may refuse custody (fail-closed, cards withheld with the cause). A
    non-normal (empty, absolute or escaping) observed path is recorded as an
    uncaptured observation and refuses.
- The snapshot identity is `snapshot:sha256:` over the typed outcome digest,
  the sorted canonical item identities, and each item's evidence digest:
  equivalent roots at equivalent inputs share one portable identity, while
  the concrete root evidence stays the separate host-local root hash. An
  evidence change therefore yields a new snapshot identity instead of
  serving altered bytes under the old one.
- `ripr_list_gaps` serves the snapshot's stored shared-budget selection
  (`lsp::diagnostic_budget` with its default budget): total/eligible/
  selected/omitted counts, selected and complete serialized bytes, every
  omitted identity with its reason, the snapshot/profile/budget identity
  and selection basis, and one small summary per selected item. Eligibility
  is the producer's candidate-actionability predicate captured at
  projection time, so non-actionable items surface as disclosed
  profile-filtered omissions rather than MCP inventing its own filter. The
  adapter never re-runs ranking, never truncates silently, and infers no
  business risk. Overflow is disclosed with reasons and the
  `ripr_get_gap` continuation route.
- Optional `file`, `class`, `language`, and `repair_ready` filters narrow
  the stored selection by conjunction before paging; they never re-run
  analysis or widen the shared budget. File matching removes a leading `./`
  and accepts exact paths, segment-bounded prefixes, and trailing segments.
  Class/language are exact wire tokens. An unknown `class` is Invalid
  Params (`-32602`) naming the seven exposure-class tokens, with the echoed
  token truncated so the error stays bounded; an unknown `language` is
  Invalid Params (`-32602`) listing the allowed wire strings, because an
  empty page would read as "no gaps in that language"; a known value with
  no match, and an unknown `file`, produce an empty disclosed page.
  `selected` remains the stored-selection count; `filtered` is the narrowed
  count. `filters` echoes all four dimensions, with null for omitted values.
  Summaries carry `repair_packet_ready` and `repair_ineligibility` (null when
  ready; otherwise the typed first failing gate), matching `ripr_get_gap`.
  `continuation.next_page.arguments` pins the snapshot, filters, and explicit
  limit; replay cannot cross a completed refresh that replaced the snapshot.
- Gap-list success and refusal documents use `ripr-mcp-gap-list-v4`.
  v4 adds `omitted_page` and the `omitted_offset` argument (#9360): the
  omission disclosure pages on its own axis, at most 200 identities and 16 KiB per response, so
  a large truncated selection no longer fails the listing with
  `result_too_large`. `continuation.next_omitted_page` replays the snapshot pin, filters, item
  window and limit, and `next_page` keeps `omitted_offset`; walking `omitted_page.next_offset` covers every omission once.
  The former v1 success schema closed its top level with
  `additionalProperties: false`, so adding `filtered` and `filters` cannot
  retain strict v1 compatibility. Clients must rediscover `tools/list`, accept
  v2 explicitly and use the advertised output schema. The input arguments
  remain compatible for callers omitting filters; strict v1 output readers
  require migration. Other MCP document versions are unchanged.
- The tool envelope carries the document once at full fidelity (#6021):
  by default `content[0].text` is the compact JSON serialization of the
  structured document (no pretty-print inflation), and `structuredContent` repeats
  the same document in every successful response. Every tool advertises an
  `outputSchema`, so a success whose structured copy is absent is an
  undeclared contract break: when the complete double-carry envelope does
  not measure under the id-aware response budget, the call fails closed
  with `result_too_large` naming both sizes and the caller's recovery
  route instead of shipping a text-only success (#7768 — the former
  fallback that dropped `structuredContent` past the budget was the
  silent drop this contract removes). The
  id-aware budget is the 128-KiB response bound minus the JSON-serialized
  request id of the call being answered (so client-controlled id escaping
  is charged exactly) minus a fixed reserve for the JSON-RPC frame fields
  and the `resultType` field a `2026-07-28` peer keeps (#7768); the same
  reservation applies to the paging fit decision, so a page that passes it
  still carries `structuredContent` on the wire whatever id the client
  sent.
- The text companion is selectable per server process (#9360).
  `structuredContent` is the single complete result and is identical in
  every mode. `RIPR_MCP_TEXT_CONTENT` unset, `full` or any unrecognized value
  keeps `content[0].text` the compact JSON copy above, so a host that reads
  only text content loses nothing. `summary` (ASCII case-insensitive, trimmed)
  makes `content[0].text` a digest that states it is not a copy and names
  `structuredContent` and `RIPR_MCP_TEXT_CONTENT=full`, then lists one line per
  top-level field: scalars verbatim (strings over 200 characters clipped with
  a marker pointing back to `structuredContent`), arrays as entry counts plus
  at most five leading `canonical_id` values, objects as field names. The
  digest is a projection of the document and adds no fact. The size guard,
  paging and `result_too_large` measure the complete double-carry envelope in
  both modes, so refusal boundaries do not depend on the mode; typed failures
  (`isError: true`) and the status tool's recovery sentence are unchanged. The
  default is not flipped because a host that hands only text content to its
  model would silently lose the result; revisiting it is a compatibility
  change for text-only hosts.
- `ripr_list_gaps` accepts `offset` (default 0) and `limit` (minimum 1)
  over the narrowed (filtered) items in snapshot order. When the narrowed set
  would exceed the wire-calibrated tool-document ceiling (half the
  response bound, the worst-case escaping budget), the default call
  returns the first wire-fitting page and discloses the window through
  `page` (`offset` echoing the request, `limit` when the caller set it,
  `returned`, `has_more`, `next_offset`); walking `next_offset` covers the
  narrowed set exactly once. Pages are sized so the complete double-carry
  envelope measures under the response bound: a paged success always
  carries `structuredContent`, as the tool's advertised `outputSchema`
  requires. Empty pages obey the same structured-envelope bound: echoed
  filters can exceed it even when the raw document fits. Such a page fails
  closed with the existing typed `result_too_large` refusal and current
  snapshot identity instead of a text-only success. The generated `next_page`
  route pins the snapshot identity,
  filters, and explicit limit, so
  a refresh between pages fails closed with `stale_snapshot` instead of
  mixing snapshots. Only a listing that cannot fit even one page fails
  closed with `result_too_large`: when the echoed filter value itself is
  what pushes the shell past the bound — the same shell with an empty
  filter echo fits — the refusal names the filter as the shrinkable bound,
  because the snapshot identity route cannot fix a filter-driven overflow;
  for every other non-pageable disclosure (for example an omission
  disclosure alone over the ceiling) the refusal names
  `ripr://snapshot/{snapshot_id}` as the identity route (#9895).
- Every served file path renders relative to the analyzed workspace
  root, so a file under the root never appears as an absolute host path
  and the snapshot identity stays portable across checkouts (#5254
  item 6). Finding locations (list summaries, gap locations) render
  through the shared finding-location owner (#5996) with its `./`
  prefix, matching the check, context, and LSP surfaces; related-test
  files and fix sites render through the repository-relative renderer,
  which additionally tolerates producer spelling drift (canonicalized
  `\\?\` prefixes, mixed separators, Windows case drift). A file the
  root does not contain keeps its full stable spelling.
- `ripr_get_gap` and `ripr://gap/{canonical_id}` return one canonical
  item's complete bounded evidence bound to its snapshot identity:
  identity/location, changed behavior (expression, before/after, delta
  kind, probe family), causal attribution (canonical gap owner, behavior
  kind, probe kind, normalized discriminator), discriminator availability,
  related tests with oracle kind/strength, `related_tests_total` from the
  shared Finding accessor before the producer's retained-row cap, typed limitations for producer
  fields that are not established, and resource links. This slice pinned
  readiness as a hard `repair_packet_ready: false` negative; RIPR-SPEC-0214
  (slice C, #3090) owns the committed producer repair-readiness evaluation
  that replaced the pin, and it also owns binding the repair boundary and
  the repair-attempt link when a session transaction exists — until then
  the boundary stays `none_declared` and the link stays an explicit null. A
  missing field stays a typed state; MCP never fills it
  from prose. `ripr://snapshot/{snapshot_id}` returns bounded snapshot
  evidence: identity, typed outcome, the full item index, and the stored
  selection summary.
- `related_tests_total` is a required nonnegative integer in the gap item's
  advertised output schema. Legacy omitted/null metadata falls back to the
  retained row count, which is also the minimum for explicit metadata. The
  total participates in the existing evidence digest before snapshot identity
  is computed: identical eight-row windows with totals 12 and 13 must have
  distinct evidence and snapshot identities. Retained rows and repair-readiness
  decisions remain unchanged; one-row and empty findings disclose 1/1 and 0/0.
- Reads may bind an explicit `snapshot_id`; a mismatch fails closed with
  `stale_snapshot` and the current identity. Snapshot-dependent reads on
  a root resolved unavailable at startup fail closed with
  `workspace_unavailable` and the same restart-with-root recovery as
  `ripr_refresh`; the adapter does not discover or recheck the root again.
  A root resolved unavailable at startup also prints exactly one
  operator-facing warning line on stderr before serving, naming the root
  error code, its specific cause and a concrete next step (#9895; no path; stdout
  stays protocol-only, and a usable root stays silent). The write is
  fallible and ignored: an unwritable stderr never aborts startup, and the typed
  `workspace_unavailable` replies still arrive.
  On a usable root, reads before the first successful refresh fail closed
  with `no_snapshot`; reads during an
  attempt report `analysis_in_flight`; unknown item ids fail closed with
  `item_not_found`; a document whose complete tool envelope cannot fit the
  128-KiB response bound fails closed with `result_too_large` — for tool
  and resource responses the envelope is measured in its final form
  against the id-aware budget (the bound minus the actual JSON-escaped
  request id and the fixed JSON-RPC/`resultType` overhead, #7768) before
  it is sent (#6021), so neither the document-level guard nor a legal
  request id can downgrade a success into a response without
  `structuredContent`. Request ids whose serialized form exceeds 16384
  bytes are refused at receive with `-32600` and `"id": null` before
  dispatch, so the typed refusal always has room for its id; `MCP output
  limit` remains the fail-closed backstop.
  Reserved vocabulary
  (`config_invalid`, `workspace_ambiguous`, `static_limitation`,
  `cancelled`, `superseded`) is named on the wire for the slices that own
  those states.
- The read-only boundary (ADR 0022) holds for the whole surface: no source
  edit, no verification or mutation execution, no project-local
  configuration loading, no model provider, and no repair-ready claim
  anywhere. MCP does not call the LSP server, parse LSP diagnostics, or
  read VS Code artifacts as authority.

### Unknown-resource recovery (#7767)

An unrecognized resource URI returns a bounded, static human-readable message
naming `ripr://workspace/status` and all five advertised templates:
`ripr://snapshot/{snapshot_id}`, `ripr://gap/{canonical_id}`,
`ripr://repair-attempt/{attempt_id}`, `ripr://receipt/{receipt_id}`, and
`ripr://repair-card/{canonical_id}`. The latter repair routes retain the
identity and typed-failure authorities in RIPR-SPEC-0214 and RIPR-SPEC-0215.
The same routes remain in the existing `error.data.available` and
`error.data.resource_templates` arrays. A client displaying only
`error.message` can discover a next request without inspecting structured
error data. The message does not echo the unrecognized caller URI.

The SDK still maps resource misses to the negotiated error code; this adds
no protocol-version decision, schema field, route, or session mutation.
Recognized resource routes keep their existing missing/stale-identity
errors rather than receiving this generic route list. Listing a template
is not evidence that a resource with a particular identity exists.

### Resource URI trailing-slash tolerance (#9895)

Every id-bearing resource route — `ripr://snapshot/{snapshot_id}`,
`ripr://gap/{canonical_id}`, `ripr://repair-attempt/{attempt_id}`,
`ripr://receipt/{receipt_id}`, `ripr://repair-card/{canonical_id}`, and
the well-known `ripr://workspace/status` — normalizes exactly one trailing
slash before the id capture: `ripr://gap/{id}/` addresses the same item
as `ripr://gap/{id}`, and the read returns the identical document. Clients
— especially LLMs hand-assembling `ripr://` links — not uncommonly append
one slash. A second trailing slash is not a normalization artifact and
still misses: `ripr://gap/{id}//` fails closed instead of resolving to a
near-miss id, as does a slash that would leave the id empty
(`ripr://gap//`). The single-segment grammars (`snapshot`, `repair-attempt`,
`receipt`) stay strict inside the id, and this tolerance never turns an
unknown route into a recognized one.

## Required Evidence

- `cargo test -p ripr --lib mcp::workspace` — session state machine,
  snapshot identity portability, stale/no-snapshot/in-flight fail-closed
  reads, complete-zero vs incomplete-zero distinctness, last-known-good
  retention across a failed refresh, bounded failure detail, and the #6021
  paging contract: an oversized aggregate listing degrades to byte-fitted
  disclosed pages (walking `next_offset` covers the selection exactly
  once), while a fitting listing still ships whole with a closed window.
- `cargo test -p ripr --lib mcp::gaps` — canonical identity projection
  (producer gap id preferred, finding id fallback), the readiness block this
  slice pinned (the readiness evaluation itself is owned with
  RIPR-SPEC-0214), strict resource-URI parsing.
- `cargo test -p ripr --lib mcp` — descriptor contracts, positive
  LLM-facing tool descriptions, resource-template discovery, dispatch-edge
  argument rejection, and the #6021/#7768 envelope calibration: the tool
  envelope carries the document once, every success keeps
  `structuredContent`, and anything past the envelope budget fails closed
  with `result_too_large`.
- `crates/ripr/tests/mcp_sdk.rs` — the pinned official SDK client
  discovers the slice-B tools (four at that slice; the surface is seven
  tools and four templates after RIPR-SPEC-0214), the static resource, and
  the resource templates across the `initialize` and `server/discover`
  lifecycles, and the status tool/resource project the same session
  document.
- `crates/ripr/tests/mcp_stdio.rs` — raw-wire controls: tool/resource
  equality, rejection arms, and the new fail-closed control proving a
  stock-shaped client receives typed `no_snapshot` failures (and the
  resource-miss mapping) before the first refresh on a usable root.
  `unavailable_workspace_evidence_names_restart_before_any_refresh`
  exercises four snapshot tools and three evidence resources before and
  after refresh for markerless and missing explicit roots: each names
  `workspace_unavailable` and the actionable restart recovery, while
  status remains readable. Removing the shared snapshot availability
  guard restores the misleading `no_snapshot` recovery and fails it.
  `resource_reads_match_tools_after_refresh` pins gap tool/resource
  parsed-document equality plus the snapshot envelope
  (`ripr-mcp-snapshot-v1`, identity, typed outcome) after refresh
  (#7145), and the #9895 trailing-slash tolerance: one appended slash on
  the snapshot, gap, and repair-card URIs returns the identical document,
  while a second slash fails closed. Resource payloads are pretty-printed
  while tool payloads are compact by design, so the control compares
  parsed documents, not serialized bytes.
- `cargo test -p ripr --lib mcp::transport` — bounded framing, typed-shape
  recovery, and the #5267 control: repeated pre-initialize violations each
  receive a correlated `-32602` answer, the session then completes a normal
  `initialize` on the same connection, and stdin EOF ends it cleanly.
  Removing the handshake-retry recovery must fail that control.
- `cargo test -p ripr --lib lsp::diagnostic_budget` — the shared budget
  authority this slice consumes stays green.

## Non-Goals

- No repair-attempt creation, receipt status, or CommandSpec projection
  tools; #3090 owns them, and this slice's `ripr_get_gap` leaves their
  links as explicit nulls so #3090 can extend the document without a
  breaking change.
- No source edit or command execution. This slice pinned readiness as a
  hard negative; RIPR-SPEC-0214 (slice C, #3090) owns the committed
  producer repair-readiness evaluation that replaced that pin.
- No custom LSP request expansion; MCP and LSP remain peers over shared
  producers.
- No provider-specific configuration. Analysis configuration is honored
  from #6825 on: refresh resolves the workspace's own `ripr.toml` through
  `config::load_for_root` (the same resolution the CLI uses), the status
  and session profile disclose the posture (`loaded` with its
  `config_identity`, `built_in_defaults` when none resolves,
  `detected_not_loaded` when the file cannot be read or parsed), and an
  unparseable config fails the attempt closed with `config_invalid`.
- No support-tier promotion.
- No durable snapshot persistence; the session is in-memory and a server
  restart drops it (a fresh `ripr_refresh` rebuilds it).

## Acceptance Examples

1. A stock MCP client calls `ripr_workspace_status` (root + authority +
   session facts), `ripr_refresh` (one bounded analysis, terminal-state
   report), `ripr_list_gaps` (bounded working set with disclosed
   omissions), and `ripr_get_gap` (one complete item), or reads
   `ripr://workspace/status`, `ripr://snapshot/{snapshot_id}`, and
   `ripr://gap/{canonical_id}` — no report-file, clipboard, log, or
   LSP-protocol archaeology involved.
2. An unavailable startup root makes snapshot-dependent reads fail closed
   with `workspace_unavailable` and the restart-with-root recovery before
   any refresh. On a usable root, pre-refresh list/get/snapshot reads fail
   closed with typed `no_snapshot`; during an attempt they report
   `analysis_in_flight`; a
   named snapshot that is not current fails with `stale_snapshot` naming
   the current identity.
3. A refresh whose outcome is `unsupported_input` or `analysis_failed`
   never commits a snapshot; the previous last-known-good snapshot remains
   readable; a `partial_with_limitations` outcome commits with its typed
   limitations and stays distinct from `complete_no_findings`.
4. Two equivalent roots at equivalent inputs produce the same
   `snapshot:sha256:` identity, while each server's status document keeps
   its own host-local root hash.
5. `ripr_get_gap` for any item reports the committed producer
   repair-readiness facts (the evaluation is owned by RIPR-SPEC-0214, slice
   C #3090, which superseded this slice's hard `repair_packet_ready: false`
   pin), a `none_declared` repair boundary, and an explicit-null
   repair-attempt link until a session transaction binds one; missing
   producer fields remain typed states.

6. After a client reads an unknown URI, the correlated error names the
   status URI and every template in both prose and structured arrays. A
   recognized snapshot read before refresh still reports `no_snapshot`,
   and the same connection can subsequently read the valid status resource.

## Test Mapping

- `crates/ripr/src/mcp/workspace.rs::tests::omission_disclosure_pages_independently_of_items`
  — with more omitted identities than one window, each response carries at
  most 200, `omitted_page`/`continuation.next_omitted_page` walk every
  omission exactly once, and a past-end window is empty and closed (#9360).
  `long_omitted_identities_are_byte_fitted_and_fully_walked` and
  `continuation_routes_preserve_the_other_axis` pin the byte budget and the
  two-axis replay.
- `crates/ripr/src/mcp/workspace.rs::tests::status_serves_per_language_completeness_from_the_view`
  and `status_without_a_view_keeps_the_unchanged_shape` — literal
  `language_completeness` rows and absence without a view.
  `every_emitted_session_key_is_declared_by_the_status_schema` is a
  structural schema-alignment check only (emitted keys are declared, the
  property is optional, status-enum membership), not a full JSON-Schema
  validation.
- `crates/ripr/tests/mcp_stdio.rs::refreshed_gap_filters_narrow_real_stdio_and_replay_bound_continuation`
  — separate real-refresh evidence: both the status tool and the
  `ripr://workspace/status` resource serve the producer's rows (rust, plus
  python when compiled and enabled). Dropping the commit-time view binding in
  `run_check` fails it, for both feature profiles.
- `crates/ripr/tests/mcp_stdio.rs::rejection_arms_survive_the_stdio_transport`
  — literal complete recovery prose and arrays, retained recognized-route
  refusal, then valid status recovery on the same session.
- `crates/ripr/tests/mcp_sdk.rs::official_sdk_legacy_client_lists_calls_and_reads_status`
  and `official_sdk_discovery_client_lists_calls_and_reads_status` — the
  same literal recovery contract through the official SDK lifecycles,
  preserving the SDK-owned negotiated resource-miss code.

- `crates/ripr/tests/mcp_stdio.rs::unusable_root_warns_once_on_stderr_and_keeps_stdout_clean`
  — nonexistent, file and non-git roots each print exactly one stderr warning
  with empty stdout; a usable root stays silent. The unavailable-root session
  tests (`unavailable_workspace_evidence_names_restart_before_any_refresh`,
  `current_discovery_requires_metadata_and_rejects_legacy_ping`) assert stderr
  is exactly that one warning while keeping every protocol assertion.
- `crates/ripr/tests/mcp_stdio.rs::unwritable_stderr_does_not_abort_unusable_root_session`
  — stderr failing every write (`/dev/full`), unusable root: startup survives and typed
  `workspace_unavailable` replies still arrive over stdout.
- `crates/ripr/tests/mcp_stdio.rs::refreshed_gap_filters_narrow_real_stdio_and_replay_bound_continuation`
  — actual committed multi-item analysis over raw stdio: literal IDs, counts,
  class/language/readiness exclusion, conjunction, known-value empty sets,
  filtered one-item continuation replay, unchanged snapshot after reads, and
  stale replay after replacement. The frozen pre-feature closed v1 success
  schema rejects the extended envelope, which carries v2. A dropped-forwarding
  experiment must fail this oracle after argument validation.
- `crates/ripr/src/mcp/workspace.rs::tests::gap_list_summaries_project_readiness_matching_get_gap`
  + `gap_list_filters_narrow_each_dimension_then_combine_before_paging`
  — direct stored-fact narrowing/readiness and continuation controls.
- `crates/ripr/src/mcp/server_tests.rs::list_gaps_accepts_filters_at_the_dispatch_edge`
  — argument admission before refresh; this is not successful narrowing proof.
- `crates/ripr/tests/mcp_stdio.rs::list_gaps_rejects_unknown_class_tokens_with_the_vocabulary`
  + `list_gaps_bounds_the_echoed_unknown_class_token` — unknown class is
  `-32602` naming the vocabulary; valid classes after a real refresh return
  exactly their filtered gaps; near-cap, escape-heavy and multibyte tokens
  yield a bounded, id-correlated `-32602`, not `-32603`.
- `crates/ripr/tests/mcp_stdio.rs::empty_gap_pages_keep_structured_outcomes_at_the_response_bound`
  — real committed analysis with nonempty subjects before filtering, fitting
  empty pages, independently sized structured-envelope overflow below the
  raw-document ceiling, raw-document overflow, the `file` and `language` filters,
  typed refusals bound to the current snapshot, echo-driven refusals naming
  the filter rather than the snapshot identity route, and same-session recovery.
- `crates/ripr/src/mcp/workspace.rs::tests::list_gaps_overflow_recovery_names_the_shrinkable_cause`
  — filter-driven shell overflow names the filter; shell-driven overflow
  keeps the snapshot identity route.
- `crates/ripr/tests/mcp_stdio.rs::oversized_request_id_is_rejected_with_null_id_and_the_session_survives`
  — a request id whose serialized form exceeds 16384 bytes (including one
  that fits the input cap but not the output cap) is answered with a
  bounded `-32600` frame carrying `"id": null`, an id exactly at the bound
  and a following ping are still answered, and the process stays alive.
  The #7768 id-aware envelope budget remains pinned by the in-crate unit
  tests below; its wire stimulus (ids near the response cap) is no longer
  admitted.
- `crates/ripr/src/mcp/server_tests.rs::window_sized_gap_item_refuses_typed_instead_of_dropping_structured_content`
  — the same window at the real `ripr_get_gap` dispatch with a
  document-guard-approved item, asserting the serialized wire shape
  (`isError` plus structured `failure.code`), not a Debug rendering.

- `crates/ripr/src/mcp/workspace.rs::tests` — session lifecycle, typed
  failures, identity portability, boundedness, last-known-good retention;
  `session_profile_resolves_the_workspace_configuration` pins the #6825
  config postures (loaded with identity, built-in defaults,
  detected-not-loaded) at the profile level.
- `crates/ripr/src/workspace_status.rs::tests::dangling_ripr_toml_symlink_is_present_not_built_in_defaults`
  — a dangling `ripr.toml` is present, not built-in defaults.
- `crates/ripr/tests/mcp_workspace_config.rs` — the #6825 wire contract: a
  python-enabled workspace scores its finding through
  refresh/list/get_gap with the `loaded` posture, a config-less rust
  workspace keeps the defaults posture, and an unparseable `ripr.toml`
  fails the refresh closed with `config_invalid`.
- `crates/ripr/src/mcp/gaps.rs::tests` — canonical item projection and the
  readiness block this slice pinned (the readiness evaluation itself is
  owned with RIPR-SPEC-0214).
- `crates/ripr/src/mcp/protocol.rs::tests::tool_result_carries_the_document_once_when_the_double_envelope_overflows`
  + `crates/ripr/src/mcp/server_tests.rs::list_gaps_accepts_offset_and_limit_at_the_dispatch_edge`
  — the #6021 envelope calibration and paging dispatch contract; the
  first also pins the #7768 refusal window (a document the document guard
  approves whose double-carry envelope cannot fit fails closed instead of
  shipping text-only).
- `crates/ripr/tests/mcp_workspace_config.rs::list_gaps_pages_over_the_wire_with_offset_and_limit`
  — the #6021 wire contract: `offset`/`limit` accepted, the window
  disclosed, an over-selection offset an empty final page.
- `crates/ripr/tests/mcp_stdio.rs::optional_tool_arguments_reject_null_and_recover_over_stdio`
  — the #7314 schema/dispatch contract for all six optional argument
  positions, with omission, valid-type, wrong-type and recovery controls.
- `crates/ripr/tests/mcp_interrupts.rs::cooperative_interrupt_lifecycle`
  (Linux) — real stdio and gated git children cover SIGINT, SIGTERM,
  early-primary pipe holders, typed failure, last-good retention, same-session
  recovery and an independently owned surviving sentinel. Identity-pinned
  process handles and adopted descendant wait statuses distinguish product
  settlement from harness cleanup.
- `crates/ripr/src/cli/interrupt.rs::tests` — isolated native signal controls,
  concurrent installation, repeated mixed/TERM escape, terminal signal epochs
  and bounded adverse harness controls.
- `crates/ripr/src/process_owner.rs::tests` — non-reaping cleanup ordering,
  successful output/status, refusal after custody loss, repeated cleanup and
  preservation of caller process groups.
- `crates/ripr/tests/mcp_stdio.rs::catalog_list_cursors_fail_closed_over_stdio`
  — a `cursor` on `tools/list`, `prompts/list`, `resources/list`, or
  `resources/templates/list` is a `-32602` whose message explains the list
  never paginates and whose structured data names `cursor` and the literal
  omission recovery;
  string and malformed non-null inputs fail in both protocol modes, while
  null/omitted inputs retain their defaults. The same complete lists
  still answer normally without one, on the same session (#9895).
- `crates/ripr/tests/mcp_stdio.rs::malformed_catalog_cursors_preserve_protocol_validation_and_recover`
  — malformed cursor refusal retains SDK metadata/protocol precedence, bounded
  request-ID refusal, unknown-method routing and same-session recovery.
- `crates/ripr/src/mcp/protocol.rs::tests` + `server_tests.rs` —
  descriptor and dispatch contracts.
- `crates/ripr/tests/mcp_sdk.rs`, `crates/ripr/tests/mcp_stdio.rs` —
  hosted wire interop controls, including
  `post_initialize_ping_with_handshake_meta_returns_an_empty_result`
  (empty-params ping is not sufficient proof).

## Implementation Mapping

| Surface | Responsibility |
| --- | --- |
| `crates/ripr/src/mcp/workspace.rs` | session state machine, snapshot binding, typed failures, bounded documents, `run_check` bridge to `app::check_workspace` |
| `crates/ripr/src/mcp/gaps.rs` | canonical item projection, budget items, resource-URI parsing |
| `crates/ripr/src/mcp/protocol.rs` | tool/resource descriptors, schemas, instructions, status document |
| `crates/ripr/src/mcp/server.rs` | SDK adapter dispatch, initialize-session ping-by-method, refresh single-flight, resource reads |
| `crates/ripr/src/mcp/transport.rs` + `workspace_status.rs` | retained canonical root for in-process refresh (never serialized) |
| `docs/interop/mcp.md`, `docs/adr/0022` (Slice B note) | operator-facing surface and authority boundary record |
| `docs/specs/README.md` + `.ripr/traceability.toml` | spec registration and test traceability |

## Metrics

- `mcp_tools` (4) and `mcp_resource_templates` (2) on the slice surface
- refreshed filter control subjects: five with Python, four in Rust-only builds;
  every positive narrowing asserts a nonempty fixture and literal IDs
- typed pre-refresh failure controls on the wire (target: 3 — list, get,
  snapshot resource)
- zero committed snapshots for `unsupported_input` / `analysis_failed`
  outcomes (negative invariant)

## Source-preserving recovery follow-up (#10799)

Single-gap overflow recovery is captured with its evidence snapshot and reused
at the raw-document, tool-envelope and resource-envelope boundaries. The CLI
renderer binds the captured root, effective base and explicit source; a newer
failed request cannot relabel it. The route is a bounded current-state rerun,
not immutable replay. The 4096-byte recovery-text budget refuses an oversized complete route
with an explicit non-command limitation rather than truncating executable text.

For failed saved-worktree refresh, MCP retry examples retain the requested
worktree source when the operator corrects or omits a base. Discovery states
the committed default separately from opt-in saved tracked edits. Unsaved and
untracked exclusions remain unchanged. Worktree cards and readiness follow the
producer-custody rules above: offered only with an established agreement,
withheld with the cause otherwise.
