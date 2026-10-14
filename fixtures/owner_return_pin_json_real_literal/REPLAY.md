# JSON construction admission replay receipt

Claim: #7776, bounded execution context under RIPR-SPEC-0197. No consumer
clearance, installed/package qualification, release or canonical propagation
claim is made.

The source-built API drivers invoke production `check_workspace` with Fast mode,
unchanged tests included, full construction-base diffs and JSON `render_check`.
Both use Linux x86_64 / Rust 1.95.0 and the same available dependency graph.
Base producer: `c96a7929f8c694eb62cff2dbe2ad7ee94beea815`.
The repaired producer contains this PR's production source. This execution
preceded the commit; its source hashes below bind carried-forward results to
reviewable bytes, rather than pretending the executable had a committed stamp.

Frozen didSave #17745: head `8a286993c78a9abfac38c69eb78498f926bde4a7`,
base `2e39ff72e70ada581d58be5e02e4230ba9d66963`.
Frozen documentHighlight #17789: head `63ab2520600a46305b840ad4eff5b36a4c0c9b65`,
base `fdc10428eed4a913368c3ac5eccc560a752f0067`.
Tracked consumer source and gate/pin/suppression settings were unchanged.

| Retained ID suffix | Before | After | Observe / discriminate | Remaining stop |
| --- | --- | --- | --- | --- |
| `f73080d4` | reachable_unrevealed | static_unknown | no / no | unresolved standard assertion binding from `pub use implementation::*` in perl-lsp-perltidy/native.rs:28 |
| `13ed5f30` | reachable_unrevealed | static_unknown | no / no | unresolved standard assertion binding from `pub use implementation::*` in perl-lsp-perltidy/native.rs:28 |
| `8b734a74` | reachable_unrevealed | reachable_unrevealed | no / no | unsupported `range()` interpolation in JSON operands |
| `06ffc95a` | reachable_unrevealed | reachable_unrevealed | no / no | unsupported `range()` interpolation in JSON operands |

The literal construction stage advances to the next existing binding gate for
f73080d4 and13ed5f30. The range-call cases remain refused. No row is exposed or
individually cleared. The six historical absent IDs remain non-claims.

## Matched controls

`cargo test -p ripr --test owner_pin_execution literal_json_constructor` passes:
ten correct and ten wrong callback subjects compile and execute one unignored
test each. Real root and nested literal cases reject the wrong callback at the
equality; eight ineffective controls pass. The real root case reads exposed;
the nested case has an admitted observation but remains weakly exposed under
the existing argument/pin authority. Others stay non-exposed.

Four grammar/binding/configuration tests pass; the complete owner-pin library
suite passes168 tests. The pure environment-casing test passes, with native
Windows execution unestablished. Global goldens and the honesty corpus pass.
The existing fallible-helper golden needed its pinned anyhow1.0.100 dependency
fetched; its expected result was preserved. No golden outside these new fixtures
changed. Commands and fuller logs/raw reports are retained in the executor under
`/workspace/scratch/7776-proof`; those paths are not delivered attachments.

## Execution identities

- `analyze-base` SHA256 `02481183bacaee7506021d8720518fef8931f0e5e44ab43938fb8298592ec7aa`
- `analyze-repaired` SHA256 `ce56a5244337dffa25304f52c43b8d5161e918a55fbb5afbe61c8bbd00b7af6f`
- `consumer-17789-after.json` SHA256 `fa67ad8eeedd2b16d6135fa41d536a576d62c2b8731879ebe7eb0d594c319de6`
- `consumer-17745-before-ready-sdk.json` SHA256 `a1d4f94e0a6f3e994a4e15a549c596c9a93e066db8949382b913c6bbccc9b639`
- `consumer-17789-before-ready-sdk.json` SHA256 `a855dc7baeb3bd318fa5938ee423a273ed6902c1b2cdcee49a25c16f7e1e4f4e`
- `consumer-17745-after.json` SHA256 `42222e800d7098c3f3f982f2e0202636605bf607c0b786a2394e5d8970875af2`

## Repaired production source SHA256

- `crates/ripr/src/analysis/classify/owner_pin.rs`: `bfb77690be48a12d48eb593862b27d2f4aaabff92e00307ec747a075852f7fe4`
- `crates/ripr/src/analysis/facts/drop_in.rs`: `190dc1b421aa7b156cf775db1e467630da02a33a8bc0023f27a99952a2203a99`
- `crates/ripr/src/analysis/syntax/owner_pin.rs`: `27b810396bda261f4fa29174d1fde88723478d10d65e142b27893293b1f06044`
- `crates/ripr/src/analysis/syntax/owner_pin/literal_json.rs`: `394e54fe965e059fe43046762646fb002215434b9f5bc1982c5d0bca09703bf2`
- `crates/ripr/src/analysis/workspace/assertion_dependency.rs`: `991fe1313690d6b7a82379d4dd9ded482e324f5f21077eba4a7911f09b56727a`
- `crates/ripr/src/analysis/workspace/mod.rs`: `8838ed21d33dc057767ff54baf7f148b65a3460b0a79734be03677ee28c2c2ec`
- `crates/ripr/src/analysis/seam_cache.rs`: `1597e7e8bcde716740ffa264405ab68e3174cd6a10615a5da51d891e9c0ccabd`

## Fresh review: workspace-member receipt race

Exact-head review of d94028ef reproduced an ordinary Cargo metadata race:
canonical baseline admitted; changing a member dependency to a local path after
metadata output appeared still admitted the stale registry graph. Root and
nested snapshots remained valid. This was accepted as a blocking defect.

The same candidate now hashes discovered member manifests around a second
bounded Cargo metadata probe, requiring unchanged graph bytes, manifests and
root authority. Missing or malformed manifest paths refuse. Using the repaired
module and the same ordinary Cargo adapter, the canonical baseline admits and
20/20 actual member rewrites during the discovery probe refuse. A separate
20/20 rewrites during the confirming probe also refuse. This establishes bounded
consistency detection under the existing worktree assumptions, not an atomic
snapshot against timed ABA rewrites. It does not replace the original consumer
replay with a claim of consumer clearance. Raw harnesses and results remain at
`/workspace/scratch/7776-review` for the lane handoff.
