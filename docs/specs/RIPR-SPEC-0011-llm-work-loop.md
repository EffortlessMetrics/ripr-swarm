# RIPR-SPEC-0011: LLM Work Loop

Status: proposed

Lifecycle review: 2026-08-25. Retained as proposed; acceptance remains blocked
on the complete work-loop artifact provenance, command-template, and boundary contract.

## Problem

Campaign 10 made the editor-agent evidence loop real: saved-workspace
diagnostics can hand users to agent packet, brief, verify, receipt, cockpit,
and CI artifacts. That loop is still easy for an LLM agent to misuse because
the agent has to infer state from files and remember the correct command
sequence.

The LLM work loop adds a control plane around the existing artifacts. It should
answer where the agent is in the workflow, what is missing, which seam links the
artifacts, and which command should run next.

## Behavior

`ripr agent status --root . --json` reads existing artifacts only:

```text
target/ripr/workflow/before.repo-exposure.json
target/ripr/workflow/after.repo-exposure.json
target/ripr/workflow/agent-brief.json
target/ripr/workflow/agent-packet.json
target/ripr/workflow/agent-verify.json
target/ripr/reports/agent-receipt.json
```

It must not run analysis, generate tests, edit source files, run mutation
testing, refresh LSP state, or change the schemas for brief, packet, verify, or
receipt.

The status report should:

- report each required artifact as present or missing;
- recover `seam_id` from receipt, verify, packet, or brief JSON when possible;
- emit a next command for every missing artifact;
- select `next_command` by the repair-attempt-first order below (#3906), which
  falls back to the first missing command only when no repair attempt applies;
- warn when timestamps suggest `agent verify` is older than a before/after
  snapshot or `agent receipt` is older than `agent verify`;
- keep all language advisory and static.

`--out` names the directory status reads, and only the default workflow
directory is accepted: spellings that are lexically equivalent to
`target/ripr/workflow` (for example `./target/ripr/workflow`, a trailing
slash, or a `..` that pops only real directories) run exactly like the default
spelling, while any genuinely different directory is refused with exit 2 and
`agent status --out <dir> is not supported` on stderr instead of being
silently ignored (#10542). Lexical equivalence is not trusted when a `..`
follows a symlink: the filesystem resolves `..` after the symlink target, so
such a spelling is refused even when lexical cleaning would make it match
(#10624 review).

General inventory continuation and missing-artifact commands bind the selected
root from its native filesystem spelling. UTF-8 Unix directory names containing
a literal backslash must select that directory when pasted from another working
directory, including awaiting/after, failed/open-gap restart and missing-receipt
commands. Windows retains separator presentation. Report-only display text is
separate from this command identity (#6313).

If receipt issuance cannot find the default workflow verify artifact, its
producer hint uses the native canonical selected root and preserves the
requested relative or absolute output locator. Pasting the hint from another
directory must write the artifact that the refused receipt selected, including
when a UTF-8 Unix directory name contains a literal backslash. A custom missing
verify path has no known snapshot pair and emits no workflow producer hint.
The refusal and receipt evidence boundaries remain unchanged (#6684).

With an absolute selected root that has no literal Unix backslash in its name,
`agent start --out` preserves a custom workflow directory's native UTF-8 Unix
filename characters in manifest locators,
artifact inventory and generated commands. Both relative and absolute `--out`
inputs must address that same physical directory when the emitted snapshot,
verify, receipt and regeneration commands run from another working directory.
A literal backslash or shell quote must not select a slash-normalized decoy.
Windows retains separator presentation. This does not qualify arbitrary
relative-root invocations, workflow root spellings containing a literal Unix
backslash, or change missing custom-input recovery (#6809).

The loop command templates are centralized in one internal module before the
workflow manifest is introduced. That module owns the current workflow artifact
paths, the editor/CI pilot-agent artifact paths, and the command builders for:

```text
ripr agent start
ripr check --format repo-exposure-json
ripr check --format agent-seam-packets-json
ripr agent packet
ripr agent brief
ripr agent verify
ripr agent receipt
ripr agent status
ripr agent review-summary
ripr outcome
```

Current consumers must preserve their existing emitted command text while
sharing these builders where they construct command payloads or missing-input
commands.

`ripr agent start --root . --seam-id <id> --out target/ripr/workflow` writes a
source-edit-free workflow packet for one visible seam:

```text
target/ripr/workflow/workflow.json
target/ripr/workflow/commands.md
target/ripr/workflow/agent-brief.json
```

The command may run the same static seam selection used by
`ripr agent brief --seam-id`, because the manifest needs the selected seam's
missing discriminator, suggested assertion shape, related-test target, and
effective mode. It must not edit source files, generate tests, run mutation
testing, call LLM APIs, refresh LSP state, configure CI blocking, or add
vendor-specific prompt/model behavior. The packet is deterministic context for
humans and external agents.

`ripr agent receipt --root . --verify-json <agent-verify-json> --seam-id <id>
--json` records provenance for the static artifacts behind the selected seam
receipt. The receipt renderer still reads existing artifacts only. It must not
run analysis, edit source files, generate tests, run mutation testing, call LLM
APIs, refresh LSP state, or claim runtime adequacy.

Receipt provenance records:

- `ripr_version`;
- `repo_root`;
- `config_fingerprint` when `ripr.toml` exists and can be read without running
  analysis; a present but unreadable `ripr.toml` (including a dangling
  symlink) is an error, not a missing fingerprint;
- `command_template_version`;
- `generated_at`;
- before, after, and verify artifact paths plus SHA-256 hashes;
- selected `seam_id`, before class, after class, and movement;
- explicit static boundary flags.

Receipt `provenance.repo_root`, before/after/verify artifact paths and
`inputs.agent_verify_json` preserve native UTF-8 Unix filename characters for
later reopening. Fields that omit one leading `./` keep that omission;
before/after provenance paths retain their existing leading `./` behavior.
Canonical root equality, containment, content hashes and currentness admission
remain mandatory. A different slash-path checkout cannot inherit the receipt's
authority (#6313).

`ripr agent review-summary --root .` reads existing artifacts and emits a
compact Markdown packet for PR review. `--json` emits the schema `0.1` JSON
contract. The command joins agent status, workflow, receipt, operator cockpit,
repo exposure, LSP cockpit when present, and local CI artifact file state. It
must not run analysis, edit source files, generate tests, run mutation testing,
call LLM APIs, refresh LSP state, or query GitHub Actions.

The review summary should answer:

- which seam was targeted;
- what static before/after movement the receipt records;
- which receipt and verify artifacts carry the evidence;
- which joined surfaces are present, missing, optional-missing, or malformed;
- what command should run next when the loop is incomplete;
- what the reviewer should inspect;
- which static limits remain.

The executable status command embedded in a review-summary surface uses the
native selected root for both its `--root` argument and anchored redirect.
It shares command authority with the top-level continuation even though the
report's `root` field and analysis-outcome presentation key retain display
formatting. Unix literal filename backslashes remain distinct from Windows
separators (#6313).

### Repair phase value recovery (#5059)

`ripr agent repair --phase` accepts only `before`, `after`, and `verify`.
An unknown value uses the shared CLI value-suggestion rules: `befor`, `aftr`,
and `verfy` suggest their corresponding phase; empty and distant values,
including the sibling command name `packet`, list the phases without a suggestion. Suggestions never accept
or execute the rejected spelling. Invalid occurrences, including one followed
by a valid repeated flag, exit 2 with empty stdout and the refusal on stderr
before configuration loading or repair/store writes. A missing value retains
its missing-value refusal. Canonical phase meanings and the existing `--help`
short circuit remain unchanged.

### Repeated value flags refuse (#10542)

Every `ripr agent` value flag is single-use. A second occurrence of the same
value flag is a usage error, refused with exit 2, empty stdout, and
`<command> takes <flag> once` on stderr before any artifact or state write.
For example, `ripr agent brief --seam-id s --max-seams 1 --max-seams 10`
refuses with `agent brief takes --max-seams once` instead of silently keeping
the last cap, and `ripr agent verify --before a --before b --after c --json`
refuses with `agent verify takes --before once` instead of comparing `b`
against itself. Two deliberate exceptions keep their family contracts:
`--root` absorbs an identical repeated value so scripted double-root
invocations keep working; a conflicting repeated value refuses with exit 2
and names both roots before execution (#9008, #13326). The first explicit
`--root` still replaces the implicit default. `agent receipt --command` stays
a repeatable accumulator. An empty `agent verify --gap-id` is refused at
the parser with the same exit-2 usage-error shape, naming the empty flag
instead of flowing the empty id into the artifact layer.

## JSON Shape

The status report uses schema version `0.1`:

```json
{
  "schema_version": "0.1",
  "tool": "ripr",
  "status": "incomplete",
  "root": ".",
  "seam": {
    "seam_id": "67fc764ba37d77bd",
    "source": "agent_receipt"
  },
  "artifacts": [
    {
      "name": "before_snapshot",
      "label": "before snapshot",
      "path": "target/ripr/workflow/before.repo-exposure.json",
      "required": true,
      "state": "present",
      "bytes": 12000,
      "modified_unix_ms": 1778179200000
    }
  ],
  "missing_commands": [
    {
      "step": "agent_packet",
      "artifact": "target/ripr/workflow/agent-packet.json",
      "reason": "agent packet artifact is missing",
      "command": "ripr agent packet --root . --seam-id 67fc764ba37d77bd --json > target/ripr/workflow/agent-packet.json"
    }
  ],
  "next_command": {
    "step": "agent_packet",
    "artifact": "target/ripr/workflow/agent-packet.json",
    "reason": "agent packet artifact is missing",
    "command": "ripr agent packet --root . --seam-id 67fc764ba37d77bd --json > target/ripr/workflow/agent-packet.json"
  },
  "test_run": null,
  "warnings": []
}
```

The workflow manifest uses schema version `0.1`:

```json
{
  "schema_version": "0.1",
  "tool": "ripr",
  "status": "ready",
  "root": ".",
  "mode": "draft",
  "out_dir": "target/ripr/workflow",
  "seam": {
    "seam_id": "67fc764ba37d77bd",
    "file": "src/pricing.rs",
    "line": 88,
    "seam_kind": "predicate_boundary",
    "grip_class": "weakly_gripped",
    "why": "caller requested seam_id 67fc764ba37d77bd",
    "missing_discriminator": "amount == discount_threshold",
    "assertion_shape": "assert_eq!(...)",
    "recommended_test_file": "tests/pricing.rs",
    "recommended_test_name": "discount_threshold_equality_boundary_is_asserted",
    "related_test_to_imitate": "applies_discount_above_threshold"
  },
  "outputs": {
    "workflow_manifest": "target/ripr/workflow/workflow.json",
    "commands_markdown": "target/ripr/workflow/commands.md",
    "agent_brief": "target/ripr/workflow/agent-brief.json"
  },
  "artifacts": [],
  "commands": [],
  "missing_inputs": [],
  "next_command": null,
  "boundaries": {
    "source_edits": false,
    "generated_tests": false,
    "runtime_mutation_execution": false,
    "llm_api_calls": false,
    "ci_blocking": false
  }
}
```

The agent receipt uses schema version `0.3` once provenance and structured
next-action guidance are present:

```json
{
  "schema_version": "0.3",
  "tool": "ripr",
  "status": "advisory",
  "inputs": {
    "agent_verify_json": "target/ripr/workflow/agent-verify.json",
    "before": "target/ripr/workflow/before.repo-exposure.json",
    "after": "target/ripr/workflow/after.repo-exposure.json"
  },
  "provenance": {
    "ripr_version": "0.7.0",
    "repo_root": ".",
    "config_fingerprint": "fnv1a64:4c94a2f6cfaa5c21",
    "command_template_version": "0.1",
    "generated_at": "unix_ms:1778179200000",
    "workflow_artifact": null,
    "before_artifact": {
      "path": "target/ripr/workflow/before.repo-exposure.json",
      "sha256": "sha256:..."
    },
    "after_artifact": {
      "path": "target/ripr/workflow/after.repo-exposure.json",
      "sha256": "sha256:..."
    },
    "verify_artifact": {
      "path": "target/ripr/workflow/agent-verify.json",
      "sha256": "sha256:..."
    },
    "seam_id": "67fc764ba37d77bd",
    "before_class": "weakly_gripped",
    "after_class": "strongly_gripped",
    "movement": "improved",
    "limits": {
      "static_artifact_relationship": true,
      "runtime_mutation_execution": false,
      "runtime_adequacy_claim": false
    }
  },
  "summary": {
    "remaining_gap": "No remaining static gap is named by this receipt; inspect the current seam packet if review needs final assertion detail.",
    "next_recommendation": "Run the focused test with the project's test command and keep it only if it passes; ripr compared static evidence and did not run it. Then attach this receipt with the agent verify JSON.",
    "next_action": {
      "kind": "improved",
      "summary": "Static grip improved.",
      "recommended_action": "Run the focused test and keep it only if it passes; ripr did not run it. Then include this receipt in review.",
      "safe_to_merge": false
    }
  }
}
```

Receipt next-action guidance stays static and bounded. It derives only from the
selected seam movement in the saved `agent verify` JSON:

| Movement | `next_action.kind` | Guidance |
| --- | --- | --- |
| `improved` | `improved` | Run the focused test and keep it only if it passes; ripr did not run it. Then include the receipt in review. |
| `changed` | `changed` | Inspect the evidence delta and strengthen the discriminator named by the packet. |
| `regressed` | `regressed` | Revisit the test or code change before merge. |
| `unchanged` | `unchanged` | Add the missing discriminator or stronger assertion named by the packet. |
| `new` | `new_gap` | Generate a fresh packet or brief for the seam. |
| `resolved` | `resolved` | Confirm the seam disappeared intentionally before relying on the receipt. |

`safe_to_merge` remains `false` for every static receipt. The receipt is
review evidence, not a merge policy or runtime adequacy claim.

The agent review summary uses schema version `0.1`:

```json
{
  "schema_version": "0.1",
  "tool": "ripr",
  "status": "ready",
  "root": ".",
  "target_seam": {
    "seam_id": "67fc764ba37d77bd",
    "source": "agent_receipt",
    "file": "src/lib.rs",
    "line": 42,
    "seam_kind": "predicate_boundary"
  },
  "static_movement": {
    "state": "improved",
    "before_class": "weakly_gripped",
    "after_class": "strongly_gripped",
    "grip_class": "strongly_gripped",
    "evidence_artifact": "target/ripr/reports/agent-receipt.json",
    "verify_artifact": "target/ripr/workflow/agent-verify.json",
    "summary": "Static movement is improved (weakly_gripped -> strongly_gripped).",
    "next_action": {
      "kind": "improved",
      "summary": "Static grip improved.",
      "recommended_action": "Run the focused test and keep it only if it passes; ripr did not run it. Then include this receipt in review."
    }
  },
  "next_command": null,
  "surfaces": [],
  "ci_artifacts": [],
  "reviewer_summary": {
    "headline": "Review packet is ready for seam 67fc764ba37d77bd.",
    "what_changed": "Static movement is improved (weakly_gripped -> strongly_gripped).",
    "evidence": "Review target/ripr/reports/agent-receipt.json with target/ripr/workflow/agent-verify.json.",
    "remaining": "Run the focused test and keep it only if it passes; ripr did not run it. Then include this receipt in review.",
    "reviewer_should_inspect": [
      "target/ripr/reports/agent-receipt.json",
      "target/ripr/workflow/agent-verify.json"
    ]
  },
  "limits": {
    "static_artifact_relationship": true,
    "runtime_mutation_execution": false,
    "automatic_edits": false,
    "generated_tests": false
  }
}
```

## Amendment (#3906): next command selection

The legacy rule "first missing artifact" sent a user with a prepared repair
attempt back into the manual snapshot loop, sent a fresh workspace to a Bash
redirect into `target/ripr/workflow/` before that directory existed, and sent
a failed attempt to `ripr agent receipt`. `ripr agent status` now also reads
`target/ripr/repair-attempts/*/attempt.json` through the attempt authority
(`app::repair_attempt::inventory_repair_attempts`, the same validation the
after phase applies) and selects `next_command` in this order:

1. **Unreadable inventory.** If any attempt manifest fails validation, select
   nothing and warn `repair_attempt_unreadable`, naming the attempt.
2. **Resume.** Exactly one attempt in `awaiting_edit` whose after phase would
   evaluate the current `HEAD` as the attempt's: its recorded `next_command`
   (`--attempt <id> --phase after`), step `repair_attempt_after`. Status asks
   the attempt authority's head rule
   (`app::repair_attempt::after_phase_head_admission`), the same one the after
   phase applies: the prepared head, or for an ordinary attempt a commit that
   descends from it (a focused test committed between the phases). A
   trust-bound attempt needs the exact prepared head.
3. **Ambiguous.** More than one such attempt: select nothing and warn
   `ambiguous_repair_attempts`, listing each attempt's command. Status never
   picks the newest (docs/REPAIR_ATTEMPT.md).
4. **Head unknown.** `HEAD` cannot be read, so rules 2 and 3 match no
   attempt. If any attempt is `awaiting_edit`, select nothing and warn
   `repair_attempt_head_unknown` rather than restarting its seam.
5. **Restart.** Group attempts by seam. A seam with a `ready_to_finish`
   attempt is finished. A seam whose attempts all ended (`failed`, `stale`,
   `incomparable`), were prepared at another `HEAD`, or never left `prepared`
   is open. (Refined below: a `ready_to_finish` attempt finishes its seam only
   through the receipt issued for it.) An ordinary attempt "prepared at
   another `HEAD`" is one whose `HEAD` no longer descends from its prepared
   head; its reason repeats the after phase's recovery (`git reset --soft
   <prepared-head>` then the attempt's after command) before the new-attempt
   route. Exactly one open seam: `ripr agent repair --root <root> --seam-id
   <seam> --phase before`, step `repair_attempt_before`. Several: select
   nothing and warn `multiple_open_repair_seams`, listing each start command.
6. **Legacy loop.** Otherwise the first missing artifact's command, with two
   refusals. With no known seam the next command is `ripr pilot --root
   <root>` (step `select_seam`), never a `<seam-id>` placeholder, except
   that a complete pilot summary whose top seam recorded no repair start
   (`next.repair_command: null`) selects nothing and warns
   `pilot_found_no_repair_target`: rerunning pilot would rank the same seam
   again, so the warning names the hand step (a test in the seam's own
   crate) instead. A complete pilot summary that ranked no seam because it
   withheld every seam as a static limitation (`opaque` or `*_unknown`,
   `withheld_static_limitations_total` > 0) and recorded no repair start
   selects nothing and warns
   `pilot_withheld_static_limitations_no_repair_target`, naming the repo
   exposure report where each withheld seam names its unresolved stage
   (#5497). When that run's repo exposure report says a seam limit applied,
   the warning names raising the limit and rerunning pilot instead, since
   seams past the cut may hold gaps. A complete pilot summary that ranked and
   withheld no seam, recorded no repair start and carries
   `declined_rust_files` (a parse limit declined Rust files, #9308) selects
   nothing and warns `pilot_declined_rust_files_no_repair_target`, naming the
   declined file(s) and the parse-limit repair (split below the Rust parse
   budget or save as UTF-8, then rerun pilot with the selected root); a
   recorded `seam_limit` adds the raise-the-limit remedy. With withheld
   seams the withheld warning above keeps precedence and already names the
   declined files (#9371). Likewise a complete pilot summary that ranked no seam,
   recorded no repair card (`python_first_use` absent, `null`, or status
   `no_python_findings` or `no_repair_cards`; `analysis_unavailable` is not
   this fact), routed the code to `ripr check` (`language_routes.state:
   required` with a recorded route command) and recorded no repair start
   selects nothing and warns `pilot_routed_to_check_no_repair_target`, for
   any routed language. For an enabled route the warning names the recorded
   check command; for a route with `enabled: false` it names the enable step
   (add the language to `[languages] enabled` in `ripr.toml`) instead. It
   also says to rerun pilot if the workspace changed since that run. For an
   enabled TypeScript-family route, the warning uses the same gap-start
   admission as the routed `ripr check` (#10001): when `ripr check` would print
   a gap-bound start, the warning names that start and offers adding or
   strengthening a test by hand, then rerunning the check, as an alternative.
   For a refused or undecidable admission, it reports "status could not
   confirm a gap-bound start" and directs the user to the routed check for
   the current finding and any reason a start is unavailable, without claiming
   that no start exists.
   A timed-out, missing or unreadable summary still routes to pilot. When the
   first missing artifact's directory does not exist and the seam is known,
   the next command starts a repair attempt, which writes the workflow
   artifacts itself, instead of redirecting into the missing directory.

`status` is `incomplete` whenever a next command is selected. The JSON report
adds `repair_attempts` (`attempt_id`, `seam_id`, `state`, `head_current`,
`disposition`, `manifest`, `command`); `missing_commands` keeps its legacy
meaning. `ripr agent review-summary` carries the same `next_command`.

### Refinement: the attempt's outcome, not only its state

`ready_to_finish` means the edit cage admitted the edit; it does not mean the
gap closed. Status reads the workflow receipt against each `ready_to_finish`
attempt, matching the receipt's `repair_attempt` binding (attempt ID, after
`HEAD`, delta and packet digests) to the attempt's after verdict, and
interprets it through the receipt owner (`output::agent_receipt`):

- `advisory` with movement `improved`: `finished`. Status words it as static
  grip improved (receipt advisory), not as a closed gap. The loop is
  `complete` only when the head and worktree binding still match the after
  phase (the after head may descend from the prepared head). A stale reading
  produces a `repair_receipt_stale` warning.
- movement `unchanged`, `changed`, or `regressed`, whatever the receipt
  status: `gap_open`. The seam is open, and rule 5 starts a new attempt whose
  reason names the receipt status, movement, staleness, and the receipt's own
  next action.
- anything else (an `invalid` or `incomplete` improved receipt, a new or
  resolved seam, or no receipt issued for the attempt): `unconfirmed`. The seam
  is not restarted and a `repair_receipt_unconfirmed` warning says why.
- the attempt retained a terminal receipt: status reads that attempt-local
  artifact (hashes, path, and after-verdict binding) even when the
  compatibility file now holds another attempt's receipt. Replacing, deleting,
  or corrupting `target/ripr/reports/agent-receipt.json` does not change the
  retained reading.
- a declared attempt-local receipt is missing, digest-mismatched, path-escaped,
  or bound to a different after verdict: `receipt.unavailable` is true and the
  reading is `unconfirmed`. Status does not fall back to another attempt's
  compatibility receipt.
- a legacy manifest with no `terminal_artifacts` whose workflow receipt is bound
  to another attempt: the workflow keeps one compatibility receipt, so a later
  attempt's after phase superseded this attempt's. `receipt.superseded_by` names
  that attempt; the reading is `unconfirmed`, because the earlier outcome cannot
  be reconstructed, and the warning names the new-attempt command for the seam
  in case its gap is still open.

One `agent status` call takes at most one successful worktree observation per
(root, whole edit-cage policy) and reuses it for every finished attempt it
reevaluates, so equal-policy attempts do not each rescan the worktree (#9823).
A capture that fails is not retained. The observation is call-scoped: the next
status call captures again, and an edit made between two readings inside one call
is retained outside that call's shared observation until the next call. Each
attempt keeps its own baseline, delta and verdict authority; receipt issuance,
the after phase and MCP reads capture afresh.

The finished attempt's delta commitment includes the captured post-change
index and worktree identities of changed paths, including non-owned paths
inside operational directories. The selected attempt manifest's identity is
omitted to avoid hashing the digest into itself; its path and kind remain bound,
and tracked or authored paths do not receive that exception. The producer also
omits newly added, untracked workflow backup copies only when this invocation
exclusively created them and their captured bytes and executable state still
match its receipt. Baseline, authored, forbidden, and non-operational paths
never receive this backup exception. Cleanup of these exact temporary copies
must not invalidate the just-finished attempt; another attempt's bound manifest
remains identity-sensitive. One bound compatibility receipt may be replaced
by the next issued receipt without staling that producer: an optional
`receipt_projection_predecessor` records only a different, validated issued
attempt in the same store, plus the captured path kind and executable state.
The read-only currentness owner validates both retained terminal pairs and the
exact live replacement bytes, reconstructs only this fixed receipt slot, and
requires the original whole-delta digest and verdict. Unowned or modified
bytes, missing predecessor evidence, tracked/authored/forbidden paths and
neighbor or sibling drift receive no exception. The predecessor can remain
historical; this does not recurse through or grant it currentness. Legacy
attempts without this optional witness retain their existing behavior. Neither
this witness nor identity binding alone authorizes reconstructing missing gap
projections or ignoring present projection drift.
The same HEAD and the same changed path and kind do not establish
the same bytes: changing an already-bound boundary assertion after finish must
make both status currentness and receipt issuance refuse or remain unknown;
restoring the recorded state may restore currentness. Legacy path-only
commitments cannot establish this identity and read as unknown. The
`finished_attempt_currentness_follows_worktree_drift_like_the_receipt` and
`finished_gap_attempt_detects_changed_bound_test_bytes` tests exercise the
shared attempt owner and the gap-bound CLI respectively.

The attempt's own mutable operational scaffolding cannot be bound as if it
were authored input: a finished receipt must remain comparable after its
producer cleans up transient rollback or compatibility scaffolding, without
exempting arbitrary files or treating names as proof of ownership. For a
gap-bound attempt, the separate missing-projection mechanism (#10988) requires
validated retained receipt, verify, and `after_check_findings` evidence and
`RetainedGapAfterUse::CurrentEvidence` validation. Historical receipt
readability alone does not grant missing-projection custody. Only absent
members of the fixed, protected `GAP_WORKFLOW_PROJECTIONS` set are candidates;
present projection drift, selected/authored/forbidden paths, baseline-index
entries and unsafe root-relative paths receive no exemption.

The bounded reconstruction must produce exactly one candidate matching the
original whole-delta digest and verdict. It composes with the exact issued
receipt replacement above and validates declared retained gap evidence on
every accepting route. Root/path binding, attempt-store custody and unrelated
neighbor/sibling drift protections still apply. Legacy retention without the
after snapshot cannot authorize missing-projection reconstruction. The
check-findings validators refuse `input:v4:` identities; repo-exposure's
separate `input:v4:` identity algorithm remains supported. These are static
custody/currentness requirements, not a runtime-qualification claim.

Repair inventories and Git currentness probes ignore the user's
`diff.orderFile`: a dangling orderfile must not abort the before phase or the
after phase, including when the focused test edit was committed between phases.
A live orderfile leaves the same static result. Dirty production input still
refuses with its path and commit/restore recovery, without publishing an attempt.
Successful static repair evidence stays advisory and records that verification
was not run (#6837).

An after phase that refuses after selecting its attempt records the refusal on
the attempt (`last_after_refusal`, owned by `app::repair_attempt`): the final
error followed by the cause and recovery the after phase printed (for example
the changed analysis inputs), bounded by the attempt authority. Rule 2 still
resumes that attempt, but its reason names the refusal before repeating the
command. The next after phase that reaches the durable finish clears it.

Commit contention and changed-underfoot refusals map to exit 3 only when the
whole error is the authority's retry sentence with a valid attempt ID. An
operational error containing that sentence in a path remains an exit-2 error.
A rejected commit state check preserves manifest bytes and releases the lock,
allowing a later refusal observation and after-phase finish to proceed.

Rerunning the after phase of an attempt that is no longer awaiting its edit is
refused with the attempt's documented state, whether it finished or ended, the
workflow receipt path, and the next step (`ripr agent status`, or a new
`--phase before` while the gap is open).

Recorded resume commands keep the `--root` spelling the before phase was given,
because the manifest is the authority for that command (including the
`--edit-authorized` suffix of trust-bound Python attempts).

## Required Evidence

The first LLM work-loop slice requires:

- a JSON status report with schema version `0.1`;
- artifact presence for before snapshot, after snapshot, agent brief, agent
  packet, agent verify, and agent receipt;
- recoverable seam identity when an existing artifact names one;
- one missing-input command for every absent artifact;
- `next_command` selected by the #3906 order below, or `null` when nothing is
  missing or when status cannot choose honestly;
- stale-looking warnings for timestamp drift between verify and snapshots, and
  between receipt and verify;
- output schema, traceability, capability, and campaign entries that point to
  the behavior.
- shared command templates for the existing status next commands, agent brief
  next commands, LSP copy actions, pilot next commands, generated CI artifact
  paths, and operator cockpit missing-input commands.

The workflow manifest slice additionally requires:

- `ripr agent start --root . --seam-id <id> --out target/ripr/workflow`;
- `workflow.json`, `commands.md`, and `agent-brief.json` outputs;
- selected seam details from the agent brief;
- artifact paths and commands for before snapshot, packet, brief, after
  snapshot, verify, and receipt;
- explicit boundary flags proving the packet does not edit source, generate
  tests, call LLM APIs, run mutation testing, or configure CI blocking.

The receipt provenance slice additionally requires:

- agent receipt schema version `0.3`;
- SHA-256 hashes for before, after, and verify artifacts;
- `ripr_version`, `repo_root`, optional config fingerprint, command template
  version, and render timestamp;
- selected seam identity, before class, after class, and movement copied from
  the verify artifact;
- static boundary flags that do not claim runtime adequacy;
- bounded `summary.next_action` guidance for improved, changed, regressed,
  unchanged, new-gap, and resolved receipt states;
- fixture, schema, traceability, capability, and campaign updates.

The reviewer-summary slice additionally requires:

- `ripr agent review-summary --root .` Markdown output;
- `ripr agent review-summary --root . --json` schema `0.1` output;
- read-only joins for agent status, workflow, receipt, operator cockpit, repo
  exposure, LSP cockpit when present, and local CI artifact file state;
- target seam recovery from receipt first, then workflow, then status;
- static movement and next-action text copied from the receipt;
- an incomplete state with the next status command when the receipt is missing;
- explicit static-limit flags;
- schema, traceability, capability, and campaign updates.

The fixture slice additionally requires checked, artifact-only work-loop cases
for:

- happy path with improved static movement;
- unchanged static movement;
- regressed static movement;
- missing artifact recovery;
- stale-looking verify or receipt artifacts;
- configured-off seam handoff;
- path arguments with spaces;
- Windows separator normalization.

## Non-Goals

The LLM work loop must not:

- edit source files;
- generate tests;
- run mutation testing;
- run repo analysis inside `ripr agent status`;
- refresh LSP state;
- add speculative editor surfaces;
- add public crates;
- change the existing brief, packet, or verify schemas. Receipt schema changes
  are limited to the provenance-backed `0.2` shape and the next-action `0.3`
  additive shape.

## Acceptance Examples

- `ripr agent status --root . --json` succeeds without repo analysis when the
  root directory exists.
- Missing artifacts do not fail the command; they are reported with matching
  next commands.
- Malformed or unreadable present JSON artifacts are warnings, not hidden
  failures.
- The command can recover a seam from receipt, verify, packet, or brief JSON.
- Path arguments with spaces are quoted in generated next commands.
- LSP agent-loop copy action command payloads remain byte-for-byte compatible
  with the existing fixture expectations.
- Operator cockpit missing-input commands remain fixture-compatible while
  sharing the same template source as the CLI/LSP command builders.
- `ripr agent receipt` records artifact hashes and static provenance without
  rerunning analysis.
- `ripr agent receipt` emits structured, static `summary.next_action` guidance
  for improved, changed, regressed, unchanged, new-gap, and resolved states.
- `ripr agent review-summary --root .` emits compact Markdown without rerunning
  analysis.
- `ripr agent review-summary --root . --json` joins status, receipt, cockpit,
  repo exposure, optional LSP cockpit, and local CI artifact status.
- Generated GitHub CI writes and uploads workflow packet artifacts under
  `target/ripr/workflow`, including status JSON/Markdown and review summary
  JSON/Markdown, while keeping the job advisory.
- `docs/LLM_OPERATOR_GUIDE.md` documents the source-edit-free operator loop
  from status through workflow packet, focused test target, after snapshot,
  verify, receipt, and reviewer summary.
- Missing optional cockpit artifacts are visible state, not command failures.
- A missing receipt yields `status: incomplete` and carries the next command
  from agent status.
- No automatic edits, generated tests, runtime mutation execution, speculative
  LSP features, or new public crates are added.

## Amendment (#4396): selected-root editor command copy

The LSP action payload keeps `root: "."` as a portable role, while its
copyable command binds the selected workspace as an absolute `--root`. The VS
Code copy handler accepts a fixed agent-loop action only when its command body
exactly matches the server template for the selected workspace (or its realpath)
and an output redirect, when present, names the declared artifact under that
**same** root spelling. Relative redirects, another workspace root, traversal,
extra shell tokens, and ambiguous quoting fail closed. The separate gap command
labels retain their own bounded legacy contract. Copying a command is advisory;
it does not execute the command or grant edit authority.

## Amendment (#12831): bare repair takes the start check recommends

When bare `ripr repair` finds more than one eligible seam on the changed lines
and the default `ripr check` for the same change prints a repair start whose
seam is one of them (and needs no commit first), bare repair starts that seam.
`ripr check` stays the single authority on which start is recommended; repair
never picks by position or recency. When check prints no start, prints one for
a seam that is not eligible, cannot run, or the change cannot be scoped to
changed lines, the selection prompt (`several_eligible_seams`) is unchanged.

## Amendment (#12849): bare repair drops starts check cannot offer

When several seams are eligible on the changed lines and `ripr check` names no
single start, a seam whose bound check finding (the shared `finding_for_seam`
join) is exposed or static-limited (`finding_is_static_limited`) is not offered: check reads it as noticed or
cannot judge it, and offers no start for it. The refusal says so in plain words
for each such spot; if none remain, bare repair refuses (`no_eligible_seam`)
instead of listing starts. When exactly one seam survives and check recommended none, it is not started on its own; the refusal gives its `ripr repair <id>` command. A single eligible seam and an unscoped change keep
their earlier behavior.

## Amendment (#12861): refusal closing step for open spots on the change

A bare-repair refusal for a scoped change that still has open spots which
cannot start a repair (or whose candidates check closed) ends its lead with
"each changed spot below says what it still needs; write that test by hand",
naming `ripr check` for detail, instead of the repository-wide `ripr pilot`
route. When every candidate was dropped as already noticed, the outcome is the nothing-to-do one instead. Without a diff scope the repository route is kept. Eligibility and the
`no_eligible_seam` code are unchanged.

## Amendment (#12867): no start whose handoff card cannot be produced

A start publishes an attempt and then renders the handoff card, which names the
workspace only through an admitted test target. A seam whose repair would begin
in a proposed new test file has none, so the shared repair-admission gate
refuses it up front ("found no existing test to extend ... write the test by
hand") for bare repair (`no_eligible_seam`) and an explicit id
(`seam_not_repair_eligible`), and no attempt is created. `ripr agent start` and
the MCP admission use the same gate and refuse these seams too. The check
screen reads the same gate as a not-ready route and keeps its own wording
("not complete enough to name a safe test target"); it does not print this
reason.

## Amendment (#13203 A0): a producer-owned proposal may start a repair

The #12867 refusal existed because no producer-owned path could name the
workspace for a seam whose only target is a proposed new test. The producer now
does: a complete `Proposed` target (InlineUnit or Integration) from the
`new_test_target` authority carries the portable workspace identity minted from
the same workspace authority that names an existing test target's identity. The
shared admission gate (`repair_refusal`, through `workspace_identity_for`) starts
such a seam only when all of these hold: the readiness selection is the very
proposal the seam's admitted evidence holds, that proposal carries a non-empty
identity, and the packet's allowed edit surface is the proposal's file (a new
test surface, or the one governed inline module), so the attempt can enforce an
edit cage on it. Anything the producer does not admit (missing, ambiguous,
generated or vendored, wrong package, private owner, colliding file, stale
source, cross-language, path escape) still has no proposal and refuses as
before; a proposal without an identity or one the evidence does not hold keeps
the no-target text. `ripr check` and `ripr repair` read the same gate; a
`no path` (static-limited) finding keeps the F3 rule (#12888) and prints no
runnable repair line even where bare `ripr repair <id>` starts the
proposal-only seam, so the F3 silence is unchanged. Starting does not create the
proposed file, generate a test body or grant production-source permission.

## Amendment (#13203 A): an attempt binds the proposal it follows

When the repair packet's `recommended_test.target_kind` names a producer-owned
new-test proposal (`new_inline_test_module` or `new_integration_test`), the
attempt records it in the retained edit-cage baseline policy as
`new_test_proposal` (`kind`, `file`). The proposal's file must be the selected
edit target, and its kind must agree with the cage's inline-module confinement;
otherwise the policy is invalid and fails closed. An integration proposal is
realized only by a file the attempt created at exactly that path: creating any
other file, even a test file, is an out-of-cage edit, and a baseline whose
proposed file already exists, or a proposed file later seen as modified, is a
stale proposal and is refused rather than credited. An inline proposal is
realized only by editing the existing file's governed module. An existing-test
repair carries no `new_test_proposal`, so its policy and packet bytes are
unchanged. "Already exists" at bind time means any non-missing identity at the integration
path, a symlink or special file included, so such a path is stale and fails
closed. Staleness at bind time and at verdict time is covered by the
edit-cage unit tests; the real-binary tests cover the bound, created-file
credit and the out-of-cage refusal.

## Amendment (#14079): a recorded focused test result is read back, not reported as absent

A finish that retained a caller-reported focused result (#13203 B) reports it
on the test-run axis. The finish summary, `ripr status` and `ripr agent status`
(default and `--attempt`, Markdown and JSON) say `Test run: reported by the
caller, not run by ripr: N test(s) ran in <file> (P passed, F failed)`, and the
JSON `test_run` object carries `status: "caller_reported"` with `file`, `ran`,
`passed` and `failed`. A reported failure is named with its next step; it is
never presented as a pass and never hidden behind `none recorded`. An attempt
with no retained result keeps `Test run: none recorded` / `not_recorded`.

The reading comes from the record the finish committed in the attempt
manifest, and only when the `focused_test_result` terminal binding names
exactly those bytes and the record is for this attempt. The two axes stay
separate: the receipt's `verification.status` remains `verification_not_run`
(ripr ran nothing), and static movement is whatever the static comparison
found, so a reported failure can sit beside `improved`. The receipt-owned
`next:` line is unchanged. MCP documents do not project the record yet.

## Amendment (#13203 B): a caller-supplied focused test result is validated and recorded

ripr does not run `cargo test` (ADR 0022 keeps execution authority outside
ripr). `ripr agent repair --phase after --attempt <id> --test-result <file>`
takes a result the caller produced and only validates and records it. The file
is a JSON document with `schema_version` `0.1`, `kind` `focused_test_result`,
`attempt_id`, `head`, `test_file` and `libtest_events` (libtest JSON event
objects). The shared after-phase authority checks, before anything is
finished, that the envelope and recognized event fields are well formed, that `attempt_id` is this attempt,
that `head` is the repository HEAD the phase evaluates, that `test_file` is the
attempt's selected test file (the proposal's file for a proposal), and that at
least one test **ran** (`ok` or `failed`; `started`, `ignored` and libtest's slow-test `timeout` notice do not count)
and is an executable test defined in that file, or, for an inline module,
reached through the governed module's path. Duplicates do not add to the count.
Any failure, including zero counted tests, is a refusal that leaves the attempt
awaiting its edit and records nothing. When the cage then finishes compliant,
the normalized record (`ran_tests`, `passed`, `failed`, `uncounted_ran`, the
bound attempt id and HEAD) is retained as the `focused_test_result` terminal
artifact. The record states that ripr checked shape, binding, file membership
and per-test outcomes and did not observe the run: the caller's claim that
those events came from that file stays the caller's. A failed test the selected file
does not define is refused rather than passed over. `--test-result` is
optional and does not gate finish: it validates and records a caller-reported
result, and requiring one to finish is a later, separate change.
The retained artifact is the normalized record, not the caller's bytes.
Caller-reported progress must be complete: started or timeout tests need a
terminal outcome, and each announced suite must close before another begins.
Suite integer summaries must match distinct observed outcomes (including foreign
tests) and the announced test count, excluding filtered-out tests. Contradictory
outcomes within a suite refuse; identical duplicates do not inflate counts.
Sequential suites are supported; any observed failure remains visible in the
normalized record. Terminal-only streams (no suite framing) reuse one outcome table across
binaries, so one test name reported with different outcomes by two binaries is
refused as contradictory. Test names are module-qualified executable test
functions; a function whose symbol identity is ambiguous (for example cfg-gated
twin tests at one path) is dropped from the defined set, so a result naming it
counts nothing and is refused rather than guessed at. Terminal-only claims remain supported and do not establish
whole-process completion. Neither framing nor counts authenticate the binary,
the run's origin, an exit code or omitted suites. Bench rows require numeric
nonnegative median and deviation and reconcile as measured outcomes.
Before committing a compliant finish, the existing edit-cage capture must
match the HEAD and the digest of the selected test file taken at validation
time. If either differs from the evaluated capture, the after phase refuses without committing a verdict, so the caller
can rerun the test and retry. A descendant committed before validation remains
eligible. An edit to the test file between the caller's run and validation,
with HEAD unchanged, is not detected: that the events came from this file's
content stays the caller's claim. A result remains optional and does not gate
finish.
If publication or terminal retention fails after a compliant finish, the
attempt is restored to `awaiting_edit` for retry when the exact finish is
still owned and no terminal evidence has been committed. An apply-record
failure can also restore the exact terminal retention that this same
invocation committed, provided its entire manifest remains unchanged; reusing
another retainer's result grants no rollback custody. An unsuccessful restore
is reported explicitly. If evaluation cannot start after the finish committed,
the finish is restored the same way. A retainer that reuses terminal evidence
another writer already committed receives no rollback custody: a later
apply-record failure is reported and the attempt stays finished rather than
being rolled back. Restoring an attempt leaves the compatibility receipt and
verify projections under `target/ripr/` on disk; they are overwritten by the
retry and are never the attempt's authority. Terminal retention now holds the
manifest commit lock for the whole write for every attempt; non-focused
attempts keep the same evidence set and the existing retention tests cover
them. The finish commits the normalized claim before
publishing the receipt, and retention checks the bytes against that claim.
A focused finish records its receipt protocol before evaluation starts.
A separate durable marker precedes evaluation, and a successful decision commits
the exact rendered receipt and verified-input digests before receipt publication.
A stop after the marker without a matching durable success and receipt is
ambiguous, so status reports the attempt as limited and a new attempt is the
safe route. Standalone receipt publication does not commit the after-phase
outcome. Recovering an interrupted focused finish with the original claim is
not part of this change: it lands separately, and until then an interrupted
focused finish is inspected and restarted as a new attempt. These transaction
checks assume the local manifest and cooperating writers honor the existing
lock and integrity model; they do not protect against a writer that can
arbitrarily rewrite the manifest itself.
When a trusted-surface receipt refusal ends evaluation after the marker was
recorded, the attempt is deliberately not restored: it stays finished, status
reports it as limited with the started marker, and the safe route is to start a
new attempt (`ripr agent repair --phase before`). If the marker commit itself
reports an error whose outcome is uncertain, restore refuses (a marker may
exist) and says so; the same inspect-then-restart route applies. The named trusted-surface receipt refusal remains a decision refusal with its
existing recovery guidance; it is not classified as a publication failure.
These records remain caller claims.

A refusal is an after-phase refusal like any other: it counts against the
attempt's after-phase retry budget, so "retryable" holds only up to that
budget. When a validated result is not recorded because the cage verdict is not
compliant, stderr says so in one line instead of dropping it silently.

Attribution remains the caller's claim: libtest events do not identify their
test binary, so another binary can have the same test path. For an integration
file the event's module-qualified test name must match a parser-identified
executable test in the selected file. For an inline module the event must match
the parser-identified test's full path within the uniquely governed module,
prefixed by the file's own module path (`src/foo.rs` is `foo`, `src/a/b.rs` is `a::b`,
`src/a/mod.rs` is `a`, `lib.rs` and `main.rs` are the crate root, `tests/x.rs`
is its own root), so `bar::tests::adds` does not
count for `src/foo.rs`. When that module path cannot be derived (a `src/bin`
target, a non-`src` file, a nested `tests/` file) nothing counts; a `#[path]`
module is not detectable from the file and stays the caller's claim. A failed
test counts as having **ran**: `ran_tests` includes it and `failed` reports it,
so a later finish gate would have to require `failed == 0` to treat it as a pass.

A focused test result is accepted for Rust test files only: a Python or
TypeScript attempt that passes `--test-result` is refused up front with that
scope and the next step of finishing without the flag. A result file must be
saved outside the checkout; one inside it would be an untracked edit the cage
judges, so it is refused with that next step.

## Amendment (#13205): a proposed new-test target is preflighted read-only

Before an attempt follows a new-test proposal, one shared preflight
(`preflight_new_test_target`) looks at the proposed target without running or
writing anything. It requires a plain path inside the repository with no
symlinked directory on the way, the same stale-premise rule the bind uses (an
integration proposal's path must hold nothing, an inline proposal's production
file must exist), a `tests/<name>.rs` shape for an integration test, and an
owning Cargo package. A target that fails is refused with a next step and no
attempt starts. A target that passes is reported as its own state, not yet
existing: the attempt's `limitations` carry a `target preflight:` line naming the
proposed file and its package and saying there is no test result for it yet, so
a missing result never reads as an empty one. Attempts that follow an existing
test target carry no such line and are unchanged. Nothing is executed and no
test body is generated.
The preflight runs at manifest publication; the edit-cage bind remains the
early stale guard, and both use the same stale rule. A virtual workspace
manifest (no `[package]`) does not own a proposed test, and a manifest that
exists but cannot be read or parsed refuses rather than falling through to an
ancestor.

The limitation records the before-phase observation in historical language, so
it remains accurate after the proposed test is created and the attempt finishes.
Package-name syntax follows stable Cargo 1.95 (Unicode XID start or underscore,
then XID continuation or hyphen), rather than cargo-new or registry restrictions.
This check establishes name identity, not complete Cargo manifest validity.

## Amendment (#13204): one projection of the selected new-test proposal

The selected producer-owned new-test proposal is read through one projection,
`SelectedProposal {kind, file, module}`, owned beside the proposal in
`analysis/new_test_target.rs`. The repair card's proposed target and the agent
packet's recommended test take their kind and file from it unchanged, and MCP
discloses it as `proposed_new_test {kind, file, module, summary}` on a gap item
whose shared-gate target is the proposal (a `SeamVerdict::AdmittedProposed`),
including a proposal-only item whose finding has no existing fix site
(`fix_site_not_established`). An item held by any other gate names none.
Amendment (#13538): the verdict is also the readiness authority. A finding that
passed the finding-level actionability and discriminator gates but holds only
for lack of an existing fix site (`fix_site_not_established`, or an existing
test off a test surface) is ready exactly when the shared gate admits the
proposal as the seam's target, with the proposal as its fix site: the
integration file (which must be a shared test-surface path) or the production
file with its one governed inline module. `ripr_prepare_repair` then names that
file as its allowed edit surface, the file `ripr agent repair --phase before`
confines the edit cage to (`selected_target.path`), with `fix_site.new_test_kind`
set and `test_name`, `line` and `oracle` empty because no test exists yet. A
proposal that cannot take that shape stays held, and a seam the producer admits
no proposal for stays not ready on both surfaces. No MCP-side gate decides this.
The proposal fix site additionally requires the filesystem-free part of the CLI
preflight (`is_plain_relative_path`, `is_integration_test_file_shape`: no `..`,
no absolute path, exactly `<dir>/tests/<name>.rs`); a ready item whose proposal
cannot take that shape is held as `fix_site_not_established` rather than keeping
a stale existing-test fix site. An inline proposal is read from the shared
verdict before the plain inline-module verdict, so it is distinguished from an
existing inline test and stays confined to its governed module
(`inline_test_module`). The file-exists, symlink and owning-manifest checks stay
at the CLI start preflight (`ripr agent repair --phase before`), not at MCP
prepare. Prepare packets now carry `fix_site.new_test_kind` (`null` for an
existing-test fix site).
No renderer picks its own file or kind, no target is
selected here and no test is generated. No insertion anchor exists yet; the only
anchor-like fact is the governed inline module name, carried as `module` where
the surface holds the admission's region (MCP does, and an InlineUnit proposal's
module appears there; the card and packet hold only the proposal, so they show
none). A seam whose recommended file is its own production file with an owner
inline region keeps the existing `AdmittedInline` verdict, which already names
the module, so it is not repeated as a proposal. A not-ready item held by any gate
other than `fix_site_not_established`, or a refused verdict, discloses no
proposal. The
LSP and VS Code surfaces already read `recommended_test.file` from the packet;
asserting their parity is slice 2 of #13204.

## Amendment (#12959): a refused changed-field seam names its next step

A refusal for a field-construction seam whose only missing fact is the changed
field value names the working stub (`ripr agent stub ... --kind
field_construction`) or, when the stub refuses (for example a by-value `self`
setter), says why it cannot write one and that the test is written by hand.
Refusal text only: eligibility is unchanged, and starting a repair from a weak
read of a written field stays #11870.

## Amendment (#11584): task-first repair and continue print JSON on request

`ripr repair --json` prints the repair packet with its `repair_attempt`
block (the attempt id and after-phase command) on stdout instead of the
summary and card; a repeated start that resumes an awaiting attempt prints
that attempt's status document. `ripr continue --json` prints the after-phase
document, or the attempt's status document when it is already complete; a
typed after-phase refusal or failure prints its `repair_after_refusal`,
`repair_after_failure` or verify document beside the same exit code and
stderr prose. Repair refusals, continue selection refusals and operational
errors before verify keep empty stdout. An unknown flag (or, for `continue`,
a stray positional argument) under `--json` keeps exit 2 and its stderr prose
and writes the standalone `cli_refusal` document to stdout (#6834 decision);
other argv usage errors keep empty stdout.

## Amendment (#13833): line-ending-only files are named, not silently dropped

A path whose before and after states differ only in line endings, on a path
Git itself normalizes (`core.autocrlf` or a `text` attribute), stays out of the
attempt delta (#7377). The after phase records each such path in
`after.line_ending_only_paths`, beside the verdict and omitted when empty, and
narrates one bounded stderr line that starts `not counted as edits:`. Path
names in that line are shown with control characters escaped. The bound delta
digest, the stored verdict and the receipt binding do not include the list. A
flip on a path Git does not normalize, a real content change, and a path a
committed change also touches are not listed.

- `crates/ripr/src/edit_cage/mod.rs::tests::eol_only_flip_of_a_forbidden_file_is_not_an_edit` (the flipped file is reported exactly; its real-edit control is not)
- `crates/ripr/src/edit_cage/mod.rs::tests::eol_flip_without_git_normalization_remains_an_edit` (a git-visible flip is not reported)
- `crates/ripr/src/cli/commands/agent.rs::tests::line_ending_only_note_names_skipped_files_and_stays_silent_without_them` (narration, bound, escaping, manifest round-trip)

## Test Mapping

- `crates/ripr/tests/cli_help_hierarchy.rs::agent_help_and_missing_seam_refusal_agree_that_finding_ids_resolve` (agent group help, the missing-`--seam-id` refusal and the workflow catalog agree that `--phase before` resolves `ripr check` finding IDs)
- `crates/ripr/tests/repair_check_recommended_start.rs::boundary_change_starts_the_seam_check_recommends` (#12831)
- `crates/ripr/tests/repair_check_recommended_start.rs::error_variant_change_starts_the_changed_branch_check_recommends` (#12831)
- `crates/ripr/tests/repair_check_recommended_start.rs::a_change_check_reads_as_noticed_and_unknown_lists_no_start` (#12831)
- `crates/ripr/tests/repair_check_recommended_start.rs::an_unscoped_change_keeps_the_selection` (#12831)
- `crates/ripr/tests/repair_check_recommended_start.rs::open_spots_on_the_change_close_on_the_per_spot_reasons` (#12861)
- `crates/ripr/tests/repair_check_recommended_start.rs::a_start_whose_card_cannot_be_produced_creates_no_attempt` (#12867)
- `crates/ripr/tests/repair_check_recommended_start.rs::a_refused_field_seam_names_the_stub_or_why_there_is_none` (#12959)
- `crates/ripr/tests/mcp_cli_repair_parity.rs::six_step_agent_contract_holds_through_cli_and_mcp` (#14079: gap, one unit, one test edit, re-recorded static evidence, the caller's run as a separate axis, durable read-back from a new MCP process)
- `crates/ripr/src/app/focused_run.rs::tests::a_retained_record_reads_back_only_for_its_own_attempt_and_kind` (#14079)
- `crates/ripr/src/cli/commands/agent.rs::repair_summary_tests::repair_summary_reports_a_recorded_caller_run_instead_of_none_recorded` (#14079)
- `crates/ripr/tests/proposal_only_repair_start.rs::integration_proposal_only_seam_starts_with_the_proposed_file_as_its_cage` (#13203)
- `crates/ripr/tests/proposal_only_repair_start.rs::inline_proposal_only_seam_starts_confined_to_its_governed_module` (#13203)
- `crates/ripr/tests/proposal_only_repair_start.rs::seam_without_any_proposal_still_refuses` (#13203)
- `crates/ripr/tests/proposal_only_repair_start.rs::ambiguous_inline_modules_still_refuse` (#13203)
- `crates/ripr/tests/proposal_only_repair_start.rs::colliding_integration_files_still_refuse` (#13203)
- `crates/ripr/src/app/repair_card_handoff.rs::tests::a_proposal_names_the_workspace_only_when_the_evidence_admitted_it` (#13203)
- `crates/ripr/tests/new_test_proposal_attempt.rs::created_proposed_file_binds_the_attempt_to_its_proposal` (#13203)
- `crates/ripr/tests/new_test_proposal_attempt.rs::edit_outside_the_proposals_cage_is_refused` (#13203)
- `crates/ripr/tests/proposal_only_repair_start.rs::inline_proposal_with_an_invalid_package_identity_leaves_no_attempt` (#13205)
- `crates/ripr/tests/proposal_only_repair_start.rs::package_name_preflight_agrees_with_cargo_metadata` (#13205)
- `crates/ripr/src/output/agent_seam_packets.rs::tests::selected_proposal_reads_the_same_in_card_packet_and_mcp` (#13204)
- `crates/ripr/tests/new_test_proposal_attempt.rs::a_proposal_attempt_says_no_test_exists_yet` (#13205)
- `crates/ripr/tests/new_test_proposal_attempt.rs::an_existing_test_target_attempt_has_no_not_yet_disclosure` (#13205)
- `crates/ripr/src/analysis/new_test_target/preflight_tests.rs::a_plain_integration_path_with_no_file_has_no_test_yet` (#13205)
- `crates/ripr/src/analysis/new_test_target/preflight_tests.rs::a_member_package_is_named_for_its_own_tests_directory` (#13205)
- `crates/ripr/src/analysis/new_test_target/preflight_tests.rs::paths_that_escape_or_are_not_plain_are_refused` (#13205)
- `crates/ripr/src/analysis/new_test_target/preflight_tests.rs::a_stale_premise_is_refused_by_the_shared_rule` (#13205)
- `crates/ripr/src/analysis/new_test_target/preflight_tests.rs::a_target_with_no_owning_package_is_refused` (#13205)
- `crates/ripr/src/analysis/new_test_target/preflight_tests.rs::a_symlinked_parent_directory_is_refused` (#13205)
- `crates/ripr/tests/new_test_proposal_attempt.rs::a_symlinked_tests_directory_fails_the_preflight_and_leaves_no_attempt` (#13205)
- `crates/ripr/src/analysis/new_test_target/preflight_tests.rs::a_virtual_workspace_root_does_not_own_tests` (#13205)
- `crates/ripr/src/analysis/new_test_target/preflight_tests.rs::an_empty_test_file_name_is_refused` (#13205)
- `crates/ripr/src/analysis/new_test_target/preflight_tests.rs::a_symlinked_production_file_is_refused_for_an_inline_module` (#13205)
- `crates/ripr/src/analysis/new_test_target/preflight_tests.rs::a_broken_member_manifest_refuses_instead_of_falling_to_the_parent` (#13205)
- `crates/ripr/src/edit_cage/mod.rs::tests::integration_proposal_is_bound_only_by_the_created_proposed_file` (#13203)
- `crates/ripr/src/edit_cage/mod.rs::tests::integration_proposal_for_an_existing_file_is_stale` (#13203)
- `crates/ripr/src/edit_cage/mod.rs::tests::proposal_kind_must_agree_with_the_cage_shape` (#13203)
- `crates/ripr/src/edit_cage/mod.rs::tests::inline_proposal_for_a_missing_file_is_stale` (#13203)
- `crates/ripr/src/edit_cage/mod.rs::tests::integration_proposal_over_a_symlink_is_stale` (#13203)
- `crates/ripr/src/output/agent_seam_packets.rs::tests::recommended_test_matches_only_the_exact_proposed_file` (#13203)
- `crates/ripr/tests/focused_test_result.rs::a_bound_result_with_a_ran_test_is_validated_and_recorded` (#13203)
- `crates/ripr/tests/focused_test_result.rs::a_result_where_no_test_ran_in_the_changed_file_is_not_counted` (#13203)
- `crates/ripr/tests/focused_test_result.rs::a_result_bound_to_another_attempt_or_head_is_refused` (#13203)
- `crates/ripr/tests/focused_test_result.rs::a_result_for_another_file_or_a_malformed_result_is_refused` (#13203)
- `crates/ripr/tests/focused_test_result.rs::an_inline_result_counts_only_tests_in_the_governed_module` (#13203)
- `crates/ripr/tests/focused_test_result.rs::test_result_is_only_valid_with_the_after_phase` (#13203)
- `crates/ripr/tests/focused_test_result.rs::a_failure_outside_the_file_is_refused_not_passed_over` (#13203)
- `crates/ripr/src/app/focused_run.rs::tests::a_failure_the_file_does_not_explain_is_refused` (#13203)
- `crates/ripr/tests/focused_test_result.rs::a_result_saved_inside_the_checkout_is_refused_up_front` (#13203)
- `crates/ripr/src/app/focused_run.rs::tests::a_non_rust_selected_file_is_refused_up_front` (#13203)
- `crates/ripr/tests/focused_test_result.rs::a_failed_only_result_is_recorded_as_ran_with_the_failure_visible` (#13203)
- `crates/ripr/tests/focused_test_result.rs::failed_focused_result_publication_restores_the_attempt_for_retry` (#13203)
- `crates/ripr/tests/focused_test_result.rs::failed_receipt_publication_restores_focused_finish_for_retry` (#13203)
- `crates/ripr/tests/focused_test_result.rs::receipt_surface_decision_is_not_mislabeled_as_publication_failure` (#13203)
- `crates/ripr/tests/focused_test_result.rs::another_after_cannot_drop_or_undo_a_focused_result` (#13203)
- `crates/ripr/tests/focused_test_result.rs::foreign_focused_bytes_are_not_retained` (#13203)
- `crates/ripr/tests/focused_test_result.rs::noncompliant_finish_does_not_retain_the_supplied_focused_result` (#13203)
- `crates/ripr/tests/focused_test_result.rs::malformed_suite_event_is_refused_even_with_a_counted_test` (#13203)
- `crates/ripr/tests/focused_test_result.rs::completion_counts_and_multiple_suites_are_checked` (#13203)
- `crates/ripr/tests/focused_test_result.rs::incomplete_suites_are_refused_before_finish` (#13203)
- `crates/ripr/tests/focused_test_result.rs::interrupted_surface_refusal_cannot_be_retried` (#13203)
- `crates/ripr/tests/focused_test_result.rs::standalone_receipt_does_not_finish_an_unresolved_evaluation` (#13203)
- `crates/ripr/tests/focused_test_result.rs::a_failed_evaluation_start_restores_the_attempt_for_retry` (#13203)
- `crates/ripr/src/app/repair_attempt/mod.rs::tests::focused_restore_refuses_another_finish_or_committed_evidence` (#13203)
- `crates/ripr/tests/focused_test_result.rs::focused_result_does_not_bind_a_replaced_test_file` (#13203)
- `crates/ripr/tests/focused_test_result.rs::focused_result_does_not_bind_an_intervening_descendant_commit` (#13203)
- `crates/ripr/tests/focused_test_result.rs::focused_result_accepts_a_descendant_committed_before_validation` (#13203)
- `crates/ripr/tests/focused_test_result.rs::finish_without_a_focused_result_remains_supported` (#13203)
- `crates/ripr/src/app/focused_run.rs::tests::inline_result_requires_the_named_test_to_be_in_the_governed_module` (#13203)
- `crates/ripr/src/app/focused_run.rs::tests::an_inline_module_is_attributed_through_the_files_own_module_path` (#13203)
- `crates/ripr/src/app/focused_run.rs::tests::an_inline_module_whose_path_cannot_be_derived_counts_nothing` (#13203)
- `crates/ripr/src/app/focused_run.rs::tests::a_timeout_event_is_not_terminal_and_never_counts_by_itself` (#13203)
- `crates/ripr/src/app/focused_run.rs::tests::zero_in_scope_tests_are_not_counted` (#13203)
- `crates/ripr/src/app/task_first.rs::tests::closure_outcome_says_nothing_to_do_when_everything_was_noticed` (#12861)
- `crates/ripr/src/cli/commands/task_first.rs::tests::open_on_change_closes_on_the_reasons_not_the_repository_tour` (#12861)
- `crates/ripr/src/app/task_first.rs::tests::both_check_consumers_share_one_check_run` (#12831)
- `crates/ripr/src/app/task_first.rs::tests::check_pick_narrows_only_when_eligible_and_committed` (#12831)
- `crates/ripr/tests/cli_smoke/repair_orderfile.rs::repair_completes_despite_user_orderfile`
- `crates/ripr/tests/cli_smoke/repair_orderfile.rs::dirty_production_requires_recovery_despite_user_orderfile`

- `crates/ripr/src/app/agent_status.rs::tests::agent_status_reports_missing_artifacts_and_next_commands`
- `crates/ripr/src/app/agent_status.rs::tests::agent_status_recovers_seam_id_from_receipt`
- `crates/ripr/src/app/agent_status.rs::tests::agent_status_recovers_seam_id_from_verify_packet_or_brief`
- `crates/ripr/src/app/agent_status.rs::tests::agent_status_warns_when_verify_or_receipt_look_stale`
- `crates/ripr/src/app/agent_status.rs::tests::agent_status_quotes_paths_with_spaces`
- `crates/ripr/src/app/agent_status.rs::tests::agent_status_refuses_to_choose_between_open_seams`
- `crates/ripr/src/app/agent_status.rs::tests::agent_status_does_not_restart_a_seam_that_finished`
- `crates/ripr/src/app/agent_status.rs::tests::agent_status_never_redirects_into_a_missing_workflow_directory`
- `crates/ripr/src/app/agent_status.rs::tests::agent_status_stops_when_pilot_routed_changed_code_to_check`
- `crates/ripr/tests/cli_smoke.rs::agent_status_routes_a_fresh_workspace_to_pilot`
- `crates/ripr/tests/cli_smoke.rs::agent_status_resumes_the_current_awaiting_repair_attempt`
- `crates/ripr/tests/cli_smoke.rs::agent_status_resumes_only_the_current_attempt_and_refuses_to_guess`
- `crates/ripr/tests/cli_smoke.rs::agent_status_restarts_a_failed_attempt_and_completes_a_finished_one`
- `crates/ripr/tests/cli_smoke.rs::agent_status_selects_nothing_past_an_unreadable_attempt`
- `crates/ripr/src/app/agent_workflow.rs::tests::workflow_manifest_extracts_seam_and_commands`
- `crates/ripr/src/app/agent_workflow.rs::tests::workflow_manifest_errors_when_brief_does_not_return_seam`
- `crates/ripr/src/cli/agent.rs::tests::agent_status_parses_root_and_json`
- `crates/ripr/src/cli/agent.rs::tests::agent_status_requires_json_and_rejects_unknown_arguments`
- `crates/ripr/src/cli/agent.rs::tests::agent_args_parse_start_request`
- `crates/ripr/src/cli/agent.rs::tests::agent_start_defaults_out_dir_and_requires_seam_id`
- `crates/ripr/src/cli/commands.rs::tests::agent_status_rejects_missing_root_before_reading_artifacts`
- `crates/ripr/src/cli/commands.rs::tests::agent_start_rejects_missing_root_before_analysis`
- `crates/ripr/src/agent/loop_commands.rs::tests::workflow_commands_match_existing_status_templates`
- `crates/ripr/src/agent/loop_commands.rs::tests::editor_commands_match_existing_lsp_templates`
- `crates/ripr/src/agent/loop_commands.rs::tests::anchored_redirect_target_roots_relative_outputs_at_root`
- `crates/ripr/src/agent/loop_commands.rs::tests::bound_roots_render_absolute_and_relative_roots_stay_portable`
- `crates/ripr/src/agent/loop_commands.rs::tests::bound_root_keeps_a_unix_backslash_directory_name`
- `crates/ripr/src/mcp/repair_root_tests.rs::durable_inventory_awaiting_and_missing_commands_retain_literal_unix_root`
- `crates/ripr/src/mcp/repair_root_tests.rs::durable_inventory_failed_restart_retains_literal_unix_root`
- `crates/ripr/src/mcp/repair_root_tests.rs::durable_inventory_open_gap_restart_retains_literal_unix_root`
- `crates/ripr/src/app/agent_status.rs::tests::pilot_select_command_binds_raw_and_bound_roots_once`
- `crates/ripr/tests/generated_review_workflow.rs::generated_status_command_runs_from_a_foreign_working_directory`
- `crates/ripr/src/lsp/tests.rs::agent_loop_command_payloads_stay_root_anchored_for_platform_roots`
- `editors/vscode/test/suite/extension.test.ts::agent loop commands must equal the body the server renders from the payload (#4225)`
- `editors/vscode/test/suite/extension.test.ts::real server surfaces seam diagnostic, hover provider, and agent actions`
- `crates/ripr/src/output/agent_workflow.rs::tests::workflow_json_is_structured_and_advisory`
- `crates/ripr/src/output/agent_workflow.rs::tests::workflow_markdown_lists_commands_and_boundaries`
- `crates/ripr/src/output/agent_receipt.rs::tests::agent_receipt_json_selects_changed_seam`
- `crates/ripr/src/cli/commands_agent_support.rs::tests::dangling_ripr_toml_symlink_is_present_not_built_in_defaults`
- `crates/ripr/src/cli/commands_agent_support.rs::tests::unreadable_ripr_toml_lookup_is_not_built_in_defaults`
- `crates/ripr/src/output/agent_receipt.rs::tests::agent_receipt_guidance_covers_improved_state`
- `crates/ripr/src/output/agent_receipt.rs::tests::agent_receipt_guidance_covers_changed_state`
- `crates/ripr/src/output/agent_receipt.rs::tests::agent_receipt_guidance_covers_regressed_state`
- `crates/ripr/src/output/agent_receipt.rs::tests::agent_receipt_guidance_covers_unchanged_state`
- `crates/ripr/src/output/agent_receipt.rs::tests::agent_receipt_guidance_covers_new_gap_state`
- `crates/ripr/src/output/agent_receipt.rs::tests::agent_receipt_guidance_covers_resolved_state`
- `crates/ripr/src/output/agent_receipt.rs::tests::agent_receipt_input_paths_extracts_verify_snapshot_paths`
- `crates/ripr/src/agent/provenance.rs::tests::sha256_file_hashes_artifact_bytes`
- `crates/ripr/src/app/agent_review_summary.rs::tests::agent_review_summary_joins_status_receipt_cockpit_repo_and_lsp`
- `crates/ripr/src/app/agent_review_summary.rs::tests::agent_llm_work_loop_review_summary_fixtures_pin_core_states`
- `crates/ripr/src/app/agent_review_summary.rs::tests::agent_llm_work_loop_review_summary_fixture_pins_missing_artifact`
- `crates/ripr/src/app/agent_review_summary.rs::tests::agent_llm_work_loop_review_summary_fixture_pins_stale_artifact`
- `crates/ripr/src/app/agent_review_summary.rs::tests::agent_llm_work_loop_review_summary_fixtures_pin_path_arguments`
- `crates/ripr/src/app/agent_review_summary/root_tests.rs::review_summary_commands_retain_literal_unix_root_and_redirect`
- `crates/ripr/src/app/agent_review_summary.rs::tests::agent_review_summary_reports_missing_receipt_with_next_command`
- `crates/ripr/src/app/agent_review_summary.rs::tests::agent_review_summary_markdown_names_review_focus_and_limits`
- `crates/ripr/src/cli/agent.rs::tests::agent_review_summary_parses_root_json_and_human_default`
- `crates/ripr/src/cli/agent.rs::tests::agent_review_summary_requires_values_and_rejects_unknown_arguments`
- `crates/ripr/src/cli/commands.rs::tests::agent_review_summary_rejects_missing_root_before_reading_artifacts`
- `crates/ripr/tests/cli_smoke.rs::agent_receipt_writes_one_seam_handoff_json`
- `crates/ripr/tests/cli_smoke.rs::agent_start_writes_source_edit_free_workflow_packet`
- `crates/ripr/tests/cli_smoke.rs::agent_start_unknown_seam_id_exits_3_while_other_failures_exit_2`
- `crates/ripr/tests/cli_smoke.rs::agent_start_config_hidden_known_seam_is_a_policy_refusal`
- `crates/ripr/tests/cli_smoke.rs::agent_packet_rejects_configured_off_seam`
- `crates/ripr/src/lsp/tests.rs::agent_loop_command_payloads_stay_workspace_relative_for_platform_roots`
- `xtask/src/reports/operator.rs::tests::operator_cockpit_matches_editor_agent_loop_fixture`
- `crates/ripr/tests/task_first_facade.rs::repair_json_prints_the_packet_with_the_attempt_id` (#11584)
- `crates/ripr/tests/task_first_facade.rs::continue_json_prints_its_after_phase_document` (#11584)
- `crates/ripr/tests/task_first_facade.rs::continue_json_prints_the_typed_after_refusal_document` (#11584)
- `crates/ripr/tests/task_first_facade.rs::repair_and_continue_json_keep_refusals_prose_and_usage_errors_cli_refusal` (#11584)

## Implementation Mapping

- `crates/ripr/src/cli/commands_agent_support.rs` shares native CLI identity-path
  formatting and produces admitted receipt provenance; command dispatch in
  `crates/ripr/src/cli/commands/agent.rs` reuses it for verify and receipt inputs.

- `crates/ripr/src/app/agent_status.rs` builds and renders the report from
  existing artifact files and the repair-attempt inventory.
- `crates/ripr/src/app/repair_attempt.rs::inventory_repair_attempts` lists and
  validates attempt manifests read-only for status.
- `crates/ripr/src/app/agent_review_summary.rs` joins existing agent status,
  workflow, receipt, cockpit, repo exposure, optional LSP cockpit, and local
  CI artifact file state into review-summary JSON and Markdown.
- `crates/ripr/src/app/agent_workflow.rs` builds a selected-seam workflow
  manifest from the generated agent brief and shared command templates.
- `crates/ripr/src/agent/loop_commands.rs` owns internal command and artifact
  templates for status, brief, LSP copy actions, pilot next commands, generated
  CI paths, and cockpit missing-input commands.
- `editors/vscode/src/client.ts` validates copied LSP command templates against
  the selected workspace root and artifact redirect before clipboard write.
- `crates/ripr/src/agent/provenance.rs` hashes receipt artifacts with SHA-256.
- `crates/ripr/src/cli/agent.rs` parses the status, start, and review-summary
  subcommands.
- `crates/ripr/src/cli/commands.rs` validates the root and dispatches the
  report, builds receipt provenance from existing artifacts, and reuses shared
  path templates for generated GitHub workflow agent artifacts.
- `crates/ripr/src/cli/help.rs` documents the command surface.
- `crates/ripr/src/cli/commands.rs` renders the generated GitHub workflow that
  uploads LLM work-loop packet artifacts without changing analyzer behavior.
- `docs/LLM_OPERATOR_GUIDE.md` documents the human and external-agent operating
  loop and anti-goals.
- `crates/ripr/src/output/agent_workflow.rs` renders the workflow JSON and
  commands Markdown.
- `crates/ripr/src/output/agent_receipt.rs` renders receipt schema `0.3` with
  provenance and structured next-action guidance.
- `crates/ripr/src/output/agent_brief.rs`, `crates/ripr/src/output/pilot/mod.rs`,
  and `crates/ripr/src/lsp/actions.rs` reuse the shared command builders for
  their current command payloads.
- `xtask/src/reports/operator.rs` reuses the shared command builder source for
  editor-agent cockpit missing-input commands.
- `docs/OUTPUT_SCHEMA.md` defines the Agent Status, Agent Workflow Manifest,
  Agent Receipt, and Agent Review Summary output contracts.
- `.ripr/traceability.toml` maps this spec to tests, code, outputs, and
  metrics.

### Repair-loop state projection (#13382)

Recorded after-phase refusals preserve narration line boundaries and source/path
case; joining a path or colon-ended heading must not invent punctuation or a
different path. When the shared start gate withholds a new attempt, status
removes both fully spelled before-phase recipes and the bare compatibility
repair-start hint from its projected refusal and history. Retained manifests
are not rewritten by inspection. Finished attempt state, gap closure and actual
project-test execution remain separate observations.


## Metrics

### Legacy repo-exposure refusal (#5115)

Producer identity is read from the JSON object (`schema_version` and
`artifact.producer.version`) before the full typed repo-exposure shape is
deserialized. The document is parsed to `Value` once; typed admission uses
that value. Byte-bounded and seam-limited pilot files are classified from
`run_status` / `limitations` before producer-stamp substitution, so a cut
file without an envelope keeps the control-specific recovery rather than a
legacy-binary diagnosis. A missing producer envelope (released 0.10: string
`schema_version`, scope `repo`, seams array, no `artifact` object) or an
older/foreign `artifact.producer.version` on a document with those markers
and `artifact.producer.tool = "ripr"` fails closed with an actionable
substitution diagnostic: it names the producer and schema the artifact
carries, the consuming RIPR version and expected schema, that a different
`ripr` binary likely won `PATH` resolution, and `ripr --version` / use the
same binary. That diagnosis is the first useful failure; it is not a serde
`missing field` error. The diagnostic does not embed an absolute executable
path. A matching current `producer.version` is unchanged: present-but-invalid
envelopes, missing `producer.tool`, malformed JSON, and unrelated documents
keep canonical parsing errors, and current artifacts continue through
existing commitment and root/currentness validation.

The missing-envelope refusal still names the current RIPR contract and a
selected-root regeneration command, writing `recovered.repo-exposure.json`
directly within that root so no workflow directory must already exist. It names
the affected input for replacement without treating a shared-consumer label as
a CLI option. The selected root is bound at production for foreign-CWD replay.
This diagnosis never accepts
legacy evidence or authenticates an executable.
Recovery labels the Bash/Git Bash form and the PowerShell form translated by
the existing Markdown command helper. If that helper refuses translation, the
diagnostic labels the command Bash/Git Bash only and names PowerShell recovery
as unavailable. Root apostrophes and dollar signs remain literal path text.
The printed recovery command runs the executable that issued the refusal
(`std::env::current_exe()`, quoted by the shared shell-argument helper) and the
refusal prints that executable's version line, so a stale `ripr` earlier on
`PATH` cannot satisfy it (#9500). If the operating system cannot name the
running executable the command falls back to bare `ripr` and the refusal says to
confirm `ripr --version`. Only this refusal is rebound; other emitters still
render PATH `ripr`, and installed-binary substitution on every route stays
unqualified.

An envelope whose `artifact.producer.version` is not exactly the consuming
executable's version is refused at the same validation site (#9500) with a
"produced by X, consumer is Y" diagnostic, the substitution clause above, and
the same recovery command. This is fail-closed version equality, not producer
authentication: a document can still declare the current version, and artifacts
from another RIPR build, including a patch release, must be regenerated.
`reports gap-ledger --repo-exposure` is a separate route governed by
RIPR-SPEC-0046 (a missing envelope is disclosed as a warning there, #9570).

- `agent_loop_status_available`
- `agent_workflow_manifest_available`
- `agent_receipt_provenance_available`
- `agent_receipt_next_action_guidance_available`
- `agent_review_summary_available`
- missing artifact count by status report
- stale-looking warning count by status report
- recovered seam source distribution
