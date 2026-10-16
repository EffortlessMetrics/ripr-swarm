# Independent #8147 review: REPAIR_REQUIRED

Reviewed `EffortlessMetrics/ripr-swarm` head **425317d867f4639ad5ec878c3ba44c82730d7af7**, tree **ba7d480ccfcbe7f6acfcdf6a4191c34fa8a0f90e**, parent/merge-base **20022bbfeb0d29882f97f51dc5f83ead26919dd4**. PR base observed **116598e5207372cdca620a8931600dcb8890b514**. Inspection and execution: 2026-10-16 UTC, Linux x86_64, Rust 1.95.0 (`59807616e1fa2540724bfbac14d7976d7e4a3860`), locked/default features, dev/test debug=0. Submitted source and inherited checkout remain unchanged.

## Findings and ownership

These are **independent confirmations of existing-owner findings**, not new competing issues or repairs:

1. **P1: matcher tokens revoke real assertion credit.** `syntax/owner_pin.rs::macro_body_bindings` flattens all definition token trees (1252–1256), including matchers. `macro_rules! m { (use provider::assert_eq;) => {}; }` emits no import, yet changes an ordinary native-discriminating owner assertion from `exposed` to `static_unknown`. Exact scanner control fails because `names == ["assert_eq"]`, expected empty. Existing [thread 4290291518](https://github.com/EffortlessMetrics/ripr-swarm/pull/8147#discussion_r4290291518); matcher follow-up also already recorded in [#8149](https://github.com/EffortlessMetrics/ripr-swarm/issues/8149). Read transcribers only while retaining nested emitted bindings.
2. **P1: private glob body escapes its crate.** `macro_binding_ambiguities` passes `crate_local=false` for every `BodyBindsAny` (667–676); `classify/owner_pin.rs::trusted_macro_sites_in` consequently inserts it globally. A private macro and invocation in `benches/other.rs` containing `use provider::*` changes the library's genuinely discriminating test to `static_unknown`. A direct glob in that same bench and a private macro emitting a named import both remain `exposed`. An exported-body control refuses as expected. Existing [thread 4290291614](https://github.com/EffortlessMetrics/ripr-swarm/pull/8147#discussion_r4290291614). Derive reach from actual export/visibility and preserve target-root isolation.
3. **Direct-leaf omission confirmed, without a false-exposed claim.** `use $n;` has `$` at `start`, failing the `at > start` condition at 1327. The exact extracted scanner fails an expected `binds_fragment.is_some()` assertion; `use provider::$n;` passes, and `use $n as check;` correctly binds no caller-selected name. Native `($n:path)` / `vec!(provider::assert_eq)` with the test emitted in the same expansion compiles and accepts both correct and wrong owner values. The alias-away twin detects the wrong value. However, the analyzer returns **`no_static_path` with zero related tests** for these trusted-name generated-test shapes; this experiment does **not** establish a false `exposed`. Existing [thread 4290303425](https://github.com/EffortlessMetrics/ripr-swarm/pull/8147#discussion_r4290303425). Check effective leaves, rename-away cases and `$crate` separately.

No novel product issue is asserted. Verified-drop-in behavior remains separately owned by #8149 and was not re-executed here. #8004 remains owned separately: observed Draft head **8e0444b1a62ead18584d40b18278c6e8716dc56c**. This review neither substitutes for its review/integration proof nor modifies its candidate, cache work, performance findings or ownership.

## Independent execution

Eleven fixtures each compile correct and wrong native variants successfully and execute exactly one test per variant: **22 native test runs**. All correct variants pass. Wrong variants fail in eight discriminating fixtures, pass in the three no-op assertion fixtures. Each exact-head CLI report has exactly one `return_value` finding. Native sources/binaries live outside analyzed fixture roots, preventing unrelated-source pollution.

| Case | Wrong native | Static classification |
|---|---|---|
| plain | fails | exposed |
| matcher only | fails | static_unknown |
| emitted named import | passes | static_unknown |
| direct leaf path | passes | no_static_path |
| effective path leaf | passes | static_unknown |
| direct alias away | fails | no_static_path |
| private glob, other crate | fails | static_unknown |
| private named import, other crate | fails | exposed |
| direct glob, other crate | fails | exposed |
| exported glob, other crate | fails | static_unknown |
| private glob, same crate (`std::io::*`) | fails | static_unknown |

The six exact scanner component tests compile against the exact-head `ra_ap_syntax` dependency and verbatim extracted production scanner/helper slices: **4 pass / 2 intended assertion failures**, native exit 101. This is component evidence, not a replacement implementation or a whole-analyzer pass. The path-leaf and emitted-import positives discriminate against removing all binding detection; the matcher and unrelated-root controls discriminate against refusing everything.

Unmodified repository command exits zero: `cargo test --locked -p ripr --lib -- macro_body stored_macro_candidates_record_each_binding_construct a_crate_local_binding_in_another_target_does_not_reach_the_test`: **6 passed, 0 failed, 0 ignored, 11041 filtered**. The existing PR tests therefore do not reject these independently demonstrated defects.

The CLI build exits zero; SHA256 **6dd285dec123615fc0c9ea519c51db182275e29f4c85d6ef7520b8981d6fefb5**. Offline setup first failed on missing registry package `home`; the locked online build succeeded. Initial fixture attempts exposed Rust E0659 for macro-generated imports used outside the expansion and then source-root pollution from native twins; neither is counted as candidate failure. Final fixtures emit the shadowed test in the same expansion and place native sources outside indexed roots. Earlier partial matrix logs are retained as superseded instrument evidence.

## Replay and limits

Unpack `controls.tar.gz` into a new directory. `run.py` uses only Python's standard library to orchestrate Rust fixtures; the behavior and native controls are Rust. Build the reviewed clean checkout using:

```bash
export PATH=/workspace/.cargo/bin:$PATH
export RUSTUP_HOME=/workspace/.rustup CARGO_HOME=/workspace/.cargo
export CARGO_TARGET_DIR=/workspace/review8147-target
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
mkdir -p /workspace/review8147/target
cargo build --locked -p ripr --bin ripr
RIPR_REVIEW_BIN="$CARGO_TARGET_DIR/debug/ripr" python3 /path/to/unpacked/run.py
RIPR_REVIEW_SOURCE=/workspace/review8147 python3 /path/to/unpacked/extract.py
```

Compile generated `syntax-controls.rs` with `rustc --edition=2024 --test`, `-L dependency=$CARGO_TARGET_DIR/debug/deps`, and `--extern ra_ap_syntax=<that target's libra_ap_syntax-*.rlib>`; execute with `--nocapture`. Expected final outcome is four passes and the two named failures. Scripts default to this review's `/workspace` toolchain paths; adjust those existing paths for another host. The archive contains complete fixture bytes, reports, native compile/run outputs, extraction hashes, build and unit logs; `archive-inventory.json` binds every member. Executables remain local witnesses, with their digests in the receipt; rebuilding is not byte-identity authentication.

Observed hosted [job 116780497752](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/38840869131/job/116780497752) checked out **f49a166040664644e20a97bc1f7d199026a71092**, the merge of reviewed head into observed base. Its required-test step passed, but **PR evidence failed after a 3600-second analyzer timeout**, so no clean hosted qualification is claimed. `ci.json` retains the relevant log lines/steps. Connector inventory is first-page-only; no complete protection/current-required-check clearance, current-main composition, native Windows, broad performance result or released-bundle acceptance is established. No CI was dispatched or rerun.

Next: existing #8147 delivery owner repairs matcher/transcriber separation and body reach, adjudicates the direct-leaf boundary with these native limits, then refreshes exact-head proof/review. Keep #8149 and #8004 ownership intact. No candidate source push, Ready, merge, thread resolution, bot contact, release/deploy, credential/security change or destructive cleanup occurred.
