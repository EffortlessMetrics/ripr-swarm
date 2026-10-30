# Fixture: ts_field_update_getter_backed

Spec: RIPR-SPEC-0243

Issue: #9768

## Given

`Stats.recordHit` changes `this.hits = this.hits + 1;` to `this.hits += 1;`.
`hits` is an accessor pair: the getter returns a private `stored` value and the
setter clamps it. The test calls `recordHit` once and asserts
`assert.equal(stats.hits, 1)`.

## When

`ripr check --root input --diff ../diff.patch --format json --mode fast`
exercises the production CLI.

## Then

The changed update remains `weakly_exposed`: reading `hits` runs the getter, so
the exact assertion does not observe what the changed statement wrote
(RIPR-SPEC-0243 decision 20).

## Must Not

- Credit a field update whose field the owner's class declares as a getter or
  setter.
