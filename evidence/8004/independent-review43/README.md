# Independent exact-head review — PR #8004

**Disposition: REPAIR_REQUIRED**, reviewed published source `43f00b54eeb239c240bb71553c30a7bde3a4e080`, tree `b520b108591cf046c3498e6558604c9f6101eac7`.
This is an evidence-only commit, not another source candidate. No product source is changed. Rook owns public continuation and assignment to one source writer.

Read `REVIEW.txt` for the full findings, provenance, non-claims and next bounded repair. Minimal fixtures and complete raw reports for the key claims are retained below. The first-round full packet remains at commit `afbcaaf4c2c2d4d4919484ba0112fde281b03337`, path `evidence/8004/corpus-repair`.

- **Introduced:** same-named wrapper delegation loses genuine repository reach. Base reports helper/high; final reports file/medium and weak reach. `controls/wrapper_local` proves the loss even with a supported simple local receiver, avoiding field-flow inference. The native owner mutant fails the genuine wrapper test.
- **Inherited:** discarded operands of a module-level shadowed `assert_eq!` still receive direct/high diff credit. Native ignored-operand test survives the owner mutant. Repository full/compact demote this case; diff does not.
- **Inherited:** a foreign helper last hop still receives helper/high repository credit. In the foreign-only negative all four owner seams are `strongly_gripped` although no test executes the owner. Native mutant still passes. This remains a concrete false promotion despite the declared helper last-hop non-goal.
- Supported qualified variants and live-outer scope controls pass; expired-inner decoy is refused. Five independently replayed corpus inputs match retained hashes and preserve false_actionable accounting.
- The full-golden failure is retained historically. Missing `anyhow 1.0.100` fixture preparation is a sufficient setup cause. After the documented locked fetch in the same Cargo home, exact base/final emit equal complete JSON, exposed, matching committed findings/summary. A cache excluding only that dependency reproduces weakly_exposed. **No re-bless and no fresh full-goldens pass.**

## Minimal recovery

Create an isolated checkout at the reviewed SHA. Do not edit the source writer's checkout. Restore the toolchain from `rust-toolchain.toml` (1.95.0), retain default features, one build job, incremental off, dev/test debug 0; use one explicit Cargo home throughout. Build with `cargo build --locked -p ripr --bin ripr` and freeze the executable outside mutable Cargo output. `identity.json` records exact original final/base executable hashes; both reviewer rebuilds matched the author's witnesses byte for byte.

Each `controls/<case>/src/lib.rs` is a complete no-dependency Rust test crate source. Run `rustc --edition=2024 --test <source> -o <native>` and execute it. Compile the corresponding `native-sources/<case>/mutant.rs` similarly. The qualified Null-arm mutant is `native-sources/qualified/null-mutant.rs`; the Object-arm mutant does not affect a Null-only test. Set up a scratch crate with `[package]`, edition 2024 and an empty `[workspace]`, then copy one source to `src/lib.rs`.

Run the frozen CLI:
```
ripr check --root <scratch-crate> --diff <controls/case/diff.patch> --mode ready --format json
ripr check --root <scratch-crate> --mode ready --format repo-exposure-json
```
Keep mutation Rust files OUTSIDE the analyzer's scratch root; otherwise they are extra indexed subjects. Original native log paths precede the later move of mutant source files out of those roots; contents are retained under native-sources. `full-compact.log` comes from the additional observation test in `experiment.diff`; it asserts producer parity and emits four nonempty owner seams per case. It is an observation pass, not semantic approval. Its experimental tree identity is in identity.json and is not the submitted tree.

For the named golden, execute the documented preparation in the same Cargo home:
```
cargo fetch --locked --manifest-path fixtures/owner_return_pin_fallible_literal/input/Cargo.toml
ripr check --root fixtures/owner_return_pin_fallible_literal/input --diff fixtures/owner_return_pin_fallible_literal/diff.patch --mode fast --format json
```
The product metadata path remains offline. Run the applicable full golden gate after proper setup during the joined repair/proof wave; this packet establishes only the named fixture replay. Original failed full log remains in the prior immutable packet.

Next bounded repair: use module/source-aware assertion refusal consistently; distinguish declaration artifacts from executable helper calls; withhold foreign/unknown inherent last-hop receiver edges while preserving established same-name local delegation. Add honesty negatives plus genuine/native positives. Preserve five corpus truth labels, both integrated suites and gate policy. Fresh review is required on a new head. Keep Draft; required default-feature hosted qualification remains separate. No thread resolution, Ready, merge, source push, bot interaction or release is authorized by this packet.

Source integration: reviewed head parents cd141db8 + 67866bc9. Remote merge 50dbb658 has parents 42b064fe + 43f00b54 and tree 3ee0987f. Hosted feature job 116287116012 / run 38694285274 actually executed that merge: 10,379 passed, 2 ignored across its test/doctest results. This is lang-rust-only proof, not Routed Rust Small/default-feature qualification. Historical 2fa3df5b proof is not reused as current-head proof.
