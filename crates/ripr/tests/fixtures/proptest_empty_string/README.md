# Original empty-string property (#10843)

The subject is the unchanged `empty_string` property in
[`EffortlessMetrics/perl-lsp-swarm@6ffb36a1d2eff471407badf15721452696850d68`](https://github.com/EffortlessMetrics/perl-lsp-swarm/tree/6ffb36a1d2eff471407badf15721452696850d68).
The entire property source remains here, including its four unsupported
Unicode/loop siblings and independent ordinary test. The selected property is
not rewritten to canonical proptest paths or a simpler assertion.

These source files are byte-for-byte copies:

| Fixture file | Original path | SHA-256 |
| --- | --- | --- |
| `input/tests/prop_utf16_roundtrip.rs` | `crates/perl-position-tracking/tests/prop_utf16_roundtrip.rs` | `ab7a07b1cd73b1d04046791e61b646b9ec7c415a30a5b9559982b2de892783fb` |
| `input/src/convert.rs` | `crates/perl-position-tracking/src/convert.rs` | `1599df911d62d53b6f4e6e9390cf2805e88a552da2a55971bf6e4a6e0b7ed9ae` |
| `input/perl-test-generators/src/lib.rs` | `crates/perl-test-generators/src/unicode.rs` | `cc381aa2311db6282248632c1b6771dd8d67a3de919cdab5cb6455331c5e0530` |

The standalone fixture manifests and root library are test scaffolding. They
retain the actual converter module and its root re-export, the actual Unicode
generator used by the other properties, and the audited proptest 1.11.0 registry
provider. They omit unrelated workspace crates and add an independent ordinary
control. This is a reproducible source integration test, not a new measurement
of the original whole-workspace native selector.

Original-workspace native collection, correct/wrong/restored execution and the
nonzero exact selector are already retained in
[#10843 comment 6440237298](https://github.com/EffortlessMetrics/ripr-swarm/issues/10843#issuecomment-6440237298).
Those observations establish only this empty-input property. Neither those
observations nor this fixture establishes general Unicode strategy coverage,
model reasoning, arbitrary property frameworks or real-task repair yield.
