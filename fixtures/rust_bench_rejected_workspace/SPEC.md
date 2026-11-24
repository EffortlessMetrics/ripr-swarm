# Fixture: rust_bench_rejected_workspace

Spec: RIPR-SPEC-0356

Owner: analysis-fixtures

Issue: #15620

## Given

The bare billing package lives under an enclosing workspace that omits it from membership. Its only assertion is an unchanged test in benches/load.rs.

## When

Fast-mode analysis checks the change from doubling to tripling in fee. Cargo rejects this package during ordinary test discovery.

## Then

The return-value finding reads no_static_path and names no related test.

```expected-classes
probe:src_lib.rs:return_value:6ecab8eb -> no_static_path
```

## Must Not

Credit the bench assertion as exposure evidence for fee. The public API controls in lib_test_false_credit.rs cover an admitted package with an opted-in bench and a bench mounted by a runnable integration test.
