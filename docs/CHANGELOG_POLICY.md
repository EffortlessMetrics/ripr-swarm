# Changelog Policy

The root changelog records repository-level changes that future contributors,
users, and agents need to notice.

## Where Entries Go

Add one fragment file under `changelog.d/` per PR instead of editing
`CHANGELOG.md` directly. Many PRs adding an entry to the same `Unreleased`
section conflicted with each other on every merge; separate files do not.
`changelog.d/README.md` has the file name, format, and the fold step at the
release cut. Entries already in `CHANGELOG.md` stay there.

## What To Include

Add an entry for:

- user-visible CLI behavior
- JSON, context, GitHub, SARIF, or LSP output changes
- config changes
- release or packaging changes
- documentation-system changes
- architecture or roadmap decisions that affect future PRs
- known compatibility notes

Do not add entries for:

- purely mechanical formatting
- internal refactors with no behavior, API, output, or workflow impact
- dependency bumps with no user-visible effect, unless they affect MSRV,
  packaging, or security

## Sections

Use these headings when relevant:

- `Added`
- `Changed`
- `Deprecated`
- `Removed`
- `Fixed`
- `Security`
- `Docs`
- `Internal / CI`

`Internal / CI` is for contributor-only corpus case counts, xtask/CI plumbing,
and fixture-registry bookkeeping. Do not file that class under `Added`,
`Changed`, or `Fixed`. Analyzer behavior that a corpus case happens to exercise
stays in the user-facing section that describes the behavior. Generated
`ripr init --ci` workflow notes stay user-facing. At the release cut, fold
`Internal / CI` fragments into a short rolled-up summary under that heading
rather than one bullet per case batch.

## Retiring A `ripr.toml` Key

Teams commit `ripr.toml` as reviewed policy, and every config table rejects
unknown keys. A key that is renamed or removed without notice therefore turns
every existing config into a load failure on the first run after an upgrade.
Retire a key in this order (#7453):

1. In the release that renames or replaces it, keep the old spelling working and
   print a named warning that says the key is deprecated and what replaces it.
   File the change under `Deprecated`.
2. Keep that behavior for at least one minor release. While ripr is pre-1.0 a
   minor release is the next `0.N.0` bump, so a key deprecated in `0.12.0` can be
   retired no earlier than `0.13.0`.
3. In the release that stops accepting the key, add it to `RETIRED_KEYS` in
   `crates/ripr/src/config.rs`, so the load error names the release that
   retired it and its replacement instead of a bare `unknown field`. File the
   change under `Removed`.

The `RETIRED_KEYS` table is empty until the first key is retired. Step 1 is a
policy requirement for the first rename, not machinery that exists today.

## Static Language

Changelog entries for static analysis behavior should use conservative exposure
language. Do not describe static findings as proving tests, killing mutants, or
showing survivors.
