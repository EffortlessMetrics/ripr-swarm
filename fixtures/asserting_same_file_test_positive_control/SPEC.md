# Fixture: asserting_same_file_test_positive_control

Spec: RIPR-SPEC-0108

Issue: #10091

## Given

The same changed `Wrapper::get` owner as `compile_time_marker_test_no_observation`
plus the same compile-time-marker test `wrapper_is_send`. A second test in the
owner's file, `wrapper_reads_back`, calls `get` and asserts the value it returns.

## When

```bash
cargo xtask fixtures asserting_same_file_test_positive_control
```

## Then

A real value assertion on the owner's result is still credited beside the marker
test: the finding is `exposed` through the `direct_owner_call` test and is not
demoted to the assertion-free reading of the paired fixture. The marker test
earns nothing and withdraws nothing. This guards against a fix that withdraws
credit from every test in the owner's file.

## Must Not

- Treat the compile-time-marker refusal as a refusal of ordinary assertions.
- Claim runtime mutation adequacy.
