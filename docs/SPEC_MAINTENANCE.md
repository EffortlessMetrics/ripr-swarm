# Specification maintenance inventory

`cargo xtask specs maintenance --as-of YYYY-MM-DD` writes an advisory inventory
to `target/ripr/reports/spec-maintenance.json` and
`target/ripr/reports/spec-maintenance.md`. Add `--json` to also print the same
versioned JSON DTO to standard output. Add `--receipts <dir>` to read review
receipts from a different directory; the default is
`.allow/spec-system/reviews` when it exists, and no receipts directory at all
is the zero-receipt baseline.

The report covers every discoverable spec file in the repository's canonical
`RIPR-SPEC-NNNN-slug.md` shape, parsed by the same identifier rule the spec gates use. Other
Markdown files below `docs/specs/` are listed as omitted, with the reason they
are not part of the discoverable denominator. A missing `docs/specs/README.md`
index is likewise recorded as an omitted input with reason
`spec-index-missing`; unlike present files it does not inflate the
discoverable count, because no document was actually scanned. Reason codes
come from document structure (headings) rather than token presence: a spec
with no review-bearing heading is `never_reviewed`, and an accepted spec
without a `## Test Mapping` heading is
`accepted_without_current_or_planned_test_mapping`. Required-spec read or UTF-8
errors are instrument failures and return a nonzero status.

The denominator keeps the honest arithmetic
`discoverable == included + closed + omitted(non-index-missing)`: findings
closed by a review receipt move from the queue into `closed_specs` and are
counted in `denominator.closed`, and `closure_counts` breaks closed
observations down by disposition label. `status_counts` and `reason_counts`
count only open findings, so a closed spec can neither inflate nor deflate
the queue.

## Review receipts (#3466)

A `SpecReviewReceiptV1` is one committed TOML file per spec under
`.allow/spec-system/reviews/RIPR-SPEC-NNNN.toml` recording that this exact
spec content received a bounded advisory maintenance disposition. Receipts
are content-bound: the SHA-256 digest of the reviewed spec bytes is
authoritative for compatibility, so editing the spec reopens the finding
with reason `content_changed_since_review` even if the document status is
unchanged. An optional `waived_until` date composes with any disposition;
`waived_until == as_of` is still closed, and a past date reopens the finding
with reason `review_waiver_expired`. The semantic `receipt_id` is derived
from the disposition-bearing fields alone, so re-observation can update a
receipt without changing its identity.

Write receipts with
`cargo xtask specs close --spec RIPR-SPEC-NNNN --disposition <label>
--as-of YYYY-MM-DD --reviewed-by <identity> [--waived-until YYYY-MM-DD]
[--detail <text>]`. The writer computes the digest from current spec bytes
itself, links the previous receipt generation as its predecessor, and never
edits the spec file. Closed maintenance dispositions are review labels, not
lifecycle states; the report renders them verbatim with no interpretation.

Receipts are advisory. A rejected receipt (malformed TOML, unknown schema,
wrong spec or path binding, duplicate, unparsable filename) is recorded in
`receipts.rejected` with a named reason and closes nothing; `specs
maintenance` still succeeds. Absence of a receipt never changes a finding's
validity, and no required gate consumes receipts.

Each row includes the spec ID and path, a SHA-256 content digest, observed
document status, objective reason codes, evidence references, a bounded next
route, and limitations. JSON and Markdown are rendered from the same
`SpecMaintenanceReportV1` value, and stable sorting makes fixed repository
bytes plus a fixed `--as-of` value reproducible.

Git history is optional. When it is unavailable, the report says so and keeps
repository-only findings. Age is an observation or ordering hint only; it
never changes spec validity, lifecycle, support posture, branch protection, or
merge eligibility. The report does not create review receipts, publish
digests, alter workflows, or infer that implementation or evidence exists.
