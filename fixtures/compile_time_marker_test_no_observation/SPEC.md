# Fixture: compile_time_marker_test_no_observation

Spec: RIPR-SPEC-0108

Issue: #10091

## Given

The changed private method `Wrapper::get` returns `self.inner.get()`, a call
to `Cell::get` that shares the owner's name, so some source outside the owner's
`fn` line names `get` and reach cannot be ruled out. The only test in the owner's
file is `wrapper_is_send`, whose whole body is the compile-time trait-bound check
`assert_send::<Wrapper>()`: a turbofish call of an empty generic helper with no
value argument. It calls nothing that reaches `get` and checks no runtime value.

## When

```bash
cargo xtask fixtures compile_time_marker_test_no_observation
```

## Then

The finding is `reachable_unrevealed` with observation `no`: the test is linked
only by `same_test_file` and has no assertion, so it is listed with a
`no_assertion` miss and credits no oracle. It is never `weakly_exposed` with
`observation yes`.

## Must Not

- Read a zero-argument `assert_*::<T>()` compile-time check as an oracle.
- Credit observation, or lift the class to `weakly_exposed`, from a same-file
  test that runs no code of the owner and checks no value.
- Claim runtime mutation adequacy.

## Limits

The refusal is by shape, not by callee: every zero-argument turbofish call is
dropped, including ambient-state helpers (`assert_valid::<Config>()`), method
forms (`checker.assert_ready::<T>()`) and `assert_ok::<T>()?`, a known
fail-closed over-refusal. The check is line-based and reads only the first callee of a line, so a
multi-statement line (`foo(); assert_send::<T>();`) is judged by `foo`; the
lexical extractor read no fact from it before and reads none now (fail-open,
other statement-splitting paths not measured).


Reach stays `weak` (not `no`) because `get` is named elsewhere in the source
(`self.inner.get()`); `owner_may_be_reached_unseen` does not tell a call of
another type's method from a call of the owner (#10091 follow-up). Renaming the
owner makes reach `no` and the class `no_static_path`.
