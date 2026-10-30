# Fixture: ts_field_update_table_zero_rows

Spec: RIPR-SPEC-0243

Issue: #9534

## Given

`Stats.recordMiss` changes `this.misses = this.misses + 1;` to `this.misses += 1;`.
The test iterates a literal table whose rows are `[0, 0]` and `[0, 0]`, calling
`recordMiss` `calls` times before `assert.equal(stats.misses, want)`. No row
calls the method, so the test passes with or without the update.

## When

`ripr check --root input --diff ../diff.patch --format json --mode fast`
exercises the production CLI.

## Then

The changed update remains `weakly_exposed`: no row calls the owner at least
once and expects a nonzero value, so the table does not discriminate the update
(RIPR-SPEC-0243 decision 19).

## Must Not

- Credit a table-driven expected side whose rows never pair a call with a
  nonzero expectation.
