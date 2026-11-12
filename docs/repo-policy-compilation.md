# Repository policy compilation custody

`repo-policy` remains unpublished product-independent repository automation.
Its production identity and original absolute directory watches cover the entire
production package plus the workspace manifest, lockfile and pinned toolchain.
There are no excluded files or selective-watch assumptions inside that boundary.
The original cached-script/backdated-worktree behavioral discriminator is retained.

Pure workflow contracts live in the sibling unpublished `repo-policy-contracts`
test package. Ten suites share one executable while retaining their modules,
platform conditions and scratch namespaces. The incident-route suite stays in the production package with a separate
executable because it changes its process working directory and uses the normal
policy library. The CI leaf suite also remains there because main now exercises
the production workflow-lifecycle validator. The pure contract package has no dependency on that library,
so focused workflow compilation does not bootstrap the production policy graph. A frozen
native baseline map checks every grouped case and its ignored state; omission,
duplication and changed-ignore controls must fail. Self-reexecution uses the
complete new logical test name and retains its nonzero execution receipts.

Guarded tests in the production package keep full resource custody across all
three directories: production, contracts and the unpublished dev-only
`repo-policy-test-identity` macro. The stable proc macro computes production and
full-resource digests and emits unused include-byte compiler dependency markers.
These markers do not retain raw source bytes in linked executables. Its dev-only
build script watches the complete directory union, including new/ignored files,
and uses the same runtime manifest-root and absolute-watch relocation rule.
Pure contracts have no dependency on this macro, so its invalidation cannot
rebuild every standalone workflow target through a shared dev dependency.
These two unpublished test-only packages use the ADR 0027 development exception;
neither changes the published package boundary or product dependency graph.

A retained production CLI can be reused after a sibling test-only edit. A retained
preflight receipt and resource-bound test executable must still reject that edit.
An unknown file anywhere inside the production package remains an implementation
input and must cause ordinary Cargo recovery as well as retained-CLI refusal.
Workspace nextest, all-target Clippy and doctests discover the new test package;
source selectors and policy allowlists retain it. The wheel workflow has no
development-event path filter after the auxiliary trigger retirement.

Qualification records exact source/tree/lock identities, native case and ignore
parity, cold/warm/leaf/shared/new-file controls, elapsed and CPU time, command and
compiler RSS, and unique allocated target storage. Shared-process concurrency,
self-reexecution and retained-artifact custody are separate controls. A smaller
selected rebuild graph does not establish cheaper unrelated feature combinations
or transport savings for a complete nextest archive.

No resource guard is changed. Retirement requires repeatable cold, warm and
worst-case envelopes for the real producer/consumer configurations, including
temporary link/archive overlap and concurrent admitted writers, with conservative
fallback and adequate host headroom. This policy-package experiment alone cannot
retire product-wide compile-size or whole-run guards.
