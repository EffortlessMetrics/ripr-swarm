# Golden Output Changes

## Pending

Reason:
RIPR-SPEC-0068: emit `suggested_test.proposed_target` (`null` when the producer
has no governed InlineUnit/Integration new-test proposal) on working-set review
cards so the gate can project a distinct Proposed repair target (#3162).

Command:
`RIPR_UPDATE_FIXTURES=1 cargo test -p ripr --lib output::review_comments::tests::review_comments_pr_guidance_fixtures_pin_required_cases -- --exact`

Updated:
- `capped/comments.json`
- `changed-test-skip/comments.json`
- `exact-line/comments.json`
- `owner-function-line/comments.json`
- `same-file-line/comments.json`
- `summary-only/comments.json`

## Previous

Reason:
SPEC-0068: add gap_state and receipt_command to every working-set card; replace
free-text summary_reason strings with closed-vocabulary constants
(inline_comment_cap_reached, no_safe_changed_line_placement,
navigation_only_cross_language_target); add why_not_actionable and non_claims to
static_limitation cards.

Command:
`RIPR_UPDATE_FIXTURES=1 cargo test -p ripr review_comments_pr_guidance_fixtures_pin_required_cases`

Updated:
- `capped/comments.json`
- `capped/comments.md`
- `changed-test-skip/comments.json`
- `changed-test-skip/comments.md`
- `configured-off/comments.json`
- `configured-off/comments.md`
- `exact-line/comments.json`
- `exact-line/comments.md`
- `owner-function-line/comments.json`
- `owner-function-line/comments.md`
- `same-file-line/comments.json`
- `same-file-line/comments.md`
- `summary-only/comments.json`
- `summary-only/comments.md`
