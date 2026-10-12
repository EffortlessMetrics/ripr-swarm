# RIPR-SPEC-0251: TypeScript default-import owner local

Status: proposed

Owner: product / analysis

Created: 2026-10-12

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #6654 (TypeScript corpus cases scored false after RIPR-SPEC-0243)
- #4103 (default-import and dynamic-import owner credit)

Linked PRs:

- #6686 (TypeScript verdict corpus)

Support-tier impact:

- No tier change. TypeScript stays Preview.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. No new oracle kind, strength, class, relation
  reason or limitation name.
- Amends the RIPR-SPEC-0243 rule 7 scope, its non-goal on default-import
  locals, decision 3 and example 17, and adds rule 2's destructuring
  shadow to the rule 7 alias-rename local.
- Rust is not changed.

## Problem

The TypeScript verdict corpus reads `jest-spec0234-ex17-renamed` as a
false gap:

```ts
// src/priceRenamed.ts
export default function priceRenamed(amount: number): number {
  if (amount > 100) {
    return amount - 10;        // before
    return amount - 20;        // after
  }
  return amount;
}

// test/priceRenamed.test.ts
import cost from "../src/priceRenamed";
test("cost", () => {
  expect(cost(150)).toBe(140);
});
```

`cost` is the module's default export, so `cost(150)` calls the owner and
the exact-value pin fails for the change. The test already relates to the
owner through `ImportedOwnerCall`. The RIPR-SPEC-0098 observation guard
then asks whether the observed expression names the owner. It looks for
`priceRenamed` and for `import { owner as local }` locals (RIPR-SPEC-0243
rule 7), not for a default-import local, so it reads `cost(150)` as an
unrelated expression and the finding is `weakly_exposed` with
`propagation_unknown`.

## Behavior

**Rule 1. A renamed default import of the owner counts as the owner.** In
the value branch of the observation guard, and in the RIPR-SPEC-0248
dropped-term witness, the observed expression also references the owner
when it names, as a whole identifier, the `local` of an
`import local from "<module>"` in the test's file, when all of these hold:

- the owner is that module's default export (`exported_as_default`, the
  RIPR-SPEC-0243 rule 5 proof: `export default function price`,
  `export default price;`, `export { price as default }` or
  `module.exports = function`);
- the test relates to the owner through `ImportedOwnerCall`;
- the import binds the module's `default` (`import cost from`, or
  `import { default as cost } from`), not a namespace or a `require`, and
  `local` differs from the owner's name;
- the import's module resolves to the owner's file, by the same path and
  alias-map match that gives `ImportedOwnerCall`;
- the test body calls `local(`;
- neither the test body nor an enclosing scope declares `local`, and no
  body-local destructuring declaration names it (rule 2).

The one-hop initializer accepts the same local, as rule 7 does for an
alias rename. The relation itself is unchanged: the related-test row keeps
`import_path_affinity`.

**Rule 2. A destructured local is shadowed.** A body-local destructuring
declaration whose pattern names a renamed import local
(`const { cost } = helpers`, `const { price: cost } = helpers`,
`const [cost] = fns`) rebinds it, so the local earns no owner credit. This
applies to the rule 1 default-import local and to the RIPR-SPEC-0243
rule 7 alias-rename local. Any mention in the pattern counts: a renamed
local has no destructuring that re-anchors it to the owner, unlike the
owner's own name (#4103 shape 1).

### Decisions

The owner delegated these choices on 2026-10-04 ("make reasonable
documented decisions and proceed"). Recorded on #6654.

1. **Credit the observation guard, not the relation.** Adopted: the guard
   is what withheld credit, and the import record already proves the
   binding. Rejected: upgrading the relation to `direct_owner_call` as
   RIPR-SPEC-0102 does for an alias rename, which would change related-test
   rows, confidence and goldens beyond this false gap.
2. **Amend RIPR-SPEC-0243's non-goal.** Adopted: that non-goal rested on
   RIPR-SPEC-0102 leaving default imports out of scope, but RIPR-SPEC-0243
   rule 5 now proves a default import binds the owner only when the owner
   is the default export. Rule 1 reuses that proof. Rejected: keeping the
   non-goal, which leaves a direct exact-value pin `weakly_exposed`.
3. **Same-module default exports only.** Adopted: RIPR-SPEC-0095 does not
   follow a default import through a re-export chain, so neither does
   rule 1.

### Known limits

- A default import re-exported through a barrel (`export { default as
  cost } from "./priceRenamed"`) is not followed.
- `require` and dynamic `import()` bindings are not covered.
- The shadow checks rule 1 shares with the same-name and alias-rename
  paths read only line-start declarations, enclosing `describe`
  declarations and rule 2's destructuring. A catch, function or for-of
  parameter, a later declarator (`const x = 1, cost = ...`), a member
  call (`helper.cost(150)`), a `typeof cost` observation and a
  `spyOn(ns, "default")` fabrication can still read `exposed` on all
  three paths (#7507).

## Required Evidence

- On the TypeScript verdict corpus, `jest-spec0234-ex17-renamed` moves
  from `gap` to `credited` and no other row moves.
- Every rule 1 condition and rule 2 is held by a test that fails when the
  condition is removed.
- `cargo xtask goldens check` reports no drift.

## Non-Goals

- No change to the relation kind or the related-test row for a default
  import.
- No credit for a namespace import's `default` member
  (`ns.default(150)`).
- No change to Rust, Python or other languages.

## Acceptance Examples

Base fixture: RIPR-SPEC-0243's `src/lib.ts` with
`export default function price(...)`.

1. `import cost from "../src/lib"` and `expect(cost(150)).toBe(130)`:
   `ImportedOwnerCall`, `exposed` (RIPR-SPEC-0243 example 17 read
   `weakly_exposed`).
2. As example 1 with `price` a named export and `round` the default
   export: `cost` binds `round`, not `exposed`.
3. As example 1 with the test body declaring `const cost = ...`: not
   `exposed`.
4. As example 1 with the strong assertion on `other(150)`, where `other`
   is the default import of another module: not `exposed`.
5. As example 1 with `const { cost } = helpers` in the test body: not
   `exposed` (rule 2). The same holds for `import { price as p }` and
   `const { p } = helpers`.
6. As example 1 with `const r = cost(150); expect(r).toBe(130)`:
   `exposed`, as RIPR-SPEC-0243 example 19 reads an alias rename.
7. RIPR-SPEC-0248's `price(subtotal, fee)` fixture as a default export,
   with `fee` dropped: `expect(cost(6000, 7))` is `exposed`;
   `expect(cost(6000, 0))` is `weakly_exposed` with `fee` named as 0 for
   every tested input, directly or through a one-hop local.

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/tests/spec0243_owner_tests.rs`:
  `rule7_default_import_rename_counts_as_the_owner` pins example 1;
  `rule7_default_import_rename_negatives_keep_no_credit` pins examples 2
  to 5 with each case's relation;
  `default_import_owner_locals_keeps_only_the_owner_binding` pins rule 1's
  conditions and rule 2 on the helper;
  `rule7_default_import_local_initializer_is_an_owner_observation` pins
  example 6; `rule7_alias_local_destructured_in_the_body_is_not_the_owner`
  pins rule 2 for the alias-rename local.
- `crates/ripr/src/analysis/language/typescript/tests/dropped_term_tests.rs`:
  `dropped_term_reads_a_renamed_default_import` pins example 7.
- `fixtures/typescript-verdict-corpus` pins example 1:
  `jest-spec0234-ex17-renamed` reads `credited`.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/related_tests.rs`:
  `default_import_owner_locals`, `local_destructured_in_test_body`
  (also used by `import_alias_owner_locals`).
- `crates/ripr/src/analysis/language/typescript/classifier.rs`:
  `ts_changed_value_is_observed` and `ts_dropped_term_witness`.

## Metrics

- `typescript_default_import_local_mismatch`: acceptance examples whose
  observed reading differs from this spec. Target: zero.
