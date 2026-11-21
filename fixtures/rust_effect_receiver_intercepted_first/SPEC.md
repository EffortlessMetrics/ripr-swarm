# Fixture: rust_effect_receiver_intercepted_first

Spec: RIPR-SPEC-0108

## Given

A free owner sends its changed payload into a fresh `RefCell<Vec<String>>`.
The test reads `first().map(String::as_str)`, but an in-scope `First` trait
intercepts `Vec::first` before slice dereferencing and returns a fixed string.

## When

Run `cargo xtask fixtures rust_effect_receiver_intercepted_first` and the
`projected_effect_payload_requires_fresh_state_and_a_preserving_map` native
controls in `owner_pin_execution`.

## Then

Both call-deletion and changed-argument findings stay weakly exposed. The
assertion does not discriminate the effect: the correct and deliberately wrong
producer each compile and pass one real Rust test. The matched fresh receiver
without interception remains a positive control in the integration test.

## Must Not

- Treat a field mention as observation through an intercepted collection read.
- Credit seeded elements, constant maps or a shadowed `String::as_str`.
- Re-bless exposed output or claim mutation adequacy from a static finding.
