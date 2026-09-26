# Release Copy Checklist

Public copy is part of the product. A release can be technically correct and
still fail users by leading with metadata, documenting the publishing process,
conflating build prerequisites with target-repository requirements, or teaching
a command weaker than the one the product actually emits.

Run this checklist before finalizing:

- the root or crate README;
- the Quickstart or editor first-use path;
- the GitHub Release body;
- crates.io, VS Marketplace, or Open VSX metadata;
- generated CI instructions; and
- any install, upgrade, or known-limitations copy.

Fix the copy or the artifact before publication. Release recovery and operator
history belong in [CHANGELOG.md](../CHANGELOG.md) and
[Release](RELEASE.md), not in the public product opener.

## Front-door structure

The first screen must answer these questions in order:

1. **What does ripr do for me?**
2. **What is the first useful command or editor action?**
3. **What result should I expect—even when no repair is available?**
4. **What does ripr not claim?**
5. **Where do I go deeper?**

Check all of the following:

- [ ] The opener uses plain language before internal terms.
- [ ] One runnable path appears before prerequisites, compatibility matrices,
      architecture, mission/vision prose, or a large badge block.
- [ ] The first result is described as one selected action **or** an honest
      no-action/limited state. Zero findings is not presented as a clean bill
      of health.
- [ ] The trust boundary appears near the first workflow: static evidence is
      not runtime mutation proof, correctness, or test-adequacy proof.
- [ ] The model, terminology, support tiers, compatibility details, and
      reference mechanics follow through progressive disclosure.
- [ ] The first screen is not a metadata wall. Keep badges to one compact line
      or move product/status badges below the first useful workflow.
- [ ] Internal terms such as `seam`, `discriminator`, `oracle`, `grip`, and
      `canonical gap` are introduced only after a plain-language description
      and a link to [Terminology](TERMINOLOGY.md).

## Installation and toolchain truth

Keep these three facts separate:

```text
RIPR build/install toolchain
repository analysis availability
repository project-verification toolchain
```

- [ ] Rust 1.95+ is described as RIPR's **build/install-from-source MSRV**.
- [ ] The copy does not imply that every repository analyzed by an already-built
      binary must itself use Rust 1.95+.
- [ ] Project verification is described as using the target repository's own
      selected toolchain and as independently available, failing, or limited.
- [ ] The normal VS Code path does not tell users to install or rebuild ripr
      when the extension supplies the server.
- [ ] `cargo install ripr` is true at publication time. A named version exists
      on crates.io before the copy says it does.
- [ ] `cargo install --path crates/ripr` is development/fallback guidance, not
      the required public first-run path.
- [ ] `doctor` is described as diagnostics, not as a substitute for the first
      useful analysis command.

## Commands, roots, and base refs

- [ ] Public examples use the repository's actual base ref or a placeholder
      such as `<base-ref>`. They do not teach `origin/main` as universal.
- [ ] Where a command resolves the default base automatically, the docs say so
      instead of requiring a hand-written ref.
- [ ] Product-generated commands preserve the repository selected when they
      were rendered. Copy must not use `cd` as a permanent workaround for a
      producer-owned wrong-root defect.
- [ ] Root, before/after artifacts, verification subject, and receipt refer to
      the same repository and comparable revisions.
- [ ] A composition command such as `first-pr` is not presented as the first
      analyzer action.
- [ ] Display commands, JSON fields, help, Quickstart, generated CI, and editor
      actions describe the same current command surface.
- [ ] A command shown as paste-ready has been executed in the claimed shell and
      path context, including native PowerShell where relevant.
- [ ] Failure, stale, partial, unavailable, wrong-root, and zero-subject states
      remain explicit; prose does not strengthen them to success.

## GitHub Release body

- [ ] The first paragraph describes the shipped user outcome, not the release
      transaction, retries, branch choreography, or recovery commands.
- [ ] The body says what changed since the last public version.
- [ ] Static evidence remains in the conservative vocabulary documented by
      [Terminology](TERMINOLOGY.md).
- [ ] The body does not use runtime mutation words such as `killed` or
      `survived` unless real runtime data is part of the release.
- [ ] Positioning remains consistent: ripr is static mutation-exposure
      analysis, catches the mutation-testing class of signal earlier and more
      cheaply, does not run mutants, and keeps mutation testing as the runtime
      backstop.
- [ ] Counts name their basis and denominator. Static audit counts are not
      public badge totals, coverage, runtime outcomes, or test-adequacy rates.
- [ ] Every selected defer appears as an exact public non-claim. Required work
      is not converted into a defer merely because the release notes can
      describe it.
- [ ] The final body is re-read on the exact integrated source head after the
      history-preserving join.

## Preview-language copy

- [ ] Packaging is separated from authority: an adapter may ship in the normal
      binary while its findings remain preview/advisory.
- [ ] TypeScript/JavaScript copy names the actual routed extensions:
      `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, and `.cjs`.
- [ ] Any narrower downstream repair, rerun, packet, or repo-mode surface is
      named precisely rather than hidden behind the broad adapter statement.
- [ ] Python's scoped repair route does not promote all Python static facts.
- [ ] Preview evidence is not described as Rust parity, gate authority,
      runtime execution, or support promotion.
- [ ] A zero-seam repo-scoped view over a preview-only repository is described
      as a renderer limitation, not a clean result.

## README, crate, and marketplace metadata

- [ ] Root `README.md` and `crates/ripr/README.md` share the same plain-language
      value proposition and trust boundary.
- [ ] `crates/ripr/Cargo.toml` description is user-facing crates.io copy.
- [ ] `editors/vscode/package.json` title and description are user-facing and
      match the README opener.
- [ ] `editors/vscode/README.md` opens with the editor user job and uses
      absolute GitHub links where marketplace rendering cannot resolve relative
      paths.
- [ ] Marketplace copy is reviewed in the actual VS Marketplace and Open VSX
      previews, not only in repository files.
- [ ] A VSIX is rebuilt after package metadata or extension README changes.

## Badges and freshness

- [ ] Live badges use live, authoritative endpoints.
- [ ] Manual badges carry a nearby hidden comment naming the source and
      last-checked date.
- [ ] Pending services are hidden or visibly labelled pending.
- [ ] Repository-wide ripr badges retain their exact meaning: generated counts
      of unresolved actionable static repair gaps, not coverage, runtime
      mutation outcomes, all seams, or all code without tests.
- [ ] Diff-scoped evidence stays in PR summaries and retained CI artifacts,
      not a repository-wide public badge.

## Release assets and dependent channels

- [ ] The GitHub Release contains the asset set documented in
      [Release](RELEASE.md): VSIX, server manifest, per-target archives, and
      checksums.
- [ ] Archive checksums match the server manifest.
- [ ] The documented bundled/cached/downloaded server chain matches the actual
      extension package and release assets.
- [ ] Dependent channels are triggered only after the GitHub Release asset set
      is verified.
- [ ] The marketplace workflow consumes the final rebuilt VSIX, not an earlier
      artifact attached to the same release train.
- [ ] Public source, release tags, registries, and marketplaces remain source
      repository authority; swarm rehearsal is not described as publication.

## Automated guards

Run the public-copy and documentation checks on the exact candidate surface:

```bash
cargo xtask check-product-copy
cargo xtask check-doc-index
cargo xtask check-doc-artifacts
cargo xtask check-static-language
cargo xtask check-command-catalog
cargo xtask check-output-contracts
cargo xtask markdown-links
```

`check-product-copy` scans the principal public surfaces, including root and
crate READMEs, Quickstart, editor copy, release docs, and marketplace metadata.
It rejects unbridged internal vocabulary. A file is bridged when it explains the
user job first and links to [Terminology](TERMINOLOGY.md) before relying on the
internal model.

Automated scans are necessary but insufficient. Execute the first-use and
install commands they describe. Text search cannot establish shell behavior,
artifact identity, source-root binding, marketplace packaging, or public asset
availability.

## When copy or packaging slips

Before dependent publication:

1. fix the copy or artifact;
2. rebuild the affected package;
3. rerun the exact public-copy and package checks; and
4. verify the final user-visible surface.

After dependent publication, use the fix-forward procedure in
[Release → Recovery](RELEASE.md#recovery). Keep retry and recovery history in
the changelog/operator record. The public release body should continue to
describe the product users can now obtain, not the internal recovery sequence.
