# Evidence-bound review resolution (#8157)

## Delivery boundary

This is a staged producer/consumer capability in the existing trusted
`review_disposition.py` reader. It is selected with
`--require-explicit-resolution`. Existing callers retain the #14578 chronology
contract; this change does **not** activate stricter admission or change an
existing queue entry's contract. The selector belongs to the immutable trusted
caller, never to a PR label, body or candidate-controlled setting.

Activation requires a coordinated caller generation at both initial admission
and the final Result recheck, including a plan for queued heads whose reviews
were written under the old contract. The lifecycle owner must qualify that
migration before claiming enforcement. The parent #14584 remains the separately
qualified precedence repair. No additional workflow or service is introduced.

## What the stronger mode establishes

A still-present trusted current-head negative COMMENT cannot be cleared by a
later generic ready COMMENT, native APPROVED, or positive PR description. The
clearing record must name the original native review ID and its exact body
digest, the repository, PR number and full head SHA. The same authenticated
GitHub account must submit the record later than the objection. A GitHub account
is not a worker identity: this does not establish independent-agent review.

The supported dispositions are `refuted` and `retracted`, for an alleged defect
that does not apply to the unchanged source. Each needs an explicit rationale
and at least one source excerpt fetched from the exact candidate SHA. The
consumer verifies the source Git blob hash, excerpt digest and bounds. It does
not execute candidate files or follow an arbitrary evidence URL. This binding
establishes which source the adjudication refers to; it does not prove that the
rationale is true or that a test ran. Native `CHANGES_REQUESTED` and thread
resolution remain separate controls with their existing contracts.

A valid resolution supplies **no readiness attestation**. Review readiness still
has to be earned and carried separately. A new exact-head objection refuses
again, even when an earlier objection was validly resolved. It must receive its
own explicit disposition. Changing the target body or evidence invalidates the
binding. A malformed resolution record also refuses: correct that record; a
fresh positive marker or a resolution targeting the malformed record does not
make it valid.

## Producing a record

Use a trusted checkout of the reader and the original negative review's numeric
REST review ID. For example, after actually adjudicating the finding:

```sh
python3 .github/actions/pr-candidate-admission/review_disposition.py \
  --repository OWNER/REPOSITORY --pr PR_NUMBER --head FULL_HEAD_SHA \
  --format-resolution NEGATIVE_REVIEW_ID \
  --resolution-disposition refuted \
  --rationale 'Explain why this exact source refutes the named finding.' \
  --source-path src/example.rs --start-line 10 --end-line 18
```

The formatter reads the review, PR and source and prints an **unpublished**
record. It neither posts a review nor approves anything. Inspect it, then submit
it as a native COMMENT review on that exact head from the account that issued
the objection. Post the independently earned source-ready attestation after the
record, or use the existing exact-head description carrier; the existing latest
COMMENT selection rules still apply. Do not move H merely to evade a finding.

The whole review body is this exact header followed by JSON:

```text
RIPR_REVIEW_RESOLUTION_V1
```

The JSON object has exactly these fields:

| Field | Value |
| --- | --- |
| `schema` | `ripr.review-resolution.v1` |
| `repository` | Exact `owner/repository` from the PR |
| `pull_request` | Positive integer PR number |
| `head_sha` | Full current 40-character SHA |
| `resolutions` | 1–16 explicit dispositions |

Each disposition has exactly `review_id`, `body_sha256`, `disposition`,
`rationale`, and `source_evidence`. The body digest is SHA-256 of the original
native review body's UTF-8 bytes. Each source evidence object has exactly `path`,
`blob_sha`, `start_line`, `end_line`, and `excerpt_sha256`. Lines are one-based,
inclusive; the excerpt digest covers the exact selected UTF-8 bytes, including
line endings. The Git blob hash includes the standard Git blob header.

Unknown/duplicate fields, unknown versions, duplicate target IDs, missing
subjects, wrong accounts/heads, stale bodies, unsupported dispositions and
unavailable evidence refuse. Records are capped at 64 KiB, each rationale at
4,000 characters, and each disposition at four excerpts of at most 200 lines.
Source paths must be canonical repository-relative paths. The consumer allows
at most 16 unique source reads, caches failures too, and accepts only UTF-8
regular files up to 1 MiB; each read has a 30-second timeout. Source is fetched
only from the CLI's repository and H. The PR must be same-repository.

## Limits and remaining work

This is not a general execution-evidence receipt or a durable finding ledger.
The native API snapshot cannot establish objections whose bodies were deleted,
edited away, or dismissed before the snapshot. That needs retained finding and
currentness semantics; no append-only history guarantee is claimed here.

This version deliberately does not clear objections with `repaired`, another
head's proof, job URLs, thread `isResolved`, or self-reported worker identity.
Execution-backed adjudication requires the existing receipt producer/consumer
owners to supply and validate an appropriate current proof reference first.
Ordinary source repair continues on a new head with fresh review and proof.

The parser's prose vocabulary remains separate work. A historical red/green
sentence mistaken for a current defect can be explicitly refuted here, but
this does not fix that classifier or claim every negative statement is found.
The #14585 historical-test wording incident was reported by its owner; the
controls here exercise an explicit negative marker and source-bound retraction,
not an independent reproduction of that live incident.

## Local qualification

`test_review_resolution.py` executes the actual CLI against a bounded fake
GitHub transport. It checks producer/consumer round trips, generic positivity,
renewed objections, mutable records, authority, exact subject/source bindings,
malformed inputs, read failure caching and the separate native veto. It is
invoked by the existing repo-policy admission integration test. No hosted
qualification or production activation is implied by these local tests.
