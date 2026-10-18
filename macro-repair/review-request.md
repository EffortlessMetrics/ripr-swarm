### Independent review: f4be8084 — REPAIR_REQUIRED

**[P1] A macro-defined return type can falsely receive whole-tail observation credit.**

Frozen source: `f4be8084a1cbc7cd9d66c7e10ec4b9eb46b7a5bc`, tree `7b181b6917993fb4622c22d7ae75642b746dbd00`, predecessor `16ff7479d2a78fd9558b4ff9f5ad6c803c2857d7`. Native Git checkout/source and current remote branch readback agree; the submitted source was not changed.

**Failure mode:** [whole_tail_match.rs:66](https://github.com/EffortlessMetrics/ripr-swarm/blob/f4be8084a1cbc7cd9d66c7e10ec4b9eb46b7a5bc/crates/ripr/src/analysis/classify/owner_pin/whole_tail_match.rs#L66) borrows [returns_primitive_scalar](https://github.com/EffortlessMetrics/ripr-swarm/blob/f4be8084a1cbc7cd9d66c7e10ec4b9eb46b7a5bc/crates/ripr/src/analysis/classify/owner_pin.rs#L9634), whose textual declaration/import scan does not establish primitive identity when an externally defined module-level macro introduces the return-type name. The matched-expression macro guard sees no macro inside the match. An imported macro can therefore introduce a struct named `u64` with a non-discriminating `PartialEq<i32>`.

The complete, valid Rust control below produces header `probe:src_lib.rs:match_arm:54eaf025`: **16ff = weakly_exposed; f4be = exposed**, with `whole_tail_match_return_observation` evidence. The original, selected-arm-zero, other-arm-zero and both-arm-zero native variants all compile (exit0) and pass the same test (exit0,1/1). The selected return changes from600 to0 without the assertion noticing. This is a new header promotion; existing individual-arm behavior is not attributed to this change.

**Repair request:** keep this new receipt fail-closed when an unexpanded module-level macro can supply the apparent primitive return name. Reuse existing namespace/provenance authority or explicitly bound this adapter; do not widen assertion admission or infer primitive identity from the return spelling alone. Add this compiled counterexample to the whole-tail API/CLI regression and honesty corpus. Preserve normal primitive positives, individual-arm selection and unknown distinctions. Confidence: **high / verified_current**. No repair implementation was made in this reviewer lane.

**Fresh validation:**9 whole-tail API tests and15 field-selector regressions pass. Exact PR1503 replay was independently repeated with the original exact16ff executable and this frozen candidate, both with native exit0 and complete analysis. Head `278253a03b00d1aa777eeba7633954122163890f`, base `715a36434a56be3a9abeb5545012d56e2cb9cb60`; diff40,491bytes/SHA256 `6af0470bd2c7a8648325402af4d5294c94f35304403ef2c3952ffe986adc8b02`. Both fresh outputs are byte-identical, SHA256 `5ec3414521d71fb8d827fad0d629f93ef4b2bb5fe7e9d40a376624a9e4f1d797`: all52 finding objects unchanged, zero added/removed/changed,18 current weak. Target67654dc6 remains weak. This within-review equality does not assert byte identity with historical output from different root/route metadata.

**Remaining consumer diagnosis:** inspected the author's [immutable reason log](https://github.com/EffortlessMetrics/ripr-swarm/blob/690975cff31ae56c7d8e28f93aa308c85d497bc4/evidence/consumer-diagnostic-reason.stderr) and current assertion-admission call path. They corroborate `MacroBinding(assert_eq)` for the direct600/601 pins. The saturation pin additionally has `independent=false`: current `independent_expected` refuses its file's `use crate::*` glob. A macro-admission change must not be assumed to clear this additional limit or establish18→17. These private diagnostic calls were not re-instrumented by this review; the unchanged final-artifact consumer outcome was independently reproduced.

The [author's final validation receipts](https://github.com/EffortlessMetrics/ripr-swarm/blob/690975cff31ae56c7d8e28f93aa308c85d497bc4/evidence/final-validation-receipt.json) remain evidence for their named scopes; the broader expensive suite was not duplicated after that handoff. Passing existing controls does not dispose of the new counterexample. No consumer clearance, CI, PR/Ready, merge, release/security change or destructive cleanup. All reviewer jobs are terminal; frozen witnesses remain retained.

<details>
<summary>Reproducible valid-Rust control</summary>

`src/lib.rs`:
```rust
mod foreign;
crate::define_scalar!();
fn charge(input: bool) -> u64 {
    match input {
        true => u64(600),
        false => u64(2),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed() { assert_eq!(charge(true), 600); }
}
```

`src/foreign.rs`:
```rust
#[macro_export]
macro_rules! define_scalar {
    () => {
        #[derive(Debug)]
        pub struct u64(pub ::core::primitive::u64);
        impl PartialEq<i32> for u64 {
            fn eq(&self, _other: &i32) -> bool { true }
        }
    };
}
```

Use a minimal Cargo package with edition2024 and an empty `[workspace]`. The actual analyzer input diff was:
```diff
--- /dev/null
+++ b/src/lib.rs
@@ -0,0 +1,14 @@
+mod foreign;
+crate::define_scalar!();
+fn charge(input: bool) -> u64 {
+    match input {
+        true => u64(600),
+        false => u64(2),
+    }
+}
+#[cfg(test)]
+mod tests {
+    use super::*;
+    #[test]
+    fn observed() { assert_eq!(charge(true), 600); }
+}
```

Native control command: `rustc --edition=2024 --test src/lib.rs -o native-tests`, then `./native-tests --nocapture`. Mutate `true => u64(600)` to `true => u64(0)` to demonstrate the selected-result hole; the retained variants also cover the other/both arms. CLI command: `<frozen-ripr> check --root . --diff input.diff --mode ready --format json`.

</details>

<details>
<summary>Durable independent receipt and native mutation outcomes</summary>

Receipt SHA256 `90505fd59239e1ea309264f7952b578d189af96d19a5fc8843ad81835a313be0` (UTF-8 JSON below including one trailing newline).
```json
{
  "reviewed_head": "f4be8084a1cbc7cd9d66c7e10ec4b9eb46b7a5bc",
  "reviewed_tree": "7b181b6917993fb4622c22d7ae75642b746dbd00",
  "base": "16ff7479d2a78fd9558b4ff9f5ad6c803c2857d7",
  "candidate_cli_sha256": "10b7f876571d3339d1d83c6e729a03158483e2f7a84682f2961a2f4f68ea9d03",
  "baseline_cli_sha256": "2134df0f89473955c2ed237cb46f7983b1c765b26cc348eb7924124aca1b0c24",
  "focused_api": {
    "whole_tail_passed": 9,
    "field_selector_passed": 15,
    "exit_code": 0
  },
  "finding": {
    "priority": "P1",
    "status": "verified_current",
    "id": "probe:src_lib.rs:match_arm:54eaf025",
    "old_class": "weakly_exposed",
    "new_class": "exposed",
    "new_evidence": "whole_tail_match_return_observation: exact invocation result pinned; no selected-input or individual-arm credit",
    "failure": "An externally defined macro introduces a struct named u64 with PartialEq<i32> that always returns true. WholeTailMatchObservation treats the textual u64 return as primitive. Original and selected/other/both arm-zero native controls all compile and pass the same non-discriminating test.",
    "location": "crates/ripr/src/analysis/classify/owner_pin/whole_tail_match.rs:66",
    "helper_location": "crates/ripr/src/analysis/classify/owner_pin.rs:9634"
  },
  "native_controls": [
    {
      "case": "original",
      "source_sha256": "84da46ed7a6a7cd4aeed9a96f65200dadad28d8bd200c38fa96369f4061f5f57",
      "compile_exit": 0,
      "test_exit": 0,
      "test_output": "\nrunning 1 test\ntest tests::observed ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n",
      "binary_sha256": "793468b46c216c5f01269704dbeb0f6a2a7299bdc5e9cbdc49f93a3b7af9d26a"
    },
    {
      "case": "selected true arm -> u64(0)",
      "source_sha256": "4f5034e3fed95cc10e4f9c3e16ffd5c9b9d70158c89bc9583662deeb13cad0ef",
      "compile_exit": 0,
      "test_exit": 0,
      "test_output": "\nrunning 1 test\ntest tests::observed ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n",
      "binary_sha256": "0da3d459dcbbe925410ee389358ec319208de696f5ea6ab9c303006309caf406"
    },
    {
      "case": "other false arm -> u64(0)",
      "source_sha256": "6ae7bd533e087c53cd3796b432bd63f4c0cd11fbf841090997e3bb66bbce4a4d",
      "compile_exit": 0,
      "test_exit": 0,
      "test_output": "\nrunning 1 test\ntest tests::observed ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n",
      "binary_sha256": "323036148f346a5929834e6b60a3edc0dda4e87c85e68d348bb68f557072ffe3"
    },
    {
      "case": "both arms -> u64(0)",
      "source_sha256": "a78efbba32862dc6fd6351f0a83a3ccbf247c20a9af86934cdefbe4dad664593",
      "compile_exit": 0,
      "test_exit": 0,
      "test_output": "\nrunning 1 test\ntest tests::observed ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n",
      "binary_sha256": "70608097f079b1e89b69d926452cee9241aeb20edbb03fbf8d2723f385344cfc"
    }
  ],
  "consumer": {
    "head": "278253a03b00d1aa777eeba7633954122163890f",
    "base": "715a36434a56be3a9abeb5545012d56e2cb9cb60",
    "diff_bytes": 40491,
    "diff_sha256": "6af0470bd2c7a8648325402af4d5294c94f35304403ef2c3952ffe986adc8b02",
    "both_output_sha256": "5ec3414521d71fb8d827fad0d629f93ef4b2bb5fe7e9d40a376624a9e4f1d797",
    "all_findings": 52,
    "unchanged_findings": 52,
    "added": 0,
    "removed": 0,
    "changed": 0,
    "current_weak": 18,
    "analysis_complete": true,
    "target": "probe:src_proof_budget.rs:match_arm:67654dc6",
    "target_class": "weakly_exposed",
    "hypothesis_18_to_17": "NOT_ESTABLISHED"
  },
  "disposition": "REPAIR_REQUIRED",
  "source_changed": false
}
```

</details>

