# RIPR-SPEC-0254: TypeScript lookup-table entry

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
- Rust is not changed.

## Problem

The TypeScript verdict corpus reads `ts-vitest-unit-price-table` as a false
gap:

```ts
// src/cart.ts
export async function fetchUnitPrice(sku: string): Promise<number> {
  const table: Record<string, number> = { apple: 120, pear: 95 };   // before
  const table: Record<string, number> = { apple: 120, pear: 99 };   // after
  return table[sku] ?? 0;
}

// test/cart.test.ts
it("looks up a known price", async () => {
  await expect(fetchUnitPrice("pear")).resolves.toBe(95);
});
```

The changed line is a `const` declaration, which the probe classifier does
not recognise. It falls back to a non-specific predicate shape, and an
ambiguous fallback is never `exposed` (RIPR-SPEC-0027), so the finding is
`weakly_exposed` with "no statically comparable predicate boundary".
Reading the line as a plain return-value change would be wrong the other
way: `expect(fetchUnitPrice("apple")).resolves.toBe(120)` passes on both
versions.

## Behavior

**Rule 1. A lookup-table change is a return-value change.** A changed
line that the probe classifier does not already read as a specific shape
reads as a `return_value` / value probe when all of these hold:

- the new and old lines each declare one table on one line,
  `const|let name[: type] = { key: value, ... };`, with the same name and
  the same keys in the same order;
- every key stores the name it spells: an identifier, a plain decimal
  integer, or a quoted word of ASCII letters, digits, `_`, `$` and `-`
  (`1e3`, `0x10` and `__proto__` are refused); no key repeats;
- every value is a plain decimal number (`_` separators and a bigint `n`
  allowed), a quoted string with no quote or escape inside, `true`,
  `false` or `null`;
- at least one value moved. Two numbers compare by value (`95` to `95.0`
  does not move), two strings by contents (`'x'` to `"x"` does not move).
  Under a `??` fallback, a move to or from `null` does not count, since
  `null` reads as the fallback;
- the changed line appears once in the owner, whose body has one `return`,
  no nested function, and names the table exactly twice: at the
  declaration and in that `return`;
- the `return` is `return name[param];` or `return name[param] ??
  <literal>;`, where `param` is an owner parameter the body never rebinds
  (RIPR-SPEC-0248 stability).

The moved keys are the changed keys.

**Rule 2. Credit needs a changed-key lookup.** The change is `exposed`
only when, besides the usual observation guard, a strong `exact_value` or
`whole_object_equality` assertion of an oracle relation observes a whole
owner call (by name, a RIPR-SPEC-0243 rule 7 or RIPR-SPEC-0251 rename, or
a local bound once to that call), and that call's `param` argument is a
quoted string literal naming a changed key. Otherwise the finding is
`weakly_exposed` and names the changed keys.

### Decisions

The owner delegated these choices on 2026-10-04 ("make reasonable
documented decisions and proceed"). Recorded on #6654.

1. **Promote only a proven table.** Adopted: rule 1 recognises the shape
   only with the owner context and the old line, so every other `const x
   = { ... }` keeps the ambiguous fallback. Rejected: classifying every
   object-literal declaration as a value change, which would read option
   objects passed to calls as observed returns.
2. **Key witness, not any exact pin.** Adopted: an exact assertion on an
   unchanged key passes on both versions, so it cannot credit the change.
   This mirrors the RIPR-SPEC-0248 dropped-term witness.

### Known limits

- Tables spread over several lines, nested values, computed keys or
  values, negative numbers, template literals, escaped or non-ASCII
  strings, a table at module level, `Map` tables and `switch` lookups keep
  the ambiguous fallback.
- A key the test computes (`const sku = "pear"`, a template literal) is
  not read, so such a test stays `weakly_exposed` (under-credit).

## Required Evidence

- On the TypeScript verdict corpus, `ts-vitest-unit-price-table` moves
  from `gap` to `credited` and no other row moves.
- `cargo xtask goldens check` reports no drift.

## Non-Goals

- No change to the probe classifier for lines outside rule 1.
- No snapshot or weak-matcher credit for a lookup.
- No change to Rust, Python or other languages.

## Acceptance Examples

1. The corpus case, `await expect(fetchUnitPrice("pear")).resolves.toBe(95)`:
   `exposed`.
2. As example 1 through `const price = await fetchUnitPrice('pear');
   expect(price).toBe(95)`, or through `import { fetchUnitPrice as unit }`:
   `exposed`.
3. As example 1 with `"apple"`, a variable key, a template-literal key, a
   member of the result (`.toFixed(0)`), or `.resolves.toBeGreaterThan(0)`:
   `weakly_exposed`. The strong-assertion cases name `pear` in the missing
   evidence and the discriminate summary.
4. Rule 1 refuses: no old line, an added key, reordered keys, no moved
   value (`95` to `95.0`, `'y'` to `"y"`), a nested, computed or
   concatenated value, a duplicate key, an exponent, hexadecimal or
   `__proto__` key, a key with a space, a move to `null` under `?? 0`, a
   renamed table, a nested arrow, a second `return`, a second read of the
   table, a member read, a computed key argument, a rebound key parameter
   and a computed fallback.
5. With an owner `(region, sku)`, `expect(fetchUnitPrice("eu", "pear"))`
   is `exposed` and `expect(fetchUnitPrice("pear", "apple"))` is not;
   `.resolves.toEqual(95)` is `exposed` as example 1.

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/tests/lookup_table_tests.rs`:
  `lookup_table_change_reads_the_moved_keys_and_the_key_parameter` and
  `lookup_table_change_refuses_other_shapes` pin rule 1 and example 4;
  `lookup_table_entry_is_exposed_when_a_test_looks_up_a_changed_key` pins
  examples 1, 2 and 5; `lookup_table_entry_stays_weak_without_a_changed_key_lookup`
  pins example 3.
- `fixtures/typescript-verdict-corpus` pins example 1:
  `ts-vitest-unit-price-table` reads `credited`.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/lookup_table_guard.rs`:
  `ts_lookup_table_change`, `ts_lookup_table_witnessed`.
- `crates/ripr/src/analysis/language/typescript/classifier.rs`:
  `classify_change_with_alias_state` (probe override and rule 2 gate),
  `ts_owner_call_names` and `ts_observed_owner_call_arguments` (shared
  with `ts_dropped_term_witness`).

## Metrics

- `typescript_lookup_table_entry_mismatch`: acceptance examples whose
  observed reading differs from this spec. Target: zero.
