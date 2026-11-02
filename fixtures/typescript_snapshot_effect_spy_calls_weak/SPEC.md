# Fixture: typescript_snapshot_effect_spy_calls_weak

Spec: RIPR-SPEC-0258
Spec: RIPR-SPEC-0108

## Given

The same change as `typescript_snapshot_effect_owner_result_control`. The test
snapshots a spy's recorded calls instead of the owner result (#12465):

```ts
const spy = jest.spyOn(console, "log");
enqueue([], 7);
expect(spy.mock.calls).toMatchInlineSnapshot(`[]`);
```

## When

```bash
cargo xtask fixtures typescript_snapshot_effect_spy_calls_weak
```

## Then

A spy's recorded calls are not on the RIPR-SPEC-0258 rule 3 allowlist, so the
seam reads `weakly_exposed`. This is weak by design: rule 3 binds a snapshot to
the owner result only, and resolving a spy to the owner's effect is not
attempted. A spy snapshot that truly pins the change costs a missed credit,
never a false one. Only `expected/check.json` is retained, for the reason given
in `typescript_snapshot_effect_unrelated_subject`.

## Must Not

- Read a snapshot of a spy's calls as the owner result.
