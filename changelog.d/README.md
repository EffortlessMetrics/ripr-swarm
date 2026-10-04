# Changelog fragments

Each PR that needs a changelog entry adds one file here instead of editing
`CHANGELOG.md`. Two PRs that each add a new file never conflict, so the entry
no longer has to be rebased by hand at merge time.

## Add a fragment

Create `changelog.d/<pr-or-issue>-<short-slug>.md`, for example
`changelog.d/5188-first-pr-quoting.md`. If the PR number is not known yet, use
the issue number or a unique slug; do not rename the file after merge.

The first line is the section, as an HTML comment. The rest is the entry in the
same prose style `CHANGELOG.md` already uses, with the issue or PR reference in
parentheses at the end:

```markdown
<!-- section: Fixed -->
- `ripr first-pr` quotes the root and refs in its preflight recovery
  commands, so a path with a space or apostrophe pastes as one argument (#5188).
```

Allowed sections are the ones in `docs/CHANGELOG_POLICY.md`: `Added`,
`Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`, `Docs`.

## What this is not

- Nothing enforces the format or the presence of a fragment. Reviewers check
  that a PR which needs an entry carries one. The fold step below is manual.
- Existing `CHANGELOG.md` entries stay where they are. Do not move them.

## Fold at the release cut

At the release cut, in the same step that resolves `CHANGELOG.md`, append each
fragment's body (every `*.md` here except this `README.md`) under the matching `Unreleased` section in file-name order,
then delete the folded fragment files in that commit. The fold happens once, in
the release-copy step, never inside an ordinary PR.
