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
Run the command blocks in one operator shell, or export the variables before
continuing. The paths below are packet-local and are not repository authority.

```bash
# [LOCAL-MUTATING] repo=operator checkout; initialize transaction packet paths
VERSION=0.11.0
PACKET_ROOT="target/ripr/release-transaction/${VERSION}"
mkdir -p "$PACKET_ROOT"
RELEASE_NOTES="$PACKET_ROOT/release-notes.md"
PUBLICATION_RECEIPT="$PACKET_ROOT/publication-receipt.json"
POLICY_BEFORE="$PACKET_ROOT/policy-before.json"
POLICY_EXCEPTION="$PACKET_ROOT/policy-temporary-exception.json"
POLICY_AFTER="$PACKET_ROOT/policy-after.json"
POLICY_APPROVAL="$PACKET_ROOT/policy-owner-approval.txt"

# [READ-ONLY] repo=operator shell; require caller-selected fresh roots
SOURCE_ROOT="${SOURCE_ROOT:?set to the absolute path of a fresh ripr checkout}"
SWARM_ROOT="${SWARM_ROOT:?set to the absolute path of a fresh ripr-swarm checkout}"
test "$(git -C "$SOURCE_ROOT" rev-parse --is-inside-work-tree)" = true
test "$(git -C "$SWARM_ROOT" rev-parse --is-inside-work-tree)" = true
```

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
SOURCE_ORIGIN="$(git -C "$SOURCE_ROOT" remote get-url origin)"
SWARM_ORIGIN="$(git -C "$SWARM_ROOT" remote get-url origin)"
case "$SOURCE_ORIGIN" in
  git@github.com:EffortlessMetrics/ripr.git|https://github.com/EffortlessMetrics/ripr|https://github.com/EffortlessMetrics/ripr.git) ;;
  *) echo "unexpected source origin: $SOURCE_ORIGIN" >&2; exit 1 ;;
esac
case "$SWARM_ORIGIN" in
  git@github.com:EffortlessMetrics/ripr-swarm.git|https://github.com/EffortlessMetrics/ripr-swarm|https://github.com/EffortlessMetrics/ripr-swarm.git) ;;
  *) echo "unexpected swarm origin: $SWARM_ORIGIN" >&2; exit 1 ;;
esac
```

Admit only bounded final swarm cleanup: a release-blocking candidate failure,
source-preflight survivor, or required policy/evidence contract. After pinning,
later swarm-main merges are outside this release; branches may continue, but
their merges belong to the next release.

```bash
# [LOCAL-MUTATING] repo=packet; capture exact heads and pin files
test -z "$(git -C "$SOURCE_ROOT" status --short)"
test -z "$(git -C "$SWARM_ROOT" status --short)"
SOURCE_PARENT="$(git -C "$SOURCE_ROOT" rev-parse refs/remotes/origin/main)"
SWARM_PARENT="$(git -C "$SWARM_ROOT" rev-parse refs/remotes/origin/main)"
test "$(git -C "$SOURCE_ROOT" rev-parse "$SOURCE_PARENT^{commit}")" = "$SOURCE_PARENT"
test "$(git -C "$SWARM_ROOT" rev-parse "$SWARM_PARENT^{commit}")" = "$SWARM_PARENT"
printf '%s\n' "$SOURCE_PARENT" > "$PACKET_ROOT/SOURCE_PARENT"
printf '%s\n' "$SWARM_PARENT" > "$PACKET_ROOT/SWARM_PARENT"
```

```bash
# [READ-ONLY] repo=packet+source+swarm; load pins and reject drift
SOURCE_PARENT="$(tr -d '\r\n' < "$PACKET_ROOT/SOURCE_PARENT")"
SWARM_PARENT="$(tr -d '\r\n' < "$PACKET_ROOT/SWARM_PARENT")"
test "$(git -C "$SOURCE_ROOT" rev-parse refs/remotes/origin/main)" = "$SOURCE_PARENT"
test "$(git -C "$SWARM_ROOT" rev-parse refs/remotes/origin/main)" = "$SWARM_PARENT"
```

The queue snapshot is produced by the two `gh pr list` calls above and bound
to those exact parent SHAs. Keep the producer output, not a hand-edited table:

```bash
# [LOCAL-MUTATING] repo=packet; produce and bind the live-head selection JSON
gh pr list --repo EffortlessMetrics/ripr-swarm --state open --limit 100 --json number,title,headRefName,headRefOid,baseRefName,isDraft,updatedAt > "$PACKET_ROOT/swarm-open-prs.json"
gh pr list --repo EffortlessMetrics/ripr --state open --limit 100 --json number,title,headRefName,headRefOid,baseRefName,isDraft,updatedAt > "$PACKET_ROOT/source-open-prs.json"
jq -n --arg captured_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --arg source_parent "$SOURCE_PARENT" --arg swarm_parent "$SWARM_PARENT" --slurpfile swarm "$PACKET_ROOT/swarm-open-prs.json" --slurpfile source "$PACKET_ROOT/source-open-prs.json" '{schema_version: 1, producer: "gh pr list", captured_at_utc: $captured_at, source_parent: $source_parent, swarm_parent: $swarm_parent, queues: {swarm: $swarm[0], source: $source[0]}}' > "$PACKET_ROOT/live-head-selection.json"
jq -e --arg source_parent "$SOURCE_PARENT" --arg swarm_parent "$SWARM_PARENT" '.source_parent == $source_parent and .swarm_parent == $swarm_parent' "$PACKET_ROOT/live-head-selection.json" >/dev/null
```

If either exact parent moves before J, stop, record a new producer snapshot, and
re-run the queue disposition. Do not update only a SHA field in place; the
selection JSON, queue outputs, receipts, and authority bindings are one packet.

Hold `SOURCE_PARENT` until J is transported. If source main moves, stop and
reconcile; never silently update the variable.

## 2. Guarded swarm pin

```bash
# [LOCAL-MUTATING] repo=swarm root; create only the local transaction ref
VERSION=0.11.0
SWARM_REF="refs/ripr/release-${VERSION}-${SWARM_PARENT}"
git -C "$SWARM_ROOT" update-ref "$SWARM_REF" "$SWARM_PARENT"
test "$(git -C "$SWARM_ROOT" rev-parse "$SWARM_REF^{commit}")" = "$SWARM_PARENT"
```

```bash
# [EXTERNAL-PUBLISHING] repo=swarm remote; publish guarded pin ref only
# A Git ref is not intrinsically immutable. Guard first publication and every
# later observation; if it already exists, is deleted, or resolves to another
# SHA, stop and invalidate this packet. A protected tag/ref ruleset may replace
# this guard only when its exact update/deletion receipt is attached.
PIN_BEFORE="$(git -C "$SWARM_ROOT" ls-remote origin "$SWARM_REF" | awk '{print $1}')"
test -z "$PIN_BEFORE"
git -C "$SWARM_ROOT" push origin "$SWARM_REF:$SWARM_REF"
PIN_AFTER="$(git -C "$SWARM_ROOT" ls-remote origin "$SWARM_REF" | awk '{print $1}')"
test "$PIN_AFTER" = "$SWARM_PARENT"
printf '%s\n' "$PIN_AFTER" > "$PACKET_ROOT/pin-remote.sha"
```

At every later receipt, read `refs/ripr/release-${VERSION}-${SWARM_PARENT}`
again and require the recorded SHA to remain `SWARM_PARENT`. This is a
guarded direct ref, not a claim that an unprotected Git namespace is immutable.

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
The resolution reviewer produces `JOIN_TREE` as a full tree SHA in the
resolution manifest; the automatic `preview_tree` is never substituted.

```bash
# [LOCAL-MUTATING] repo=swarm operator checkout; preflight writes local receipts, no J
JOIN_TREE="${JOIN_TREE:?set to the separately reviewed full resolved-tree SHA from the resolution manifest}"
cargo xtask source-promotion preflight \
  --source-parent "$SOURCE_PARENT" --swarm-parent "$SWARM_PARENT" \
  --swarm-ref "$SWARM_REF" --source-repo "$SOURCE_ROOT" --swarm-repo "$SWARM_ROOT" \
  --source-main "$SOURCE_PARENT" --swarm-main "$SWARM_PARENT" \
  --version "$VERSION" --resolved-tree "$JOIN_TREE" \
  --out "target/ripr/release-transaction/$VERSION/source-promotion"
PREFLIGHT_JSON="$PACKET_ROOT/source-promotion/source-promotion-preflight.json"
test "$(jq -r '.dry_merge.reviewed_resolved_tree // empty' "$PREFLIGHT_JSON")" = "$JOIN_TREE"
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
git -C "$SOURCE_ROOT" fetch "$SWARM_ORIGIN" "$SWARM_PARENT"
test "$(git -C "$SOURCE_ROOT" rev-parse "$SWARM_PARENT^{commit}")" = "$SWARM_PARENT"
git -C "$SOURCE_ROOT" switch -c "promote/${VERSION}-swarm" "$SOURCE_PARENT"
git -C "$SOURCE_ROOT" merge --no-ff --no-commit "$SWARM_PARENT"
# Resolve only paths named by the reviewed preflight manifest.
git -C "$SOURCE_ROOT" commit -m "promote: join frozen ripr-swarm candidate for ${VERSION}"
J="$(git -C "$SOURCE_ROOT" rev-parse HEAD)"
```

```bash
# [READ-ONLY] repo=source; bind exactly one reviewed promotion PR
SOURCE_PROMOTION_PR="$(gh pr list --repo EffortlessMetrics/ripr --head "$(git -C "$SOURCE_ROOT" branch --show-current)" --state open --json number --jq 'if length == 1 then .[0].number else empty end')"
test -n "$SOURCE_PROMOTION_PR"
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
gh pr merge "$SOURCE_PROMOTION_PR" --repo EffortlessMetrics/ripr --merge --match-head-commit "$J"
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

```bash
# [LOCAL-MUTATING] repo=source+packet; produce the release-head binding after metadata merge
git -C "$SOURCE_ROOT" fetch origin main
SOURCE_RELEASE_HEAD="$(git -C "$SOURCE_ROOT" rev-parse refs/remotes/origin/main)"
test "$(git -C "$SOURCE_ROOT" rev-parse "$SOURCE_RELEASE_HEAD^{commit}")" = "$SOURCE_RELEASE_HEAD"
```

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
# [LOCAL-MUTATING] repo=packet; capture #1470 authorization and public state
AUTHORIZATION_RECEIPT="$PACKET_ROOT/publication-authorization.json"
AUTHORIZATION_BODY="$PACKET_ROOT/publication-authorization.md"
gh issue view 1470 --repo EffortlessMetrics/ripr --json state,title,body,url > "$AUTHORIZATION_RECEIPT"
gh issue view 1470 --repo EffortlessMetrics/ripr --json body --jq .body > "$AUTHORIZATION_BODY"
gh release list --repo EffortlessMetrics/ripr --limit 5 > "$PACKET_ROOT/releases-before.txt"
```

```bash
# [READ-ONLY] repo=source/public services; fail closed on #1470 authorization
test "$(jq -r .state "$AUTHORIZATION_RECEIPT")" = OPEN
grep -Fxq "VERSION=${VERSION}" "$AUTHORIZATION_BODY"
grep -Fxq "SOURCE_RELEASE_HEAD=${SOURCE_RELEASE_HEAD}" "$AUTHORIZATION_BODY"
grep -Fxq "CHANNEL_ORDER=crate,github-release,server-binaries,vs-marketplace,open-vsx" "$AUTHORIZATION_BODY"
git -C "$SOURCE_ROOT" rev-parse "$SOURCE_RELEASE_HEAD"
```

Publish one channel at a time and receipt it before the next. These templates
require #1470; they are not permission to publish from this documentation PR.

```bash
# [EXTERNAL-PUBLISHING] repo=source; explicit #1470 crate/tag authorization only
test -z "$(git -C "$SOURCE_ROOT" ls-remote origin "refs/tags/v${VERSION}" | awk '{print $1}')"
git -C "$SOURCE_ROOT" push origin "$SOURCE_RELEASE_HEAD:refs/tags/v${VERSION}"
```

```bash
# [LOCAL-MUTATING] repo=packet; capture the exact published tag target
gh api "repos/EffortlessMetrics/ripr/git/ref/tags/v${VERSION}" --jq .object.sha > "$PACKET_ROOT/tag-object.sha"
test "$(tr -d '\r\n' < "$PACKET_ROOT/tag-object.sha")" = "$SOURCE_RELEASE_HEAD"
```

```bash
# [EXTERNAL-PUBLISHING] repo=source; explicit #1470 crate authorization only
cargo publish -p ripr
```

```bash
# [READ-ONLY] repo=source/public; receipt crate publication before next channel
curl -fsSL "https://crates.io/api/v1/crates/ripr/${VERSION}" | jq -e --arg version "$VERSION" '.version == $version' >/dev/null
```

```bash
# [EXTERNAL-PUBLISHING] repo=source; explicit #1470 GitHub-release authorization only
gh release create "v${VERSION}" --repo EffortlessMetrics/ripr --target "$SOURCE_RELEASE_HEAD" --title "ripr ${VERSION}" --notes-file "$RELEASE_NOTES"
```

```bash
# [READ-ONLY] repo=source/public; receipt GitHub release before next channel
gh release view "v${VERSION}" --repo EffortlessMetrics/ripr --json tagName,targetCommitish,isDraft,isPrerelease,assets,url
test "$(gh release view "v${VERSION}" --repo EffortlessMetrics/ripr --json isDraft --jq .isDraft)" = false
```

```bash
# [EXTERNAL-PUBLISHING] repo=source; explicit #1470 server-artifact authorization only
gh workflow run release-server-binaries.yml --repo EffortlessMetrics/ripr --ref "$SOURCE_RELEASE_HEAD" -f version="$VERSION"
```

```bash
# [LOCAL-MUTATING] repo=packet; capture successful server run at exact release head
SERVER_RUN_ID="$(gh run list --repo EffortlessMetrics/ripr --workflow release-server-binaries.yml --limit 20 --json databaseId,headSha,status,conclusion,url --jq ".[] | select(.headSha == \"$SOURCE_RELEASE_HEAD\" and .status == \"completed\" and .conclusion == \"success\") | .databaseId" | head -n 1)"
test -n "$SERVER_RUN_ID"
gh run view "$SERVER_RUN_ID" --repo EffortlessMetrics/ripr --json databaseId,headSha,status,conclusion,url > "$PACKET_ROOT/server-run-receipt.json"
jq -e --arg sha "$SOURCE_RELEASE_HEAD" '.headSha == $sha and .status == "completed" and .conclusion == "success"' "$PACKET_ROOT/server-run-receipt.json" >/dev/null
```

```bash
# [EXTERNAL-PUBLISHING] repo=source; explicit #1470 VS Marketplace authorization only
gh workflow run publish-extension.yml --repo EffortlessMetrics/ripr --ref "$SOURCE_RELEASE_HEAD" -f version="$VERSION" -f publish_vs_marketplace=true -f publish_open_vsx=false
```

```bash
# [LOCAL-MUTATING] repo=packet; capture successful VS Marketplace run at exact release head
EXTENSION_RUN_ID="$(gh run list --repo EffortlessMetrics/ripr --workflow publish-extension.yml --limit 20 --json databaseId,headSha,status,conclusion,url --jq ".[] | select(.headSha == \"$SOURCE_RELEASE_HEAD\" and .status == \"completed\" and .conclusion == \"success\") | .databaseId" | head -n 1)"
test -n "$EXTENSION_RUN_ID"
gh run view "$EXTENSION_RUN_ID" --repo EffortlessMetrics/ripr --json databaseId,headSha,status,conclusion,url > "$PACKET_ROOT/vs-marketplace-run-receipt.json"
jq -e --arg sha "$SOURCE_RELEASE_HEAD" '.headSha == $sha and .status == "completed" and .conclusion == "success"' "$PACKET_ROOT/vs-marketplace-run-receipt.json" >/dev/null
```

```bash
# [EXTERNAL-PUBLISHING] repo=source; explicit #1470 Open VSX authorization only
gh workflow run publish-extension.yml --repo EffortlessMetrics/ripr --ref "$SOURCE_RELEASE_HEAD" -f version="$VERSION" -f publish_vs_marketplace=false -f publish_open_vsx=true
```

```bash
# [LOCAL-MUTATING] repo=packet; capture successful Open VSX run at exact release head
OPENVSX_RUN_ID="$(gh run list --repo EffortlessMetrics/ripr --workflow publish-extension.yml --limit 20 --json databaseId,headSha,status,conclusion,url --jq ".[] | select(.headSha == \"$SOURCE_RELEASE_HEAD\" and .status == \"completed\" and .conclusion == \"success\") | .databaseId" | head -n 1)"
test -n "$OPENVSX_RUN_ID"
gh run view "$OPENVSX_RUN_ID" --repo EffortlessMetrics/ripr --json databaseId,headSha,status,conclusion,url > "$PACKET_ROOT/open-vsx-run-receipt.json"
jq -e --arg sha "$SOURCE_RELEASE_HEAD" '.headSha == $sha and .status == "completed" and .conclusion == "success"' "$PACKET_ROOT/open-vsx-run-receipt.json" >/dev/null
```

After partial publication, stop later channels, record public state, and obtain
a corrective-release decision through #1470. Never delete or retarget a public
tag, or claim a complete release.

## 7. Construct and transport K after public verification

Freeze exact post-publication inputs:

```text
SWARM_BEFORE        = exact swarm main before back-sync
SOURCE_RELEASE_HEAD = exact released source main
BACK_SYNC_TREE      = reviewed tree retaining swarm authority
K.parent[0]         = SWARM_BEFORE
K.parent[1]         = SOURCE_RELEASE_HEAD
```

Before leaving the source publication phase, write the frozen values once and
never replace them with a later floating-head lookup:

```bash
# [LOCAL-MUTATING] repo=packet; persist frozen K inputs for later guards
test ! -e "$PACKET_ROOT/SWARM_BEFORE" && test ! -e "$PACKET_ROOT/SOURCE_RELEASE_HEAD"
printf '%s\n' "$SWARM_PARENT" > "$PACKET_ROOT/SWARM_BEFORE"
printf '%s\n' "$SOURCE_RELEASE_HEAD" > "$PACKET_ROOT/SOURCE_RELEASE_HEAD"
```

The exact-K contract is [`BACK_SYNC_VERIFIER.md`](BACK_SYNC_VERIFIER.md),
[`RIPR-SPEC-0149`](specs/RIPR-SPEC-0149-back-sync-verifier.md), and
[#3100](https://github.com/EffortlessMetrics/ripr-swarm/issues/3100), merged
at [`4589b85a`](https://github.com/EffortlessMetrics/ripr-swarm/commit/4589b85a8f63529c4838abe5ee54a78d28d989c7). Use its verifier and receipt;
this runbook does not redefine its semantics.

```bash
# [READ-ONLY] repo=swarm+source; freeze K pair and pre-transport heads
test -s "$PACKET_ROOT/SWARM_BEFORE" && test -s "$PACKET_ROOT/SOURCE_RELEASE_HEAD"
SWARM_BEFORE="$(tr -d '\r\n' < "$PACKET_ROOT/SWARM_BEFORE")"
SOURCE_RELEASE_HEAD="$(tr -d '\r\n' < "$PACKET_ROOT/SOURCE_RELEASE_HEAD")"
CURRENT_SWARM_HEAD="$(git -C "$SWARM_ROOT" rev-parse refs/remotes/origin/main)"
CURRENT_SOURCE_HEAD="$(git -C "$SOURCE_ROOT" rev-parse refs/remotes/origin/main)"
test "$CURRENT_SWARM_HEAD" = "$SWARM_BEFORE"
test "$CURRENT_SOURCE_HEAD" = "$SOURCE_RELEASE_HEAD"
test "$(git -C "$SWARM_ROOT" rev-parse "$SWARM_BEFORE^{commit}")" = "$SWARM_BEFORE"
test "$(git -C "$SOURCE_ROOT" rev-parse "$SOURCE_RELEASE_HEAD^{commit}")" = "$SOURCE_RELEASE_HEAD"
```

```bash
# [LOCAL-MUTATING] repo=swarm back-sync checkout; construct reviewed K
git -C "$SWARM_ROOT" fetch "$SOURCE_ORIGIN" "$SOURCE_RELEASE_HEAD"
test "$(git -C "$SWARM_ROOT" rev-parse "$SOURCE_RELEASE_HEAD^{commit}")" = "$SOURCE_RELEASE_HEAD"
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
# [LOCAL-MUTATING] repo=swarm; write policy-before evidence
gh api repos/EffortlessMetrics/ripr-swarm/branches/main/protection > "$POLICY_BEFORE"
```

```bash
# [READ-ONLY] repo=swarm policy; verify before-state and owner exception receipt
jq -e '.allow_force_pushes.enabled == false and .allow_deletions.enabled == false' "$POLICY_BEFORE" >/dev/null
test -s "$POLICY_APPROVAL"
grep -Fxq "ALLOW_ANCESTRY_PRESERVING_BACK_SYNC=${VERSION}" "$POLICY_APPROVAL"
test -s "$POLICY_EXCEPTION"
jq -e '.required_linear_history.enabled == false and .allow_force_pushes.enabled == false and .allow_deletions.enabled == false' "$POLICY_EXCEPTION" >/dev/null
git -C "$SWARM_ROOT" fetch origin main
test "$(git -C "$SWARM_ROOT" rev-parse refs/remotes/origin/main)" = "$SWARM_BEFORE"
```

If `required_linear_history.enabled` is `true`, the owner must use the
approved control plane to apply the narrow temporary exception, save its
response as `POLICY_EXCEPTION`, and then allow the guarded push. If it is
already `false`, save the observed effective state as the exception receipt
anyway; no policy mutation is needed. In both cases the owner approval must
contain the exact `ALLOW_ANCESTRY_PRESERVING_BACK_SYNC` line. Restore the
recorded before-state immediately after transport. Never force-push, squash,
rebase, or merge unrelated work.

```bash
# [EXTERNAL-PUBLISHING] repo=swarm remote/policy; owner-approved K transport only
git -C "$SWARM_ROOT" push origin "back-sync/${VERSION}:refs/heads/main"
```

```bash
# [READ-ONLY] repo=swarm remote; bind swarm main to K after transport
git -C "$SWARM_ROOT" fetch origin main
test "$(git -C "$SWARM_ROOT" rev-parse refs/remotes/origin/main)" = "$K"
```

```bash
# [LOCAL-MUTATING] repo=swarm policy; capture the restored policy response
gh api repos/EffortlessMetrics/ripr-swarm/branches/main/protection > "$POLICY_AFTER"
```

```bash
# [READ-ONLY] repo=swarm policy; prove normal enforcement is restored
jq -e --slurpfile before "$POLICY_BEFORE" '.required_linear_history.enabled == ($before[0].required_linear_history.enabled) and .allow_force_pushes.enabled == false and .allow_deletions.enabled == false' "$POLICY_AFTER" >/dev/null
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
git -C "$SWARM_ROOT" switch --detach "$K"
git -C "$SWARM_ROOT" branch -d "back-sync/${VERSION}"
git -C "$SOURCE_ROOT" switch --detach "$SOURCE_RELEASE_HEAD"
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
