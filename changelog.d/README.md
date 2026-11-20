# Changelog fragments

Each PR that needs a changelog entry adds one file here instead of editing
`CHANGELOG.md`. Two PRs that each add a new file never conflict, so the entry
no longer has to be rebased by hand at merge time.

## Add a fragment

Create `changelog.d/<pr-or-issue>-<short-slug>.md`, for example
`changelog.d/5188-first-pr-quoting.md`. If the PR number is not known yet, use
a unique slug (not just an issue number, since two PRs for one issue would
collide) and check that no file of that name exists on `main`; do not rename
the file after merge.

The first line is the section, as an HTML comment. The rest is the entry in the
same prose style `CHANGELOG.md` already uses, with the issue or PR reference
(`#N` or a link to it) in parentheses at the end:

```markdown
<!-- section: Fixed -->
- `ripr first-pr` quotes the root and refs in its preflight recovery
  commands, so a path with a space or apostrophe pastes as one argument (#5188).
```

Allowed sections are the ones in `docs/CHANGELOG_POLICY.md`: `Added`,
`Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`, `Docs`,
`Internal / CI`. Verdict-corpus case counts, xtask/CI plumbing, and
fixture-registry bookkeeping use `Internal / CI` so the fold can roll them
into that Unreleased heading instead of crowding user-facing sections.

## What this is not

- `cargo xtask check-changelog-fragments` (run by `precommit`) checks the
  format: the section line (the only HTML comment allowed), every entry a
  `- ` bullet at column 0 with indented continuation lines, an issue or PR
  reference in each entry, a lowercase kebab-case name that is more than
  a number, that verdict-corpus case-count, `Repo ops:`, or maintainer-tooling
  entries (opening with a backticked `cargo xtask` command, `.github/`,
  `policy/`, `AGENTS.md`, `CLAUDE.md` or a maintainer doc) use `Internal / CI`
  rather than a user-facing section, and that no entry repeats another
  fragment or Unreleased entry word for word. Nothing checks that
  a PR which needs an entry carries one; reviewers do. The fold step below is
  the repo-owned `cargo xtask fold-changelog` command.
- Existing `CHANGELOG.md` entries stay where they are. Do not move them.
- `cargo xtask check-static-language` scans fragments, although
  `CHANGELOG.md` is exempt, so a fragment cannot reuse runtime-mutation
  wording that older `CHANGELOG.md` entries contain.

## Fold at the release cut

At the release cut, in the same step that resolves `CHANGELOG.md`, run
`cargo xtask fold-changelog`. It takes every `*.md` here except this
`README.md` in file-name order and appends its entry (everything after the
`<!-- section: ... -->` line, continuation lines intact) at the end of the
first `### <Section>` heading of that name in `Unreleased`, adding the heading
if it is missing, and with `--write` deletes the folded fragment files. Never
fold inside an ordinary feature PR.

```bash
cargo xtask fold-changelog                       # dry run: print the plan, write nothing
# preflight, then fold and delete one batch; the write runs only if the check passes
cargo xtask check-changelog-fragments && cargo xtask fold-changelog --write --limit 150
```

The default is a dry run. `--limit N` takes the first N fragments in name
order, so a 150-fragment batch plus `CHANGELOG.md` is 151 files, under the
160-file candidate-admission cap;
rerun for the next batch. The command refuses, naming the file, any fragment
with an unknown section, a missing section line or another format violation,
and any fragment whose whole entry text already appears in `Unreleased`, and
then writes nothing. That fold-time guard compares each fragment's entry text
as one block, not bullet by bullet, so a fragment that repeats one existing
bullet next to a new one still folds and duplicates the old bullet.
`check-changelog-fragments` compares each bullet; run it and require it to pass
before either `--write` command (including `--release-cut`, whose own check
runs only after it has rewritten `CHANGELOG.md` and deleted the fragments).
Once folded fragments are deleted, a rerun has nothing left to fold.

The fold may also run ahead of the cut as a snapshot, so the cut itself folds
only what is left. Fold and delete each batch in the same PR (`--write
--limit 150`, or a rolled-up summary for `Internal / CI` per
`docs/CHANGELOG_POLICY.md`): `check-changelog-fragments` fails any fragment
whose entry exactly duplicates an `Unreleased` entry, so a
`--keep-fragments` roll-up that leaves its fragments in place cannot pass on
its own. Batches built from one base can insert at different bullets of a
section so they merge in any order. A fragment added after a snapshot stays
until the next snapshot or the cut. After the final
fold, `cargo xtask check-changelog-fragments --release-cut` must pass: it
fails on any fragment still here.

On cut day run the preflight, then one write command. `--release-cut` folds
every fragment (it cannot be combined with `--limit` or `--keep-fragments`),
deletes them, and then requires `check-changelog-fragments --release-cut` to
pass, so a leftover or malformed fragment fails the same step. That final
check runs after the write, so a duplicated bullet is caught only once the
inputs are already rewritten; the preflight check catches it first:

```bash
cargo xtask fold-changelog --release-cut           # dry run: plan only
# preflight, then fold, delete, verify; the write runs only if the check passes
cargo xtask check-changelog-fragments && cargo xtask fold-changelog --write --release-cut
```

It prints a warning when the PR (every fragment plus `CHANGELOG.md`) would exceed
the 160-file admission cap; fold a snapshot ahead of the cut in that case. The
fold appends entries verbatim, so run the correctness pass over `Unreleased`
(duplicates, sections, claims that overstate what shipped) after it.

Both ordinary and `--release-cut` checks reject repeated column-zero
`### <Section>` headings within the first `## Unreleased` block. The diagnostic
names the first heading and each duplicate line. Combine those entries under
one heading without dropping their text before folding new fragments. Use the
canonical heading spelling above; indented entry examples and later versioned
sections are not fold destinations (#7673).
