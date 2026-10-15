# Fixture: typescript_rewritten_local_whole_result

Spec: RIPR-SPEC-0250

Issue: #7497

## Given

The owner returns one object literal whose `adult` field is built from the
changed comparison. The test binds the owner call's result to the local
`bo`, then REWRITES the asserted field itself (`bo.adult = true`) before
asserting the whole result with `toMatchObject`. The assertion therefore
observes the test's own write, not the owner call.

## When

`ripr check --root input --diff ../diff.patch --format json --mode fast`
exercises the production CLI on the changed field construction.

## Then

The changed field construction stays `weakly_exposed`: the #7497 mutation
guard refuses the whole-result one-hop credit for a local the test
rewrites, so the rewritten-field assertion cannot promote the finding to
`exposed`.

## Must Not

- Promote the finding to `exposed` on the strength of an assertion that
  reads the test's own write (`must_not_promote`).
- Report the subject clean: the changed behavior is reached, just not
  witnessed by an honest whole-result assertion.
- Downgrade unrelated whole-result pins: an unmutated local still reads
  `exposed` (pinned by the `rewritten_owner_local_is_not_the_owner_result`
  preservation cases).
