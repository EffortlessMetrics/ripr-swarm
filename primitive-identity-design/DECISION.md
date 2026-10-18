# Primitive identity decision for #5500 / target 67654dc6

Decision: **NOT_ESTABLISHED on the unchanged consumer with existing supported facts.**
Do not implement an admission relaxation. Stop at the missing authority below.
This is a design result, not a new compiler receipt or a consumer fix.

## Frozen subjects and inspection

- Producer: c4a8f7828e15426c4d872718f66bd4bac3ab0daf,
  tree 58e19fdafaa9a83509a50c8402b13211a34eefdc.
- Consumer: ub-review 278253a03b00d1aa777eeba7633954122163890f,
  tree 6e288c65d71e1a7f7f38fa064221e53163c9b47d,
  base 715a36434a56be3a9abeb5545012d56e2cb9cb60.
- Inspected current workspace main 9d3fbe9fcf3dbc5913d88c315afa177a7dec4ac9;
  the frozen whole-tail adapter is separate unintegrated source.
- Canonical [owner record](https://github.com/EffortlessMetrics/ripr-swarm/issues/5500#issuecomment-6252014142)
  and [immutable acceptance contract](https://github.com/EffortlessMetrics/ripr-swarm/blob/51324e92c482f4f27d7eac7eec4a5af9c547433d/veto-reconciliation/ACCEPTANCE-CONTRACT.md)
  govern this result.

Read actual source in clean isolated checkouts; no consumer build, analyzer replay,
proc macro, build script, production edit or new product test was executed.
An absolute toolchain probe found rustc 1.95.0 (59807616e); the initial PATH-only
rustc lookup failed and the rustup shim used an unavailable default home.
Those setup outcomes do not establish a compiler limitation.
The direct toolchain binary works; rustc --help advertises metadata and MIR emission.

## Actual source edges

[budget.rs:5](https://github.com/EffortlessMetrics/ub-review/blob/278253a03b00d1aa777eeba7633954122163890f/src/proof/budget.rs#L5)
imports crate::*; its owner at line 95 returns bare u64.
[proof/mod.rs:9-17](https://github.com/EffortlessMetrics/ub-review/blob/278253a03b00d1aa777eeba7633954122163890f/src/proof/mod.rs#L9)
imports crate::* and reexports budget::*.
[main.rs:23-47](https://github.com/EffortlessMetrics/ub-review/blob/278253a03b00d1aa777eeba7633954122163890f/src/main.rs#L23)
imports/reexports cli::*, config::*, proof::*, sensors::* and further modules.
proof/mod.rs contains serde derives including ResourceLease; cli.rs:7 has
clap's Parser derive. These are actual namespace edges, not a claim that a
particular published serde/clap expansion emits a custom u64.

The producer's whole_tail_match.rs:43-50 refuses Any/absent candidate facts.
Line 64 separately requires primitive_namespace_is_closed; syntax/owner_pin.rs:3245
reuses extern/import/expansion closure, whose glob branch refuses crate::*.
MacroBindingCandidates (syntax/owner_pin.rs:578) is explicitly a prefilter over
trusted macro names, not a type-binding table.
The local assertion visibility helper deliberately ignores ForeignGlob in one
prefilter (same file:300); reusing it as a primitive receipt would remove authority.

## Approach comparison

| Approach | What it can establish | Result on the actual unchanged source |
| --- | --- | --- |
| Existing parser/module/export facts | Closed source-visible namespaces with proven imports and supported attributes | Stops at crate-root export closure and opaque derives. A toy local-glob positive cannot complete this consumer. |
| Fresh rustc MIR/metadata/type query | Compiler-resolved primitive identity after correct expansion/type checking | Normal compilation runs proc macros; Cargo may also run dependency build scripts. --emit selects output, not a no-execution analysis mode. Omitted/stubbed derives compile a different program. Not admissible here. |
| Existing trusted compiler artifact | Potentially reusable without executing extensions again, if its typed identity, exact source/configuration and producer provenance are independently established | No such exact-consumer receipt is present in the supplied packet or fresh consumer checkout. Ordinary test exit, binary hash, dep-info or arbitrary .rmeta/MIR spelling is insufficient. Private owner/source mapping must also be available. |
| rust-analyzer with proc macros/build scripts/check commands disabled | Source analysis that avoids those executions | The missing generated exports remain unknown. A displayed inferred type cannot certify absence of an opaque competing binding. This is a second resolver dependency, not an existing RIPR fact. |
| Source-qualified primitive anchor | Bounded source authority if an actual owner type is anchored to the genuine sysroot primitive and cannot be rewritten | Current owner has no anchor. Changing it would change consumer source/input and require separate consumer authorization and a supported producer path. A hypothetical anchor does not qualify this head. |

The [Rust Reference](https://doc.rust-lang.org/reference/procedural-macros.html)
states that derives append items and procedural macros run during compilation.
The [compiler expansion guide](https://rustc-dev-guide.rust-lang.org/macro-expansion.html)
explains integration of generated AST and name resolution.
The [rustc output options](https://doc.rust-lang.org/rustc/command-line-arguments.html#--emit-specifies-the-types-of-output-files-to-generate)
describe metadata/MIR outputs; they provide no source-bound PrimitiveScalarIdentity receipt.
The [rust-analyzer configuration](https://rust-analyzer.github.io/book/configuration.html)
documents that procedural macro support implies build-script support.
No such execution was attempted in this task.

## Concrete receipt contract and invalidation boundary

A usable receipt is an internal authority object, not a user-asserted boolean.
It must have a real qualified producer. Until that producer exists, its value
is absent and all current refusals remain.

Required subject:
- exact crate target, module identity, owner identity and return-type byte span;
- source bytes/digests, parse/index generation and complete source/module custody;
- resolved builtin integer identity (for this owner: Uint/U64), rather than text;
- evidence mode and complete dependency/provenance closure for that mode.

Source mode must prove each imported/reexported name edge, unique binding,
ancestor/prelude and attribute/macro noninterference. Opaque foreign expansion,
alias, include/path/cfg ambiguity, lexical fallback or incomplete index fails closed.
For a separately authorized qualified-anchor mode, prove the external root is
the real core/std sysroot crate under the exact edition/extern mapping; the path
alone is insufficient. Prove that the exact owner/module is retained and unrewritten.

Compiler mode would need authenticated already-produced facts tied to exact
rustc commit/sysroot/target, Cargo target/features/cfg/test mode, lockfile and
resolved dependency identities, extern mappings, flags, source/generated-input
digests and expansion/build-output provenance. A narrow typed owner query must
distinguish a builtin TyKind::Uint(U64) from an ADT/type alias and bind its
source origin. No text-MIR-u64 matcher, arbitrary cached binary, or untrusted
self-issued receipt is accepted. No such producer is installed by this design.

Consumption must compare all subject inputs before granting the one capability.
Owner identity never proves an assertion's MAX identity: that requires a separate
assertion-scope expression/type/associated-item receipt. It also never proves
assertion binding, execution, call/result flow or whole-tail/diff/new-owner custody.
The five StoredAny refusals can only be refined after their exact relevant
noninterference obligations are independently discharged, not by a blanket bypass.

Invalidate on changed source/span/owner/module/import/export/expansion inputs,
cfg/features/target/test mode, dependencies/lockfile/extern mapping, compiler or
sysroot, receipt producer/version, incomplete coverage or changed index membership.
Invalidate transitively through the relevant export graph; source-file-only hashes
do not cover changed ancestor or dependency exports. Do not persist positive
results in legacy classified caches. Any later implementation must update all
full/compact/sharded generations and consumers if classification changes; changing
persisted file-fact shapes also needs its own version/compatibility migration.
Current c4 versions and public wire schemas remain unchanged in this task.
Missing, unsupported, malformed, wrong-owner, stale or mismatched receipts refuse.

## Negative controls and claim limits

Retain the immutable [12-case packet](https://github.com/EffortlessMetrics/ripr-swarm/blob/51324e92c482f4f27d7eac7eec4a5af9c547433d/veto-reconciliation/README.md)
and its [independent assessment](https://github.com/EffortlessMetrics/ripr-swarm/blob/51324e92c482f4f27d7eac7eec4a5af9c547433d/veto-reconciliation/independent-assessment.txt):
closed primitive/literal positive; owner crate-glob; separate numeric/MAX pair;
custom imported u64 with always-true equality; overwritten actual; self-derived
expected; raw/renamed/custom bindings; ancestor derives and macro-use preludes;
opaque/foreign/partial/lexical/stale/wrong-owner provenance. Do not alter expected
results to accommodate a design. The custom-type original and zero mutant both
pass and must remain refused; numerical coincidence is not discrimination.

The published 24 compilations/12 originals/9 rejected zero mutants/3 passing zero
mutants are retained prior evidence, not rerun here. The frozen all52 comparison
and 18 current weak findings remain the last qualified consumer observation.
No fresh 52-row replay, class movement or 18-to-17 claim is made.

## Missing authority and next choice

The missing authority is an **exact-source, exact-scope builtin return-type binding
through the real root/proof expansion/export graph**. Existing supported syntax
facts cannot certify that edge, and obtaining it by fresh expansion is excluded.
There is no sound actual-consumer executable proof-of-feasibility to deliver
under this scope, so none was fabricated.

Canonical root/#5500 should keep this target held. Resume only when either:
1. a separately accepted producer supplies an authentic already-derived typed
   receipt for the unchanged consumer with the contract above; or
2. a separately authorized consumer/source premise supplies a genuine qualified
   primitive anchor, followed by an actual supported producer witness.

Neither is present or authorized for implementation here. After either changes,
independently resolve assertion MAX, StoredAny relevance and remaining flow gates,
then run the exact all52 consumer comparison. #8772 cb6f7667 remains disclosure
parity and #8597 5aa226a1 remains expected-word rebinding; neither is reassigned.
Luna's bounded read-only adversarial assessment agrees with this authority gap.
No PR, hosted CI, Ready, merge, release, security/permission or credential change.
