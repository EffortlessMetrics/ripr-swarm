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
`expect(...).toMatchSnapshot()` with no argument, or under Jest with one
hint argument (rule 2), reads `snapshot` /
strong, without `typescript_snapshot_discriminator_unresolved`, when the
runner's entry for it can be named and is committed:

- the snapshot file is `__snapshots__/<test file name>.snap` beside the
  test file, read under the per-file read cap, and its first line starts
  with `// Jest Snapshot v1` or `// Vitest Snapshot v1`;
- no `package.json`, `jest.config.*`, `vitest.config.*`, `vite.config.*`
  or `vitest.workspace.*` in the test file's directory or any ancestor up
  to the root mentions `snapshotResolver`, `resolveSnapshotPath`,
  `snapshotEnvironment`, `updateSnapshot`, `snapshotFormat`, `--update`, `-u`
  after any whitespace (a JSON string's `\t`, `\n`, `\r`, `\f`, `\v` or
  `\u0020` escape counts as whitespace) or `"-u"`, no runner config holds an
  `update` key (`update:`, `update :`, `"update":`, a computed `["update"]:`,
  a member assignment `config.update = ...`), no runner config other than
  `package.json` names `projects`, `workspace` or `root` as a key (plain,
  quoted, computed `["projects"]:`, JS shorthand, or a member assignment
  `config.projects = ...`), since those keys delegate to config files this
  walk does not read, no `package.json` passes a config, project list or root
  by path (a script flag `--config`, `--config=`, `-c`, `-cx`, `--projects`,
  `--rootDir`, `--root`, `-r`, `--dir` or `--workspace`, or a `"projects"` or
  `"rootDir"` key), and each such file reads; a `<stem>.config.<ext>` whose
  stem has a `.`, `-` or `_` separated part `jest`, `vitest` or `vite`
  (`jest.unit.config.js`) is read the same way; a moved file or a run that
  rewrites snapshots no longer fails on a changed value. The walk continues
  above the analysis root, starting from the canonicalized root so a
  symlinked root climbs its real parents, up to the first directory holding
  a repository marker (at most 16 levels; a submodule's `.git` file, whose
  `gitdir:` target has a `modules` path component, is not one, so the walk
  continues to the superproject's marker), or to the filesystem root when
  that is within 16 levels and no marker is found (nothing above is a
  repository, and any config there is still read). When 16 levels pass with
  neither a marker nor the filesystem root, the repository boundary is
  unknown (an export without `.git` inside a larger tree, or a deep nest) and
  the reader refuses. A symlinked directory inside the root, and a symlinked
  `__snapshots__` directory, are refused;
- the header agrees with the runner those files name: a Vitest header
  needs a `vite*` config or a mention of `vitest`, and a Jest header needs
  neither. The header names the runner that wrote the file, and the other
  runner keys titles with its own separator;
- the file holds no `.concurrent` test, which may record a snapshot under
  another test's title;
- in a committed-history run (the default diff), neither the snapshot
  file nor any `package.json` or runner config in those directories differs
  from `HEAD` (edited, deleted, untracked or ignored): the test file is read
  at `HEAD`, so its entry must be the one `HEAD` holds;
- every snapshot matcher call in the test file sits in a test body (text
  `Snapshot`, then after optional whitespace, block comments and `//` line
  comments (ending at LF, CR, U+2028 or U+2029) `(`, `<`, `!`, `?.`,
  `.call`, `.apply`, or a quoted bracket access closed by `]`, counts one;
  counting inline snapshots, which advance the same counter; comments and
  strings only over-count, which refuses). A matcher-shaped name (a word
  starting `toMatch` or `toThrowErrorMatching` and ending `Snapshot`) used any
  other way is a value reference that may be called any number of times
  later, and counts as two so it is never credited as the one call: the
  parenthesised `(a.toMatchSnapshot)()`, a cast `a.toMatchSnapshot as any`,
  `satisfies`, an alias `const f = a.toMatchSnapshot; f()`, an argument or a
  destructuring (#8895). So does `.bind`, which stores the callable (`const f
  = a.toMatchSnapshot.bind(null); f(); f()`). This is refuse-on-presence
  rather than call tracking: the lexical counter does not follow `f()`, so the
  reference itself refuses, and so do an `import { toMatchSnapshot }` and a
  comment or string naming the matcher (a lexical counter cannot tell them
  from a use). A word that only begins with `Snapshot`, a standalone
  `Snapshot` and other `...Snapshot` names (`getSnapshot`, `type
  FooSnapshot`) count zero unless called; a matcher name followed by a
  non-ASCII identifier part such as U+0301 or U+200C is read as a value
  reference and refuses,
  so no hook or helper advances the counter;
- the test body holds exactly one such call, and this assertion is its
  only `toMatchSnapshot`, with no argument or with the rule 2 hint;
- no other test's describe titles and title join to the same key, or to a
  `<titles>` that the key extends with `: ` under Jest or ` > ` under
  Vitest, since that test's call may carry a hint that makes the same key
  and would share the counter;
- no title holds `` ` ``, `\`, `$`, a line break or nothing, which the file
  escapes or the runner formats;
- the file holds the entry `<titles> 1` exactly once, at the start of a
  line as the runner writes it, and no `<titles> 2`,
  where `<titles>` joins the describe titles and the test title with a
  space for Jest and with ` > ` for Vitest, followed under rule 2 by
  `: <hint>`. A second entry means the assertion ran more than once when
  the file was written.

**Rule 2. A Jest hint names its entry.** Jest's
`toMatchSnapshot(hint)` records its entry under `<titles>: <hint>
<counter>`, with the counter kept per hinted name (jest-snapshot 30.2.0
builds the name as `` `${currentTestName}: ${hint}` `` when the hint is
non-empty and counts per that name in `SnapshotState.match`). A call reads under rule
1 with that key when the file's header is `// Jest Snapshot v1` and its
one argument is a single- or double-quoted string literal, followed by
`)` after optional whitespace, that is not empty (Jest drops an empty
hint) and holds no `` ` ``, `$`, `\` or line break, so its source text is
its value and the file writes it unescaped. The body must name
`toMatchSnapshot` once, so the hint read is the credited call's even when a
string or comment also names the matcher. A template, an identifier, a
concatenation, a comment before the literal, a second argument, and every
Vitest hint keep the RIPR-SPEC-0243 reading.

A matcher with a property-matcher argument, any other hint, `.not`, an inline
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

- The snapshot file and configs are read from the working tree. A
  committed-history run refuses when one differs from `HEAD`; a
  `--worktree` run reads the working tree throughout.
- A test registered from a template title (`%s`, `%#`, a non-literal
  describe title) has no matching entry and stays medium. A `test.each`
  with one row and a literal title runs once and can read strong.
- A runner config passed by path is not read; a `package.json` script that
  passes one refuses instead. A config that names `projects`, `workspace` or
  `root` is not followed into the files it names; it refuses instead. A
  computed key built at run time (`["pro" + "jects"]`) is not read and a
  delegating config written that way keeps the credit. A config above the
  repository boundary is not read.
- A `.git` directory nested in a larger repository (a vendored clone) cannot
  be told from the repository top, so it ends the walk and a config above it
  is not read. A worktree `gitdir:` pointer and a `.git` file with no
  readable pointer end the walk the same way; only a submodule pointer is
  skipped.
- The analysis root itself may be a symlink (it is the user's own checkout
  path and is canonicalized), but every directory below it, and the
  `__snapshots__` directory, must not be; the asymmetry is deliberate.
- Only a `.git` entry is a recognized repository boundary. A Mercurial,
  Subversion, Jujutsu or pnpm-workspace root without `.git` is not one: the
  walk continues past it and reads ancestor configs, and refuses (medium) if
  16 levels pass without a `.git` or the filesystem root.
- The 16-level reach is a bound on the filesystem reads per file, chosen to
  exceed the depth of ordinary monorepo package nesting (`packages/x/y/src`)
  while keeping the walk cheap; deeper nests refuse rather than read on.
- `passes_config_by_path` over-matches (`--configdir`, a `node -r` preload);
  over-matching only refuses, so it costs credit, never adds it.
- Windows junctions and `\\?\` extended paths are untested; the symlink
  checks are `lstat`-based and a junction may not read as a symlink there.
- Configs reached through environment variables or `.yarnrc` indirection are
  not seen.
- Configs above the analysis root are read from the working tree, including
  in a committed-history run.
- Scripts that call a shell script or Makefile, and configs reached
  through them, are not seen.
- A committed-history run does not compare configs above the analysis root
  with `HEAD`.
- An untracked or ignored `.snap` file is refused only in a
  committed-history run; a `--worktree` run still reads the working tree
  (#7708 item 3).
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
3. No snapshot file, an entry for another test, an unknown header, a
   second entry `priceEx03 2`, or the entry only inside a comment:
   medium.
4. A second snapshot call in the body (inline included), a snapshot call
   in an `afterEach` hook, two tests titled `priceEx03`, tests
   `a b` / `c` and `a` / `b c` (both keyed `a b c`), a `test.concurrent`,
   or an empty title or one holding `` ` ``, `$` or `\`: medium. A Jest
   hint `toMatchSnapshot("cap")` (or `'cap'`) in test `priceSnapHint`
   with the entry `priceSnapHint: cap 1`: strong. The same call against
   only `priceSnapHint 1`, `priceSnapHint: cape 1` or
   `priceSnapHint cap 1`, or with a second entry `priceSnapHint: cap 2`;
   a template, identifier, concatenated, empty or `$`- or `\`-holding
   hint; a second test titled `priceSnapHint` or `priceSnapHint: cap`;
   a string naming `toMatchSnapshot` beside the call; or a Vitest file:
   medium. Under Vitest, `describe("price")` / `test("cap")` beside a
   top-level `test("price")` reads medium, as both may key `price > cap`.
5. A `snapshotResolver` or `updateSnapshot` in `jest.config.js`, a
   `resolveSnapshotPath`, `update: true` or `update : true` in
   `vitest.config.ts`, a `snapshotEnvironment` or `"update": true` there,
   the same in `vite.config.ts` or `vitest.workspace.ts`, `jest -u`,
   `jest --updateSnapshot`, `vitest --update`, `"jest\t-u"` or `["-u"]` in a
   `package.json` at the root or in the test directory, a
   `vitest.config.ts` beside a Jest-headered file, or a Vitest-headered
   file with only a Jest `package.json`: medium. A `jest --ci` script, a
   `package.json` script named `update`, or a plain `vitest.config.ts`
   beside a Vitest-headered file: strong.
6. In a committed-history run, an edited or untracked snapshot file, or an
   edited `package.json` or runner config in the test's directory or an
   ancestor: medium. A `toMatchSnapshot ()` call with a space before `(`
   counts as a snapshot call.
7. A `package.json` script `jest --config config/jest.js` (or `-c`,
   `--config=`), or a `jest.unit.config.js` with a `snapshotResolver`: medium.
   A `jest.unit.config.js` that overrides nothing, or a `jest --ci
   --coverage` script: strong. A `jest.config.js` with a `snapshotResolver`
   above the analysis root, inside the repository: medium; the same file
   when no repository marker and no filesystem root is within 16 levels
   above the root: medium (boundary unknown); a Vitest `vitest.unit.config.ts`
   that overrides nothing: strong; a symlinked test directory or symlinked
   `__snapshots__` directory: medium.
8. A `jest.config.js` with `projects: [...]` or `["projects"]: [...]`, a
   `vitest.config.ts` with `workspace`, `root` or `config.test.root = ...`:
   medium. A `const root = process.cwd()` declaration, a Jest `roots` or
   `rootDir` option: strong. A package whose `.git` file is a submodule
   pointer (`gitdir: ../../.git/modules/app`) under a superproject whose
   `jest.config.js` sets a `snapshotResolver`: medium; the same package with a
   worktree pointer (`gitdir: ../.git/worktrees/app`): strong.

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/tests/committed_snapshot_tests.rs`:
  `committed_jest_snapshot_entry_reads_strong` pins example 1;
  `committed_snapshot_keys_join_describe_titles_per_runner` pins example 2;
  `snapshot_without_its_committed_entry_stays_medium` pins example 3;
  `snapshot_whose_counter_cannot_be_named_stays_medium`,
  `committed_jest_hint_entry_reads_strong`,
  `hinted_snapshot_without_its_exact_entry_stays_medium` and
  `hinted_snapshot_sharing_its_counter_stays_medium` pin example 4;
  `snapshot_location_or_update_override_stays_medium` pins example 5;
  `snapshot_input_differing_from_head_stays_medium` pins example 6;
  `generic_or_bracket_snapshot_calls_are_counted_and_refuse` pins the
  counted spellings of rule 1; `line_comment_value_reference_and_alias_snapshot_spellings_refuse`
  and `snapshot_calls_counts_each_spelling`,
  `non_matcher_snapshot_names_do_not_refuse_an_honest_call` and
  `line_comment_ending_in_any_line_terminator_keeps_honest_call_strong` pin the
  #8895 spellings;
  `config_passed_by_path_or_custom_named_stays_medium`,
  `config_above_the_analysis_root_is_read_up_to_the_repository`,
  `config_above_the_root_is_read_only_within_reach_of_the_marker`,
  `symlinked_analysis_root_reads_the_real_parents`,
  `flags_and_keys_that_move_the_runner_config_stay_medium` and
  `walk_without_a_marker_ends_at_the_filesystem_root_or_is_unknown`,
  `config_above_the_root_without_a_repository_boundary_is_not_read`,
  `directories_above_the_root_stop_at_the_repository_boundary`,
  `symlinked_snapshots_directory_stays_medium` and
  `symlinked_test_directory_stays_medium` pin example 7;
  `computed_and_member_spellings_of_delegating_keys_stay_medium`,
  `json_escaped_whitespace_before_the_update_flag_stays_medium`,
  `submodule_git_file_does_not_end_the_walk` and
  `only_a_submodule_pointer_is_not_a_boundary` pin example 8 and the
  #8768 spellings.
- `crates/ripr/src/analysis/committed_source/tests.rs`:
  `porcelain_parser_records_unrouted_snapshot_inputs` and
  `snapshot_inputs_differ_only_in_their_own_directory` pin the
  committed-history record example 6 reads.
- `fixtures/typescript-verdict-corpus` labels each example with runtime
  truth: `jest-spec0234-ex03` (example 1) and
  `jest-spec0258-ex02-describe-title` (example 2) read `credited`;
  `jest-spec0258-ex03-no-snapshot-file` (example 3) and
  `jest-spec0258-ex05-update-script` (example 5) read `gap`, as their
  runs let the edit pass; `jest-spec0258-ex04-hint` (example 4) reads
  `credited`; and
  `jest-spec0258-ex06-spaced-call` (example 6's spaced call) reads
  `credited`.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/committed_snapshot.rs`:
  `promote_committed_snapshots`, `committed_snapshot_file`,
  `runner_configs`, `joined_title`, `snapshot_key`, `literal_hint`,
  `snapshot_calls` and `mentions_update_key`.
- `crates/ripr/src/analysis/committed_source.rs`: the committed-history
  probe records snapshot inputs that differ from `HEAD`;
  `snapshot_inputs_differ_in` answers for one directory.
- `crates/ripr/src/analysis/language/router.rs`: `snapshot_input` names
  the files.
- `crates/ripr/src/analysis/language/typescript/mod.rs`: calls the
  promotion once per extracted test file.

## Metrics

- `typescript_committed_snapshot_mismatch`: acceptance examples whose
  observed reading differs from this spec. Target: zero.
