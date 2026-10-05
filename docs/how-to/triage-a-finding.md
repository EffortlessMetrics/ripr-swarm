# Triage a Finding

Use this guide when a `ripr` finding looks surprising. The goal is to decide
whether the changed behavior needs a test, whether the analyzer needs a
follow-up, or whether the team has a bounded, documented exception to carry.

`ripr` reports static exposure evidence. A finding is not a runtime test result,
and a suppression is not a replacement for a missing discriminator.

## 1. Keep the finding identity

Run the check that produced the finding and save its exact `finding_id` and
path. Detailed reports keep suppressed findings visible, so the identifier is
also the durable reference for later review.

```text
ripr check --root . --json --suppression-policy .ripr/suppressions.toml
```

The explicit policy flag is required for suppression-aware findings JSON.
Badge-specific formats load the repository manifest and apply the same
`finding_id` or path-glob (optionally `static_class`-narrowed) matching as
`--suppression-policy`; prefer an exact finding id when the exception applies
to one finding.

## 2. Inspect the evidence

Use `explain` for the human-readable finding and `context` for the compact
related-test packet:

```text
ripr explain --root . <finding-id>
ripr context --root . --at <finding-id> --json
```

Check the changed sink, the related test or oracle evidence, and any stated
static limitation. If the finding describes behavior that should be covered,
add or repair the test. If the analyzer appears to align the wrong entities,
capture the exact finding id, source location, command, and current commit when
filing the analyzer follow-up.

### Read the examined tests

A finding lists the related tests ripr examined (up to a bounded number),
including ones whose assertions did not match. Each line gives the test file, line and name, then why that
test did not count as noticing the change being wrong, or what ripr could not confirm about it:

```text
related test tests/it.rs:3 discount_runs uses none unknown oracle; misses: has no assertion
```

`ripr explain` repeats the list under "Why this verdict". In JSON the same
facts are `related_tests[].miss` (a controlled value) and `related_tests[].why`
(the sentence). Both are omitted when ripr established no miss, for example for
the test that catches an `exposed` change. The reason does not change the
finding's class. Use it to choose a follow-up from the table below. Rust findings carry it today. Python and TypeScript findings do not yet, and
Perl findings carry only `observation_unconfirmed`, on a narrow set of
weakly exposed rows. `no_call_path`,
`no_assertion`, `assertion_not_observing` and `assertion_not_credited` can
appear under any class, because they describe a single test. The other values
appear only under `weakly_exposed` and `reachable_unrevealed`.
On an `exposed` finding, a row with a miss only explains why that test did not
help; act on it only if you are working on that test, because another test
already supplies the discriminator.

| `why` reads | `miss` value | What to do |
| --- | --- | --- |
| no call to the changed code found | `no_call_path` | The test is linked by name or file location only. Check that it is the right test; if it is, call the owner from it. |
| has no assertion | `no_assertion` | Add an assertion on the changed value. |
| asserts, but not on the changed value | `assertion_not_observing` | Move or add the assertion so it observes the changed value, error or field. |
| assertion not credited: ripr could not establish that it runs as the standard macro | `assertion_not_credited` | An assertion exists, but ripr could not confirm it is the standard `assert!` family (a same-named local macro, for example), and it does not yet say whether the assertion is inert. Read the macro. If it expands to a real check, file an analyzer follow-up; if not, use the standard macro. |
| assertion too weak to tell the old behavior from the new | `weak_assertion` | Replace `is_ok`, `unwrap` or a broad comparison with the exact expected value. |
| ripr could not confirm that this assertion observes the changed behavior | `observation_unconfirmed` | This is an unknown, not a found miss: an assertion may already exist. Check by hand whether it observes the changed value before changing the test; if it does, treat it as an analyzer gap and file a follow-up. |
| no test input reaches `<boundary>` | `missing_input` | Add a case whose input is the boundary value named. |
| no assertion pins `<value>` | `missing_exact_assertion` | Assert the exact value named, such as the error variant or field. |

## 3. Choose fix, follow-up, or suppression

- **Fix the behavior gap** when the changed behavior lacks a meaningful test.
- **File an analyzer follow-up** when the evidence is misclassified or the
  static limitation needs product work. Keep the finding visible while that
  work is pending.
- **Suppress only an accepted exception** when the team has reviewed the gap,
  can name its owner and reason, and has a bounded review or expiry date.

Do not suppress a finding merely to improve a badge. Suppressed findings remain
in detailed reports and move only into the suppressed badge bucket.

## 4. Add a durable suppression

The current contract is a hand-authored `.ripr/suppressions.toml` file. Start
from the [suppression example](../suppressions.example.toml), then copy only
the entries that apply into the repository's `.ripr/suppressions.toml`.

Every entry needs:

- `kind = "exposure_gap"` with either an exact `finding_id` or a repository-
  relative `path` glob. A path glob may use `static_class` to narrow the
  matching exposure class, and policy health requires it for exposure-gap
  entries.
- `kind = "test_efficiency"` with `test`; `path` is optional but useful when
  test names repeat. `static_class` is not applicable to this kind because it
  has no exposure class.
- non-blank `owner` and `reason`.

Use `/` in repository-relative paths. Keep selectors narrow, add `expires`
and policy-health dates where appropriate, and avoid unknown fields: the
manifest parser rejects them. Preview-language entries also need
`language_status = "preview"` until the repository policy promotes that
language.

The path-glob example in the example file applies to both explicit
`--suppression-policy` findings runs and implicit badge suppression. For badge
counts, a path-only entry may optionally use `static_class` to narrow the
matching exposure class. Exact `finding_id` entries remain supported and
continue to identify one finding precisely.

## 5. Check suppression health

Run the read-only policy report after editing the manifest:

```text
ripr policy suppression-health --root .
```

Review the generated files:

```text
target/ripr/reports/suppression-health.json
target/ripr/reports/suppression-health.md
```

The report highlights missing ownership or reasons, stale review windows,
overbroad scope, unknown selectors, missing policy metadata, and preview
language metadata gaps. It does not create, apply, delete, or gate
suppressions.

## 6. Re-run the normal check

Run the same check mode used by the workflow or local review. Confirm that the
finding remains visible in detailed output and that only the badge counts move
to the suppressed bucket. Revisit the exception before its `review_by` or
`expires` date; an expired entry no longer applies and is reported as a
warning.

For the full schema and policy-health field definitions, see the
[configuration reference](../CONFIGURATION.md#riprsuppressionstoml). The
repository does not currently provide a `ripr suppress` convenience command;
that is separate follow-up work.
