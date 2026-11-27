# RIPR-SPEC-0452: Offline Release Milestone Fixture Comparison

Status: proposed

Owner: release automation

Linked issues: [#4781](https://github.com/EffortlessMetrics/ripr-swarm/issues/4781)
and [#3013](https://github.com/EffortlessMetrics/ripr-swarm/issues/3013).

## Problem

A local snapshot can omit an issue or PR that would change the comparison.
Checking only that all manifest issues survived can miss a dropped page
containing additional records. The fixture format needs evidence of page
continuity and row coverage before the comparator can report agreement.

## Behavior

`cargo xtask check-release-targets --fixture <path>` reads the local fixture
and evaluates `policy/release-targets.toml` with the existing release-graph
validator. The manifest supplies the release roles. The fixture supplies
observations to compare with those roles. The no-argument command retains its
offline integrity check.

### Fixture input

| Field | Contract |
| --- | --- |
| `schema_version` | Integer `2`. |
| `repository` | `EffortlessMetrics/ripr-swarm`, matched ignoring ASCII case. |
| `complete` | Required boolean. `false` refuses comparison. `true` requires a valid page witness. |
| `milestones` | Rows with integer `number`, string `title`, and string `description`. |
| `issues` | Rows with integer `number`, boolean `closed`, `milestone` as an integer or explicit `null`, and string array `labels`. |
| `pull_requests` | Rows with integer `number`, boolean `closed`, `milestone` as an integer or explicit `null`, and integer array `closes` for same-repository issue numbers. |
| `pagination` | One captured page array keyed by each of `milestones`, `issues`, and `pull_requests`. |

Each page has a one-based integer `page`, an explicit `next_page` equal to
the next page number or `null` on the terminal page, and integer array `ids`.
The comparator orders pages by number, requires a contiguous chain from page 1
to the explicit terminal marker, and rejects repeated page numbers, repeated
IDs within a list, and empty pages in a multi-page list. An empty list uses one
terminal page with empty `ids`. The union of captured IDs must agree exactly
with the observed row IDs for that list.

Record numbers are positive and unique within each list. Issues and PRs share
a number namespace. Non-null milestone references must name an observed
milestone, and a PR cannot close issue zero. Unknown fields in the fixture
objects and omitted required fields refuse comparison.

### Comparison and refusal

The comparison requires observed rows for all committed and conditional
issues, rolling issues, and parents marked outside the committed candidate
sets. A missing required issue is unavailable evidence. Other observed rows
can expose drift outside the manifest.

- Match candidates by exact milestone title. Report missing or ambiguous
  titles and any candidate description that differs from the fixed
  source-publication nonclaim.

  > Candidate qualification in `ripr-swarm`; completion does not authorize or establish source publication.

- Committed issues need their release's `target/<version>` label and, when the
  candidate title resolves uniquely, that exact milestone number.
- Candidate milestones exclude conditional and rolling issues, parents marked
  outside the committed sets, and issues without a manifest role. The exact
  `target/conditional` and `target/rolling` labels also signal exclusions.
- An open PR that closes a committed issue needs the issue's uniquely resolved
  candidate milestone. A closed release goal with open committed work drifts.

Unreadable or invalid inputs, incomplete or inconsistent capture, invalid
identities or references, and missing required issue observations produce
`not_comparable`. Once admitted, a comparison with findings produces `drift`.
An admitted comparison without findings produces `in_sync`. Only `in_sync`
returns success.

### Report contract

One result supplies the JSON and Markdown files
`target/ripr/reports/release-milestone-fixture.json` and
`target/ripr/reports/release-milestone-fixture.md`. Report schema version `1`
contains `evidence` set to `local_fixture`, `status`, the manifest SHA-256
when both inputs could be read, the observed list counts when the fixture could
be parsed, and findings with `code` and `detail`. A missing digest or count
set is JSON `null`. Findings are deduplicated and ordered by code and detail.
Reordering equivalent admitted rows, captured pages, or ID lists preserves the
report bytes.

## Required Evidence

Use a complete synthetic fixture with nonempty milestone, issue, and PR
populations. Exercise exact-identity drift and refusal cases with targeted
changes to the fixture. For page loss, retain every required manifest issue
and remove a trailing page with an additional issue. The remaining link to
that page must force `not_comparable`. Repeat a drift fixture with reordered
evidence and compare both report representations byte for byte.

The page witness establishes internal consistency of the supplied fixture.
Its coverage depends on the capture preserving the observed links and IDs.

## Non-Goals

- Live observation, collection, or verification of forge state.
- Candidate qualification or any publication decision or action.

## Acceptance Examples

| Fixture condition | Expected result |
| --- | --- |
| Complete page chains and all observations agree with the manifest | `in_sync` with the observed counts and zero findings. |
| A committed issue has the wrong milestone or an open PR closing committed work has no milestone | `drift` with findings naming the affected rows. |
| An expected candidate title is missing, another is duplicated, and a description differs from the required text | `drift` with separate identity and description findings. |
| The final page is absent, with the preceding page still pointing to it | `not_comparable` even when all manifest issues are present. |
| A page number or captured row ID repeats, the chain breaks, or IDs differ from normalized rows | `not_comparable` with a pagination finding. |
| The witness or explicit terminal marker is absent, the repository is wrong, or a required issue is missing | `not_comparable` with the applicable refusal finding. |

## Test Mapping

In [the comparator tests](../../xtask/src/policy/release_targets/milestone_parity/tests.rs):

- `complete_fixture_has_nonzero_subjects_and_clean_parity` checks the clean result and observed counts.
- `comparable_drift_pins_each_role_and_both_renderings` checks role drift, report agreement, and byte stability.
- `exact_titles_and_unique_milestone_numbers_define_candidates` checks title and description drift.
- `omitted_tail_page_cannot_hide_rows_outside_the_manifest` checks missing-tail refusal with all manifest rows present.
- `pagination_requires_a_contiguous_chain_and_exact_row_coverage` checks page linkage, uniqueness, and row coverage.
- `boolean_alone_and_implicit_terminal_marker_are_refused` checks required pagination and nullable-field presence.
- `incomplete_or_invalid_evidence_cannot_report_parity` checks other evidence refusal paths.

## Implementation Mapping

- [Command dispatch](../../xtask/src/dispatch.rs) selects the fixture route.
- [Release graph validator](../../xtask/src/policy/release_targets.rs) supplies the checked manifest roles.
- [Fixture comparator](../../xtask/src/policy/release_targets/milestone_parity.rs) owns input parsing, page validation, comparison, reports, and exit status.

## Metrics

Track `unit_test_pass_rate` for these controls. The report's milestone, issue,
and PR counts describe the observed fixture population.
