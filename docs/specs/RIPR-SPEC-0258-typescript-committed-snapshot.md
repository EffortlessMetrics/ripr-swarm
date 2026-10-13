# RIPR-SPEC-0258: TypeScript committed snapshot entry

Status: proposed

Owner: product / analysis

Created: 2026-10-13

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

The TypeScript verdict corpus reads `jest-spec0234-ex03` as a false gap:

```ts
// src/priceEx03.ts
export function priceEx03(amount: number): number {
  if (amount > 100) {
    return amount - 10;   // before
    return amount - 20;   // after
  }
  return amount;
}

// test/priceEx03.test.ts
test("priceEx03", () => {
  expect(priceEx03(150)).toMatchSnapshot();
});

// test/__snapshots__/priceEx03.test.ts.snap
exports[`priceEx03 1`] = `140`;
```

RIPR-SPEC-0243 reads every `toMatchSnapshot()` as `snapshot` / medium with
`typescript_snapshot_discriminator_unresolved`, and RIPR-SPEC-0247 rule 3
promotes only a literal inline snapshot. The finding stays
`weakly_exposed`, although Jest compares `130` with the committed `140` and
fails.

## Behavior

**Rule 1. A committed entry the assertion compares with pins the value.**
`expect(...).toMatchSnapshot()` with no argument reads `snapshot` /
strong, without `typescript_snapshot_discriminator_unresolved`, when the
runner's entry for it can be named and is committed:

- the snapshot file is `__snapshots__/<test file name>.snap` beside the
  test file, read under the per-file read cap, and its first line starts
  with `// Jest Snapshot v1` or `// Vitest Snapshot v1`;
- no `package.json`, `jest.config.*`, `vitest.config.*`, `vite.config.*`
  or `vitest.workspace.*` in the test file's directory or any ancestor up
  to the root mentions `snapshotResolver`, `resolveSnapshotPath`,
  `updateSnapshot`, `--update`, ` -u`, `"-u"` or `update:`, and each such
  file reads; a moved file or a run that rewrites snapshots no longer fails
  on a changed value;
- with a Jest header, none of those files is a `vite*` config or mentions
  `vitest`: Vitest reads a file Jest left but keys titles with ` > `;
- the file holds no `.concurrent` test, which may record a snapshot under
  another test's title;
- every snapshot matcher call in the test file sits in a test body (text
  `Snapshot(`, counting inline snapshots, which advance the same counter),
  so no hook or helper advances the counter;
- the test body holds exactly one such call, and this assertion is its
  only argument-free `toMatchSnapshot`;
- no other test's describe titles and title join to the same key, which
  would share the counter;
- no title holds `` ` ``, `\`, `$`, a line break or nothing, which the file
  escapes or the runner formats;
- the file holds the entry `<titles> 1` exactly once and no `<titles> 2`,
  where `<titles>` joins the describe titles and the test title with a
  space for Jest and with ` > ` for Vitest. A second entry means the
  assertion ran more than once when the file was written.

A matcher with a hint or a property-matcher argument, `.not`, an inline
snapshot without a literal and every refused shape keep their
RIPR-SPEC-0243 / RIPR-SPEC-0247 reading. The class still follows the
RIPR-SPEC-0243 ladder, including the observation guard.

### Decisions

1. **Read the committed file.** The runner compares the serialized value
   with the committed entry and fails on any difference (`--ci` also fails
   on a missing entry), so the entry is as much a discriminator as a
   literal inline snapshot. This reverses the RIPR-SPEC-0247 rule 3 choice
   to keep a committed file medium because its content is not in the test
   diff: whether a reviewer reads the expected text does not change
   whether the test fails. Rejected: keep medium, which leaves a
   discriminated case false actionable and asks for a test the user
   already wrote.
2. **Name the entry, or refuse.** Only an entry the reader can name from
   the test file reads strong. Rejected: credit any entry under the test's
   title, which credits an assertion a hook's or another call's entry
   satisfies.

### Known limits

- The snapshot file is read from the working tree, as owner and test
  sources are.
- A test registered from a template title (`%s`, `%#`, a non-literal
  describe title) has no matching entry and stays medium. A `test.each`
  with one row and a literal title runs once and can read strong.
- A runner config passed by path (`--config config/jest.js`) or kept above
  the analysis root is not read.
- A stale entry (one that no longer matches the base behavior) already
  fails the test; the reader does not compare entry and behavior.

## Required Evidence

- On the TypeScript verdict corpus, `jest-spec0234-ex03` moves from `gap`
  to `credited` and no other row moves.
- `cargo xtask goldens check` reports no drift.

## Non-Goals

- No reading of the entry's value.
- No change to inline snapshots, `toThrowErrorMatchingSnapshot` or
  `toMatchFileSnapshot`.
- No change to Rust, Python or other languages.

## Acceptance Examples

1. The corpus case: `snapshot` / strong, `exposed`.
2. The same test under `describe("price")` with entry `price priceEx03 1`
   (Jest) or `price > priceEx03 1` (Vitest): strong. A Vitest file keyed
   with a space: medium.
3. No snapshot file, an entry for another test, an unknown header, or a
   second entry `priceEx03 2`: medium.
4. A second snapshot call in the body (inline included), a snapshot call
   in an `afterEach` hook, two tests titled `priceEx03`, tests
   `a b` / `c` and `a` / `b c` (both keyed `a b c`), a `test.concurrent`,
   a hint (`toMatchSnapshot("cap")`), or an empty title or one holding
   `` ` ``, `$` or `\`: medium.
5. A `snapshotResolver` or `updateSnapshot` in `jest.config.js`, a
   `resolveSnapshotPath` or `update: true` in `vitest.config.ts`,
   `vite.config.ts` or `vitest.workspace.ts`, `jest -u`,
   `jest --updateSnapshot`, `vitest --update` or `["-u"]` in a
   `package.json` at the root or in the test directory, or a
   `vitest.config.ts` beside a Jest-headered file: medium. A `jest --ci`
   script: strong.

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/tests/committed_snapshot_tests.rs`:
  `committed_jest_snapshot_entry_reads_strong` pins example 1;
  `committed_snapshot_keys_join_describe_titles_per_runner` pins example 2;
  `snapshot_without_its_committed_entry_stays_medium` pins example 3;
  `snapshot_whose_counter_cannot_be_named_stays_medium` pins example 4;
  `snapshot_location_or_update_override_stays_medium` pins example 5.
- `fixtures/typescript-verdict-corpus` pins example 1:
  `jest-spec0234-ex03` reads `credited`.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/committed_snapshot.rs`:
  `promote_committed_snapshots`, `committed_snapshot_file`,
  `runner_configs`, `joined_title` and `snapshot_key`.
- `crates/ripr/src/analysis/language/typescript/mod.rs`: calls the
  promotion once per extracted test file.

## Metrics

- `typescript_committed_snapshot_mismatch`: acceptance examples whose
  observed reading differs from this spec. Target: zero.
