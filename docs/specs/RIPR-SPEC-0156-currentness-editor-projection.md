# RIPR-SPEC-0156: Currentness editor projection and qualification corpus

Status: proposed

Issue: #3282 (parent #3212; builds on #3280/#3281)

## Problem

The C1/C2 slices gave every finding a producer-owned currentness
disposition and gated the counted, gated, annotated, and diagnostic
surfaces on it — but two editor projections still presented base-deleted
evidence at its recorded coordinate without a revision marker, and the
adversarial currentness matrix existed only as in-crate unit tests, with
no end-to-end corpus a downstream consumer could run against.

## Behavior

- LSP code lenses gate on
  `Finding::is_candidate_actionable` (RIPR-SPEC-0152): a base-deleted
  finding's recorded line is the projected new-side coordinate (the
  #3212 incident shape), and pinning an advisory lens there presents
  deleted-side evidence at an impossible candidate position. A
  candidate-current finding keeps its lens. The full-profile diagnostic
  remains historical-context by design and keeps its revision label.
- The currentness matrix corpus pins, end to-end:
  - **deleted function tail** — removed-only probes are `base_deleted`,
    retained as labelled base-side evidence, never a head-actionable
    gap;
  - **reused coordinate with a different expression** — replacement
    pairing seeds only the added expression's `candidate_current`
    probe; identity is expression-addressed, so a deleted expression
    never merges into a same-line different-expression finding;
  - **whole-helper move without same-text evidence** — no resolved
    move is claimed; the records stay `base_deleted` base-side
    evidence;
  - **whole-file delete (mixed cell)** — the deleted file contributes
    no probe, finding, or repair target (disclose-not-probe, disclosed
    as a named non-claim) while the changed production owner in the
    same diff stays `candidate_current` and actionable.
  - **deleted Python statement** (#7587) — a removed Python behavior
    line that no added line pairs with (a pure deletion, or a code
    line whose comment pairs by content with a re-added comment) is a
    removed-only probe on the owner that held it, `base_deleted`
    (`moved_or_renamed` when the same text is re-added), never nothing
    and never a candidate edit target. A line deleted in the same
    removed run as its own `def` or `class` records nothing; a removed
    declaration, comment, docstring, import or lone bracket records
    nothing. A deleted statement that spans lines (an open bracket, a
    triple-quoted string, a trailing backslash) is one record on the
    line that starts it, or on its first behavior line when it starts
    with a lone opening bracket; its continuation lines record nothing
    and do not decide which owner held it (#7690). Where a statement
    starts and ends is read from the old file. Lines deleted from inside a statement the
    candidate keeps (an argument of a kept call, a line of a kept
    string) are one record on the first unpaired behavior line among
    them.
    Base-side lines are not counted as candidate lines.
  - **deleted TypeScript statement** (#7689) — the same rule for
    TypeScript and JavaScript: an unpaired removed behavior line is a
    removed-only probe on the narrowest live owner that held it. A line
    deleted in the same removed run as its own function, method, class
    or named arrow function records nothing, whether its header sits on
    one line or wraps its parameters across several; a removed callback,
    call argument or control-block header leads on to the enclosing
    owner, and a header
    an added header replaces (a type-only edit) keeps its owner live. A
    removed import, re-export, comment or closing bracket records
    nothing, and a module-level deletion has no owner and records
    nothing.

## Required Evidence

- The lens reproduction (base-deleted finding received a lens at its
  projected coordinate on main) and its discriminating fix; the lens
  view identity now mirrors the visible set, so non-actionable-only
  changes do not trigger a spurious refresh.
- The four corpus fixtures with real golden outputs (the
  downstream-consumable export pattern).
- Existing C1/C2 recurrence tests remain green.

## Required guards

- A lens never anchors a non-candidate-actionable finding.
- Replacement pairing never merges identities across different
  expressions.
- Movement resolution requires same-text evidence; absence stays
  `base_deleted`.

## Acceptance Examples

- Accept: a base-deleted probe — no lens, no diagnostic, no
  annotation, no gap; labelled evidence retained in check JSON and
  human-full.
- Accept: a replacement at one line — one `candidate_current` probe
  with the added expression's identity.
- Accept: a deleted Python statement in a live owner — one
  `base_deleted` record on that owner; an even replacement block — no
  base-side record, since every removed line pairs with an added one.
- Reject: a resolved-move claim without same-text evidence.

## Test Mapping

`lsp/lens.rs` `currentness_lens_tests`;
`analysis/language/python/deleted_lines_tests.rs`;
`analysis/language/typescript/deleted_lines_tests.rs`; fixtures
`currentness_matrix_{tail,reuse,move,whole_delete,python_deleted_statement,typescript_deleted_statement}`.

## Non-Goals

- No re-coordination of recorded locations (deferred with #3280).
- No rename-map retention in the diff parser.
- No downstream release action; the exported fixtures are the
  producer-side evidence for the post-release removal.

## Implementation Mapping

- `lsp/lens.rs` — the eligibility filter.
- `analysis/language/python/deleted_lines.rs` — Python removed-only
  probes.
- `analysis/language/typescript/deleted_lines.rs` — TypeScript and
  JavaScript removed-only probes.
- `fixtures/currentness_matrix_*` — the corpus.

## Metrics

No new metric; lens counts now reconcile with the diagnostic scope by
construction.
