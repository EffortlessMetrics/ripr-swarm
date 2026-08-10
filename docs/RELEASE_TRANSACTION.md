# Live-history release transaction runbook

Status: active operator runbook for the `0.11.0` live-head transaction and
future releases using the same two-repository history model.

This document is the canonical lifecycle from the release boundary through
reopened swarm development. It sequences existing verifier contracts; it does
not replace their semantics or make publication implicit.

## Authority and command labels

`EffortlessMetrics/ripr-swarm` is development authority and preserves every
commit reachable from the selected swarm main, including the reviewable history
of ordinary squash-merged feature/fix PRs.
`EffortlessMetrics/ripr` is source, release, and distribution authority.
Public tags, GitHub Releases, crates.io, VS Code Marketplace, and Open VSX are
source-side surfaces. Never publish or create a public-release tag from swarm.

Every command is marked:

- **[READ-ONLY]** — reads a checkout, receipt, GitHub state, or public channel.
- **[LOCAL-MUTATING]** — changes only a named local checkout, worktree, index,
  branch, or receipt file.
- **[EXTERNAL-PUBLISHING]** — changes a remote ref, policy, tag, release,
  registry, marketplace, or other external state. Public artifact, tag,
  release, and store operations require explicit channel authorization in
  source issue [#1470](https://github.com/EffortlessMetrics/ripr/issues/1470);
  control-plane ref and policy operations require owner approval and a receipt.

Examples use POSIX shell syntax. On Windows, translate syntax but preserve
exact SHAs and expected-head checks; never substitute floating `main`.

## Packet, authority reset, and stop points

Create `target/ripr/release-transaction/<VERSION>/` outside tracked source.
The packet must bind `SOURCE_PARENT`, `SWARM_PARENT`,
`refs/ripr/release-<VERSION>-<SWARM_PARENT>`, `MERGE_BASE`, reviewed `JOIN_TREE`,
`J`, `SWARM_BEFORE`, `SOURCE_RELEASE_HEAD`, reviewed `BACK_SYNC_TREE`, and `K`.
Retain all-reachable and first-parent counts/digests, open-PR dispositions,
toolchain/version, routed-CI URL and `headSha`, conflict/resolution manifest,
artifact receipts, authorization, channel results, policy-before/exception/
after snapshots, and cleanup. JSON and Markdown projections must share inputs.

Historical candidate-only `C -> T`, hard-cut, replacement-freeze, and
candidate-ref receipts remain audit evidence but are superseded. They are not
the active `0.11.0` authority, denominator, pin, source parent, or permission.

Stop at once when a repository identity, owner, version, exact input, receipt,
expected head, policy state, or publication authorization is ambiguous. Green
CI, a ship packet, J, or K is evidence only; none authorizes publication.

## 1. Reset truth, source queue, and hold

Use fresh checkouts. Ordinary source-repository development PRs are drift:
redirect them to swarm or classify them as release/security work before the
source hold. Record each remaining PR as included, deferred, superseded, or
not release-relevant; do not close another owner's work to make the queue look
empty.

```bash
# [READ-ONLY] repo=both fresh operator checkouts; reconcile live state
git -C "$SWARM_ROOT" fetch origin --prune --tags
git -C "$SOURCE_ROOT" fetch origin --prune --tags
git -C "$SWARM_ROOT" status --short --branch
git -C "$SOURCE_ROOT" status --short --branch
gh pr list --repo EffortlessMetrics/ripr-swarm --state open --limit 100
gh pr list --repo EffortlessMetrics/ripr --state open --limit 100
git -C "$SWARM_ROOT" remote get-url origin
git -C "$SOURCE_ROOT" remote get-url origin
```

Admit only bounded final swarm cleanup: a release-blocking candidate failure,
source-preflight survivor, or required policy/evidence contract. After pinning,
later swarm-main merges are outside this release; branches may continue, but
their merges belong to the next release.

```bash
# [READ-ONLY] repo=source+swarm clean checkouts; capture exact heads
test -z "$(git -C "$SOURCE_ROOT" status --short)"
test -z "$(git -C "$SWARM_ROOT" status --short)"
SOURCE_PARENT="$(git -C "$SOURCE_ROOT" rev-parse refs/remotes/origin/main)"
SWARM_PARENT="$(git -C "$SWARM_ROOT" rev-parse refs/remotes/origin/main)"
test "$(git -C "$SOURCE_ROOT" rev-parse "$SOURCE_PARENT^{commit}")" = "$SOURCE_PARENT"
test "$(git -C "$SWARM_ROOT" rev-parse "$SWARM_PARENT^{commit}")" = "$SWARM_PARENT"
```

Hold `SOURCE_PARENT` until J is transported. If source main moves, stop and
reconcile; never silently update the variable.

## 2. Immutable swarm pin

```bash
# [LOCAL-MUTATING] repo=swarm root; create only the local transaction ref
VERSION=0.11.0
SWARM_REF="refs/ripr/release-${VERSION}-${SWARM_PARENT}"
git -C "$SWARM_ROOT" update-ref "$SWARM_REF" "$SWARM_PARENT"
test "$(git -C "$SWARM_ROOT" rev-parse "$SWARM_REF^{commit}")" = "$SWARM_PARENT"
```

```bash
# [EXTERNAL-PUBLISHING] repo=swarm remote; publish immutable pin ref only
git -C "$SWARM_ROOT" push origin "$SWARM_REF:$SWARM_REF"
git -C "$SWARM_ROOT" ls-remote origin "$SWARM_REF"
```

The pin receipt records merge base, separately named all-reachable and
first-parent counts, ordered SHA digests, exact ref resolution, open-PR
dispositions, version/toolchain, and claims/non-claims. The existing
source-preflight receipt owns the denominator/digest recipe; do not create a
second recipe. Repin only for a release-invalidating exact-candidate
semantic/policy failure or source-preflight survivor failure. Main movement
alone never repins. Changed inputs supersede the packet and require a new one.

## 3. Exact swarm qualification and source preflight

Qualification is a detached worktree at `SWARM_PARENT`; a hosted result must
report that exact SHA as `headSha`.

```bash
# [LOCAL-MUTATING] repo=swarm root; create disposable exact-head worktree
QUAL_ROOT="${SWARM_ROOT}-release-${VERSION}-qual"
git -C "$SWARM_ROOT" worktree add --detach "$QUAL_ROOT" "$SWARM_PARENT"
test "$(git -C "$QUAL_ROOT" rev-parse HEAD)" = "$SWARM_PARENT"
```

```bash
# [LOCAL-MUTATING] repo=swarm qualification worktree; checks write local reports
git -C "$QUAL_ROOT" status --short --branch
git -C "$QUAL_ROOT" rev-parse HEAD
cargo xtask check-pr
cargo xtask check-generated-clean
cargo xtask check-doc-index
```

A missing, timed-out, or differently headed hosted result is unavailable, not
a pass. Run the existing exact-pair preflight with exact SHA declarations; its
contract is [`SOURCE_PROMOTION_PREFLIGHT.md`](SOURCE_PROMOTION_PREFLIGHT.md)
and [`RIPR-SPEC-0148`](specs/RIPR-SPEC-0148-source-promotion-preflight.md).

```bash
# [LOCAL-MUTATING] repo=swarm operator checkout; preflight writes local receipts, no J
cargo xtask source-promotion preflight \
  --source-parent "$SOURCE_PARENT" --swarm-parent "$SWARM_PARENT" \
  --swarm-ref "$SWARM_REF" --source-repo "$SOURCE_ROOT" --swarm-repo "$SWARM_ROOT" \
  --source-main "$SOURCE_PARENT" --swarm-main "$SWARM_PARENT" \
  --version "$VERSION" --out "target/ripr/release-transaction/$VERSION/source-promotion"
```

Review every conflict path, survivor, swarm exclusion, authority candidate,
and separately reviewed `JOIN_TREE`; record a resolution manifest. A clean
textual merge is not semantic approval. Parent, ref, identity, ancestry,
digest, conflict, or tree changes invalidate the receipt.

## 4. Construct and prove J in source

J is the source-promotion join:

```text
J.parent[0] = SOURCE_PARENT
J.parent[1] = SWARM_PARENT
```

The exact-J verifier is the separate source contract from
[#1493](https://github.com/EffortlessMetrics/ripr/issues/1493), merged at
[`9c53b12b`](https://github.com/EffortlessMetrics/ripr/commit/9c53b12b73aa5a11c198f8c5265902127a6d7dff).
Use its verifier and receipt; this runbook does not duplicate its semantics.

```bash
# [LOCAL-MUTATING] repo=source promotion checkout; construct reviewed J
git -C "$SOURCE_ROOT" switch -c "promote/${VERSION}-swarm" "$SOURCE_PARENT"
git -C "$SOURCE_ROOT" merge --no-ff --no-commit "$SWARM_PARENT"
# Resolve only paths named by the reviewed preflight manifest.
git -C "$SOURCE_ROOT" commit -m "promote: join frozen ripr-swarm candidate for ${VERSION}"
J="$(git -C "$SOURCE_ROOT" rev-parse HEAD)"
```

```bash
# [READ-ONLY] repo=source promotion checkout; exact J shape/tree/ancestry
test "$(git -C "$SOURCE_ROOT" show -s --format='%P' "$J" | awk '{print NF}')" -eq 2
test "$(git -C "$SOURCE_ROOT" show -s --format='%P' "$J" | awk '{print $1}')" = "$SOURCE_PARENT"
test "$(git -C "$SOURCE_ROOT" show -s --format='%P' "$J" | awk '{print $2}')" = "$SWARM_PARENT"
git -C "$SOURCE_ROOT" merge-base --is-ancestor "$SOURCE_PARENT" "$J"
git -C "$SOURCE_ROOT" merge-base --is-ancestor "$SWARM_PARENT" "$J"
test "$(git -C "$SOURCE_ROOT" rev-parse "$J^{tree}")" = "$JOIN_TREE"
```

Never append a repair commit; squash, rebase, cherry-pick, and tree-equivalent
reconstruction fail the contract. Rebuild J from the held exact pair after a
changed resolution or receipt.

```bash
# [READ-ONLY] repo=source remote; expected-head guard before J transport
git -C "$SOURCE_ROOT" fetch origin main
test "$(git -C "$SOURCE_ROOT" rev-parse refs/remotes/origin/main)" = "$SOURCE_PARENT"
```

```bash
# [EXTERNAL-PUBLISHING] repo=source PR; merge only reviewed J with expected head
gh pr merge <SOURCE_PROMOTION_PR> --repo EffortlessMetrics/ripr --merge --match-head-commit "$J"
```

```bash
# [READ-ONLY] repo=source remote; bind source main to J after transport
git -C "$SOURCE_ROOT" fetch origin main
test "$(git -C "$SOURCE_ROOT" rev-parse refs/remotes/origin/main)" = "$J"
```

## 5. Metadata and source artifact qualification

J carries the promoted graph. Version/changelog metadata is a separate source
release-preparation change after J reaches source main; never bump J.

```bash
# [LOCAL-MUTATING] repo=source release-prep checkout at J; metadata only
git -C "$SOURCE_ROOT" switch -c "release/${VERSION}" "$J"
(cd "$SOURCE_ROOT" && cargo xtask bump-version "$VERSION")
```

Set `SOURCE_RELEASE_HEAD` to the exact source-main SHA after that PR. The ship
packet needs fresh evidence for all of these, bound to that SHA:

| Surface | Repository/worktree and mode | Proof | Boundary |
| Crate | source exact release checkout, **[LOCAL-MUTATING]** for package output; **[READ-ONLY]** dry-run query | `cargo package -p ripr --list`; `cargo publish -p ripr --dry-run` | Package/publishability, not publication |
| Installed CLI | source exact release checkout, **[LOCAL-MUTATING]** | `cargo install --path crates/ripr --locked --force --root target/ripr/install-smoke` plus version/doctor/fixture smoke | Local installed binary |
| Readiness/consumer | source exact release checkout, **[LOCAL-MUTATING]** | `cargo xtask release-readiness --version <VERSION>` | Version alignment and journey receipt |
| Linux/Windows | source release workflow artifacts, **[READ-ONLY]** when inspecting exact `SOURCE_RELEASE_HEAD` outputs | Workflow artifacts bound to `SOURCE_RELEASE_HEAD` | Platform proof; one OS is not the other |
| Server | source exact release checkout, **[LOCAL-MUTATING]** | `cargo xtask release-server-archive ...`; `cargo xtask release-server-manifest ...` | Archive, manifest, checksum shape |
| VSIX | source exact release checkout, **[LOCAL-MUTATING]** | `npm --prefix editors/vscode ci`, compile, package | Local package/version proof |
| Badge | source exact release checkout, **[LOCAL-MUTATING]** | `cargo xtask repo-badge-artifacts` plus generated-clean/badge policy checks | Generated endpoint/freshness proof |

Use [`RELEASE_BINARIES.md`](RELEASE_BINARIES.md),
[`RELEASE_MARKETPLACE.md`](RELEASE_MARKETPLACE.md),
[`INSTALLATION_VERIFICATION.md`](INSTALLATION_VERIFICATION.md), and
[`BADGE_POLICY.md`](BADGE_POLICY.md) for surface semantics. Record exact SHA,
command, status, artifact path, and limitation; never copy an old blanket
no-execution sentence.

## 6. Ship packet, #1470 authorization, channels, and verification

The packet is evidence, not permission. Before external operations, source
issue #1470 must name `VERSION`, `SOURCE_RELEASE_HEAD`, and the authorized
channels/order. If absent, stale, or silent, stop.

```bash
# [READ-ONLY] repo=source/public services; audit authorization and identity
gh issue view 1470 --repo EffortlessMetrics/ripr --json state,title,body,url
git -C "$SOURCE_ROOT" rev-parse "$SOURCE_RELEASE_HEAD"
gh release list --repo EffortlessMetrics/ripr --limit 5
```

Publish one channel at a time and receipt it before the next. These templates
require #1470; they are not permission to publish from this documentation PR.

```bash
# [EXTERNAL-PUBLISHING] repo=source/public channels; explicit #1470 only
git -C "$SOURCE_ROOT" push origin "$SOURCE_RELEASE_HEAD:refs/tags/v${VERSION}"
cargo publish -p ripr
gh release create "v${VERSION}" --repo EffortlessMetrics/ripr --target "$SOURCE_RELEASE_HEAD" --title "ripr ${VERSION}" --notes-file "$RELEASE_NOTES"
gh workflow run release-server-binaries.yml --repo EffortlessMetrics/ripr -f version="$VERSION"
gh workflow run publish-extension.yml --repo EffortlessMetrics/ripr -f version="$VERSION" -f publish_vs_marketplace=true -f publish_open_vsx=true
```

```bash
# [READ-ONLY] repo=source/public channels; verify only channels that ran
gh release view "v${VERSION}" --repo EffortlessMetrics/ripr --json tagName,targetCommitish,isDraft,isPrerelease,assets,url
cargo search ripr --limit 5
curl -i "https://crates.io/api/v1/crates/ripr"
gh run list --repo EffortlessMetrics/ripr --limit 20
```

After partial publication, stop later channels, record public state, and obtain
a corrective-release decision through #1470. Never delete a public immutable
tag or claim a complete release.

## 7. Construct and transport K after public verification

Freeze exact post-publication inputs:

```text
SWARM_BEFORE        = exact swarm main before back-sync
SOURCE_RELEASE_HEAD = exact released source main
BACK_SYNC_TREE      = reviewed tree retaining swarm authority
K.parent[0]         = SWARM_BEFORE
K.parent[1]         = SOURCE_RELEASE_HEAD
```

The exact-K contract is [`BACK_SYNC_VERIFIER.md`](BACK_SYNC_VERIFIER.md),
[`RIPR-SPEC-0149`](specs/RIPR-SPEC-0149-back-sync-verifier.md), and
[#3100](https://github.com/EffortlessMetrics/ripr-swarm/issues/3100), merged
at [`4589b85a`](https://github.com/EffortlessMetrics/ripr-swarm/commit/4589b85a8f63529c4838abe5ee54a78d28d989c7). Use its verifier and receipt;
this runbook does not redefine its semantics.

```bash
# [READ-ONLY] repo=swarm+source; freeze K pair and pre-transport heads
SWARM_BEFORE="$(git -C "$SWARM_ROOT" rev-parse refs/remotes/origin/main)"
SOURCE_RELEASE_HEAD="$(git -C "$SOURCE_ROOT" rev-parse refs/remotes/origin/main)"
test "$(git -C "$SWARM_ROOT" rev-parse "$SWARM_BEFORE^{commit}")" = "$SWARM_BEFORE"
test "$(git -C "$SOURCE_ROOT" rev-parse "$SOURCE_RELEASE_HEAD^{commit}")" = "$SOURCE_RELEASE_HEAD"
```

```bash
# [LOCAL-MUTATING] repo=swarm back-sync checkout; construct reviewed K
git -C "$SWARM_ROOT" switch -c "back-sync/${VERSION}" "$SWARM_BEFORE"
git -C "$SWARM_ROOT" merge --no-ff --no-commit "$SOURCE_RELEASE_HEAD"
# Retain swarm checks/runners/settings/authority; import release receipts only.
git -C "$SWARM_ROOT" commit -m "sync: back-sync released ${VERSION} from ripr"
K="$(git -C "$SWARM_ROOT" rev-parse HEAD)"
```

Run the existing exact-K verifier before and after transport using exact
`--swarm-main` and `--source-main` SHAs, release receipt, and policy snapshots;
the copyable command lives in [`BACK_SYNC_VERIFIER.md`](BACK_SYNC_VERIFIER.md)
so verifier semantics have one owner.

```bash
# [READ-ONLY] repo=swarm+source; expected-head guard before K transport
gh api repos/EffortlessMetrics/ripr-swarm/branches/main/protection > "$POLICY_BEFORE"
git -C "$SWARM_ROOT" fetch origin main
test "$(git -C "$SWARM_ROOT" rev-parse refs/remotes/origin/main)" = "$SWARM_BEFORE"
```

If merge commits are disabled, obtain owner approval for this one
ancestry-preserving transport, record before/temporary/after policy evidence,
and restore normal policy immediately. Never force-push, squash, rebase, or
merge unrelated work.

```bash
# [EXTERNAL-PUBLISHING] repo=swarm remote/policy; owner-approved K transport only
git -C "$SWARM_ROOT" push origin "back-sync/${VERSION}:refs/heads/main"
```

```bash
# [READ-ONLY] repo=swarm remote; bind swarm main to K after transport
git -C "$SWARM_ROOT" fetch origin main
test "$(git -C "$SWARM_ROOT" rev-parse refs/remotes/origin/main)" = "$K"
```

J is `[SOURCE_PARENT, SWARM_PARENT]`; K is `[SWARM_BEFORE,
SOURCE_RELEASE_HEAD]`. Neither proves release correctness, artifact adequacy,
publication success, or semantic equivalence. Source publication workflows and
settings remain ancestry evidence and never become swarm authority.

## 8. Re-baseline and clean up

Only after K is reachable from swarm main, policy is restored, and the
publication receipt is complete may development resume. Re-baseline `0.11.1`
from exact K, not a candidate or floating branch.

```bash
# [READ-ONLY] repo=swarm; reopen development only from published K
git -C "$SWARM_ROOT" fetch origin main
test "$(git -C "$SWARM_ROOT" rev-parse refs/remotes/origin/main)" = "$K"
git -C "$SWARM_ROOT" log --oneline --decorate -n 5 "$K"
```

Remove only clean worktrees, local branches, and scratch files created by this
transaction. Keep shared Cargo/npm caches and historical receipts.

```bash
# [LOCAL-MUTATING] repo=swarm+source; lane-created clean surfaces only
test -z "$(git -C "$QUAL_ROOT" status --short)"
git -C "$SWARM_ROOT" worktree remove "$QUAL_ROOT"
git -C "$SWARM_ROOT" branch -d "back-sync/${VERSION}"
git -C "$SOURCE_ROOT" branch -d "promote/${VERSION}-swarm"
git -C "$SOURCE_ROOT" branch -d "release/${VERSION}" 2>/dev/null || true
```

## Failure recovery and rollback

- Before pin: fix premise or queue disposition; no release claim exists.
- After pin: changed input supersedes the receipt; retain it and repin only
  under section 2's rule.
- J conflict/verifier failure: regenerate preflight and rebuild J; never append
  a repair commit.
- Source metadata/artifact failure: do not tag or publish; refresh affected
  receipts after a separate source fix.
- Partial publication: stop later channels, record public state, and obtain a
  corrective-release decision through #1470; do not rewrite J.
- K expected-head/policy failure: restore policy, retain untransported K, and
  return to the owner-approved transport decision; never force-push.
- After K: do not rewrite it to repair a receipt; a corrective source release
  requires a new authorized transaction if swarm must receive it.

The safe rollback for this documentation lane is to revert its PR. This
runbook never authorizes direct `origin/main` mutation, force pushes, deletion
of ambiguous work, or deletion of historical evidence.
