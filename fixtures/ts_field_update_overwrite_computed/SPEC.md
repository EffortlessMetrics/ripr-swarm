# Fixture: ts_field_update_overwrite_computed

Spec: RIPR-SPEC-0243

Issue: #9515

## Given

`Stats.recordHit` changes `this.hits = this.hits + 1;` to `this.hits += 1;`. The
test calls `stats.recordHit()` on a fresh instance, then overwrites the field
with a computed write `stats["hits"] = 9` before `assert.equal(stats.hits, 9)`. The assertion passes whether
or not the increment runs.

## When

`ripr check --root input --diff ../diff.patch --format json --mode fast`
exercises the production CLI. For an independent runtime check, run the test
with `node --experimental-strip-types --test`, then delete the increment and
run it again: both runs pass.

## Then

The changed update remains `weakly_exposed`. A write the scan cannot see through
the receiver refuses the field-update credit (RIPR-SPEC-0243 decision 18).

## Must Not

- Credit the field update as `exposed` because the read follows an owner call.
- Treat a passing read of an overwritten field as evidence the update is observed.
