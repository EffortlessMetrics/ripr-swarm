# Fixture: match_arm_shadowed_assert

Spec: RIPR-SPEC-0229

## Given

The unknown-option arm of a Cargo selection parser changes from ignoring an
unknown option to recording it. An out-of-line test calls the actual parser and
spells `assert!(matches!(actual, SelectionRequest::Unresolved(_)))`, but its local
`macro_rules! assert` evaluates and discards the condition without checking it.
The bounded source is extracted from `cargo_selection.rs` in UB PR #1417 at
`b19edcbeaedd84274514e6a5633aa42a3394e7cb`.

## When

```bash
cargo xtask fixtures match_arm_shadowed_assert
```

## Then

The caller remains related, but the MatchArm must not credit observation or a
strong oracle from the shadowed assertion. Shared parser execution admission
must refuse the invocation rather than interpreting its spelling as authority.

## Must Not

- Credit the discarded boolean as an executing standard assertion.
- Change fallible `ensure!` or helper-result admission under #4478.
