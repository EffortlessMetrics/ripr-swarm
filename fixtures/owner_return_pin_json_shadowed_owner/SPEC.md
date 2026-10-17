# Fixture: owner_return_pin_json_shadowed_owner

Spec: RIPR-SPEC-0197

## Given

Literal JSON construction precedes an ineffective `shadowed_owner` equality.
Only the final callback return changes; an earlier return is preserved.

## When

The exact diff is analyzed and one unignored test executes against correct and
wrong callbacks under the locked registry dependency.

## Then

The current return stays non-exposed and the wrong callback passes the test.

## Must Not

- Grant an oracle from construction alone.
- Credit another binding, unrelated equality, or a different return path.
