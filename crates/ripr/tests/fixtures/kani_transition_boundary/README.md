# Staged guard transition boundary (#12512)

This is a source-only correspondence package under #10749. It changes no
production parser, admission, comparator, output, cache, or native runner.
The existing #11517 inventory and #12596 asserted-owner producer keep their
owners and branches. The seven ordinary Rust tests are authored; compilation
and execution are **not established** in this environment. Kani was not run.

## Source basis and scope

Integration basis: `a1491e7b1f7ae90e66cee74fe3d8a04ac0db3e4a` on
`EffortlessMetrics/ripr-swarm`. Reuse the landed #12601 fixtures in
`../kani_guard_transitions/`; do not duplicate their exhaustive qualification
or call them an admitted guard producer. The fixed baseline is `strong.rs`,
SHA-256 `64fa5fd8248c1043ea6247caab18e00218793ac5cac608fc52f41ebf9dd26824`.
Its exact production prefix is:

```rust
pub fn fits(n: u8, max: u8) -> bool {
    n < max
}

```

That prefix includes two final newlines; SHA-256 is
`e407483c8caa5791d021c04128989d5ec96f4d2e9a3d68ff8b01987e48f0ef38`.
`proof_deleted.rs` omits the now-unused blank separator after the identical
function. Materialize each head as `src/lib.rs` against this fixed base. These filenames
are corpus identities, not claimed production owner resolution. Expected
logical owner is `src/lib.rs::fits`, with the authored strict-u8 capacity
requirement. Keep logical owner/behavior separate from changed evidence bytes.

## Two-sided cases and future correspondence obligations

| Head source | Fixed-base relationship | Required future observation |
| --- | --- | --- |
| `assertion_removed.rs` | Only sole result assertion removed; return discarded | Retain affected owner and named guard-change uncertainty; never resolve from absent head guard |
| `proof_deleted.rs` | 52 | `d7cae7846432586f48c100b7dd1b5ea8aef09c22e18024ce2e29031a2ad16c45` |
| `owner_deleted.rs` | Production owner deleted, old proof remains | No stale owner credit; named unresolved relation |
| `opaque_replacement.rs` | Assertion replaced by unsupported macro | Prior owner retained with uncertainty; no inferred strength or discharge |
| `wrong_owner.rs` | Unrelated call to root `fits`, actual assertion calls `decoy::fits` | First/bare-name match must not bind root owner to the decoy assertion |
| `relocated.rs` | Only harness module name changes | Exact authored mapping `proofs::pins_boundary` to `relocated_proofs::pins_boundary`; production must establish mapping before reuse; no blanket protection-loss claim |
| `disabled.rs` / `unknown_cfg.rs` | Only proof cfg changes | Disabled/unknown applicability remains explicit, never empty success |
| `body_provider.rs` | Body-local `kani` before symbolic inputs | Outer inventory alone cannot admit the proof-body relationship |
| `cochange.rs` | Both implementation and assertion change to `<=` | Rewritten expectation cannot establish improvement; evaluate accepted base obligation too |
| Existing `strong.rs` on both sides | Unchanged bytes | Preservation control; no new change or blanket loss |
| Existing `narrow_correct.rs` / `false_domain.rs` | Assumption changed, production identical | Preserve domain difference without solver inference; partial-spec sibling remains legitimate |

The independent fixture requirement is strict capacity. Authored ordinary Rust
execution covers all 65,536 pairs: assertion removal keeps the same answers,
while the co-change differs on exactly 256 equality inputs. Those counts are
test expectations, not observed execution here. The existing partial-domain
fixture admits 32,640 pairs while excluding every equality fault; that does not
make every partial specification a defect.

## Executable oracles and their limits

```sh
cargo test --locked -p ripr --test kani_guard_transition_boundary
cargo test --locked -p ripr --test kani_guard_transition_inputs
cargo test --locked -p ripr --test kani_proof_only_change
cargo test --locked -p ripr --test causal_delta_unknown
```

The new target calls the real public `compare_fixture_delta` for ordinary gap
repair and retained unknown gap states. Its test-only capture oracle rejects
missing head state, wrong owner, resolved/unsupported weakening attribution,
fixture-backed confidence and missing provenance. A missing head *gap* still
resolves under the existing gap contract; the oracle rejects that result as
evidence for a removed protective guard. This is no global comparator reversal.

These are authored oracle controls against existing DTOs, not a guard adapter
or model of proof semantics. They do not exercise actual guard extraction,
source currentness, cache reuse, receipt validation, JSON or MCP projection.
The same-HEAD test checks dependency-input differentiation only. Digest equality
does not authenticate execution, and digest inequality alone is not evidence
that a production cache refused stale reuse.

Production-path qualification must feed actual producer deltas into the oracle
and separately assert exact base/head consumed bytes, provenance and named
limitations. Keep the current `partial_with_limitations` disclosure, including
`verification_only_change` and applicable `proof_harness_not_evaluated`; do not
mint a probe to inflate a zero count. Preserve `native_execution=not_observed`
and `functional_protection=not_established`. Imported native success cannot
override static admission. CLI/JSON and read-only MCP each need actual execution
before parity is claimed. An intentional retirement requires #5224 authority.

## Smallest production continuation

After #11517 and #12596 are actually admitted, join existing two-sided diff
reconstruction with that producer's exact asserted-call relationship. Build
affected logical subjects from the union of base/head dependencies, retaining
an explicit unknown state for deleted or unsupported guards. Keep ordinary
gap disappearance semantics intact. Assertion removal can first surface an
owner-bound guard change with `comparison_unknown`; affirmative weakening is
still #10687's semantic responsibility. No parallel parser or policy engine.

Invalidate dependent evidence from consumed production/guard/helper/configuration
and captured-worktree identities even when Git HEAD is fixed. Qualify cold/warm
agreement and stale refusal in the existing cache/custody owner. Test source,
guard, helper, cfg and untracked worktree changes independently, then restore
exact bytes; source deletion without reconstructable mapping remains unknown.

## Publication and rollback

Opening a Draft currently starts `ub-review` with a 30-minute job timeout and
the wheel workflow with sequential 20-minute legacy jobs. Neither establishes
the requested complete-run 20-minute bound. Do not open the Draft or request
qualification until a compliant route is established by its existing owner;
do not change workflows, settings or protections in this package. A branch and
prepared PR body can be reviewed without starting those workflows.

Rollback removes only this test target, its fixtures, traceability and changelog.
No product support, release gate, publication or complete #12512 acceptance is
claimed. Formalization remains separate from an otherwise qualified release.

## Observed source checks (2026-11-06)

A Python source/Git check (exit 0) verified ten exact fixture relationships,
seven traceability symbols, rejected a deliberate implementation-byte change
under the unchanged-production oracle, and independently edited/restored five
inputs in a real disposable Git repository without moving HEAD. That repository
was cleaned up after every input restored to clean status. It did not run the
Rust tests, RIPR, a cache, a receipt validator or Kani.

- Fixed disposable Git HEAD: `08fff71dc9fee6f4e76c4b2ee65a1c9ad4067521`.
- Its tree: `45046313f6b8ec1c96ba3740206c15716088dd4e`.
- Commit author/committer: `Source Oracle <source-oracle@example.invalid>`;
  both dates `2026-11-06T00:00:00Z`; message `fixed transition source`.
- Source-check script SHA-256:
  `63e8a340dbbbf3a15105a3cbf885004243d95ed94ad40b439481dfb51b29df93`.
- Raw JSON report SHA-256:
  `acded997ca3d7693c739acc9400ee17f21edf297ed2cd3d64602282599aa0b62`.
- Those two temporary artifacts are retained under
  `/tmp/kani-transition-evidence/` in the authoring environment; a local path
  is not a remotely retained execution receipt. The source files below are
  durable inputs. Their byte identities can be checked with `sha256sum`.

| Source | Bytes | SHA-256 |
| --- | ---: | --- |
| `assertion_removed.rs` | 236 | `c92acabc639f1c27cdd5211734a1e7d793ffb39571f11cbb58a7966efc8a163c` |
| `body_provider.rs` | 269 | `4b9844aab523c330f8cd0a5b8af761593ecc3c44ef17ba0b1bc9f56b6d1cde9a` |
| `cochange.rs` | 251 | `17a53e10ec19cafd1421d340a468ca088988d05b5e4f3b40d68f6c8a9acd6e34` |
| `disabled.rs` | 261 | `bec7fd3bcc1e92bb565bd056630e081888115bfa09d486fd72484193b3be1920` |
| `opaque_replacement.rs` | 243 | `00cc809a08e2d84d7406a48685195e089d5e605b7b631864c07eaf9842586005` |
| `owner_deleted.rs` | 196 | `b1d5f8c18ee957e2866ba6d3a090a18666303dae2c6b6c5a5981b4b780c37f3f` |
| `proof_deleted.rs` | 52 | `d7cae7846432586f48c100b7dd1b5ea8aef09c22e18024ce2e29031a2ad16c45` |
| `relocated.rs` | 259 | `d9f68d352383c8aba84887a79f0c2c88fadcec0ab6b9230a703c0b66baa9cd09` |
| `unknown_cfg.rs` | 278 | `e6f3ad5e57307e6d216db55c8dc44bc4e3e504644adf18202106342f9a938c1f` |
| `wrong_owner.rs` | 386 | `dbb1f0beae603667ba85aa26534c7fadef9bb990d0cd7284062398ae15bb91e8` |

All five observations have the fixed HEAD above; the changed input commitments
are recorded independently of Git HEAD:

| Dimension | Path | SHA-256 of edited bytes |
| --- | --- | --- |
| guard | `src/lib.rs` | `c92acabc639f1c27cdd5211734a1e7d793ffb39571f11cbb58a7966efc8a163c` |
| implementation | `src/lib.rs` | `0a13122769563c631695f9d8123e455ed87fc667ca2fc1dc0ea0aa444945307c` |
| helper | `helper.rs` | `4f7b045b9f2b7a8937ba0f32e908c2472cfc5c9c2af37c685088e0e1f4b19558` |
| configuration | `ripr.toml` | `fc4386d9ab8b5bafe518e5b9d290e18ef58def1cfb150375bd650741166a6c2e` |
| untracked | `captured.txt` | `45354a4f97ef3d0d282e3b5fda4b2fe0f5dc6872c7f373fce9520ae96e5fd708` |

The helper/configuration/untracked inputs demonstrate source-input changes,
not admitted dependencies of this ordinary proof. Production qualification must
establish the actual dependency closure before claiming invalidation. No Rust
compiler, Cargo, rustfmt or Kani executable was found on PATH or in standard
Cargo/rustup locations and configured `/opt/codex` runtime directories. No tool
was installed. Formatting, Rust compilation/execution, policy gates and native
negative-control execution remain pending.
