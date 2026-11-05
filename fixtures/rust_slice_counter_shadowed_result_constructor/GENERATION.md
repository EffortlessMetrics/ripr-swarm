# Initial expectation provenance

Reason: RIPR-SPEC-0186 / RIPR-SPEC-0108 require a real-producer constructor-shadow negative. Both adjacent-length assertions pass with either predicate because the local success spelling supplies the firing error; removing that shadow makes the wrong predicate fail.

These initial files come from verified exact candidate `edf86b3674bb850f894a9303f33311e23ad6ae5f`, tree `f4c0124e1505eda92f74cb5b4fc9ff25773cdc1b`, automatic wheel run `41987919036`, artifact `13397278264`. API ZIP SHA256 is `337b8f5ef4d2286929039d4eef7d33cf0fd8531503d544be87f4d200f5ef12cc`; wheel SHA256 is `288691108a4883c0026a3771a90f7737b19b340100d1e2550438daa806f19528`; native payload SHA256 is `b8354a4944089090343821ca59e33fbbf90e8a59d574a9946ddde900f56e792f`. Candidate/tree/Cargo.lock receipt, complete wheel RECORD and embedded version were checked before execution.

From a scratch root with exact fixture input/diff bytes, the canonical Fast commands were:

```sh
ripr check --root fixtures/rust_slice_counter_shadowed_result_constructor/input --diff fixtures/rust_slice_counter_shadowed_result_constructor/diff.patch --mode fast --json
ripr check --root fixtures/rust_slice_counter_shadowed_result_constructor/input --diff fixtures/rust_slice_counter_shadowed_result_constructor/diff.patch --mode fast --legend
ripr check --root fixtures/rust_slice_counter_shadowed_result_constructor/input --diff fixtures/rust_slice_counter_shadowed_result_constructor/diff.patch --mode fast --format human-full
```

Runs used Rust 1.95, Rayon 1 and a dedicated cache cleared once before the three surfaces. Every command exited 0. Analysis is complete with exactly one predicate finding, `infection_unknown`. Native controls and analysis inputs are kept separately: no diagnostic owner copies are inside the analyzed input root. Owner assertions remain on separate lines, matching the authored public regression.

The exact predecessor `e08e4f97b5b7a4f5a4b5331da98a059e44f16270` produces `exposed` for this identical fixture in both Fast and Ready modes, so this is a real old-Fast discriminator. Four native Rust controls distinguish the mask: both shadowed predicate versions pass; canonical correct passes and canonical wrong fails. Fourteen repaired actual Ready CLI controls retain canonical enum/literal positives while refusing all masks. Independent imported-alias controls also preserve canonical exposure and refuse shadowed constructors with and without a fallible suffix.

Normalization compiled and executed unchanged canonical fixture helpers from `xtask/src/reports/fixtures.rs` blob `4a3a22dc8dc066f3b04a9beb16cd80f9af5e2f41`; the normalizer program SHA256 is `cf0a0e590223536de8da74c1c37d5bc5f127d1b28d50969b1af063f47019ad64`. The actual output-integrity validator also passed in isolation. No verdict, count or expected output was manually adjusted.

Source SHA256: `b54f3c94b8753d621d103b30f25c7db4f8b7c36b30fc3f46ca60a4eeb7778bc3`.
Diff SHA256: `4fafe43c5ec359ee99971dafb91b551bb3889d6d206a82bad7c7f20cb6816f83`.

| File | Bytes | SHA256 |
| --- | ---: | --- |
| check.json | 8402 | 104bfcbaa4a446381d6fcf2d4e8a187dc944bdf34fe374bbaec1e712d53e7718 |
| human.txt | 4557 | 908a2381697c5d235f64ab83b34f635c22b3329288c14a9b9e17deed8803a1bc |
| human-full.txt | 4347 | 82945c5c419287d2c01602ab83b50507ae10b2017ee58c2e6cb3973028cc317c |

This is source-faithful initial expectation generation from a matching hosted artifact with canonical argv/cache and actual normalizers. It is not a `goldens bless/check`, corpus or dogfood execution. Those canonical gates and required current-head hosted qualification remain authoritative and pending. A later expectation-only commit does not relabel this edf execution as a run on the newer head.
