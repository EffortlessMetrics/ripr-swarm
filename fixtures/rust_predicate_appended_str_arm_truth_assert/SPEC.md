# Fixture: rust_predicate_appended_str_arm_truth_assert

Spec: RIPR-SPEC-0197

Owner: analysis-fixtures

Issue: #15030

## Given

A `&str` predicate that is the owner's returned tail changes from
`flag == "-v"` to `flag == "-v" || flag == "--verbose"`. One test asserts
`assert!(is_verbose("--verbose"))`, the appended arm's literal, and on its
own line `assert!(!is_verbose("-q"))`.

## When

`cargo xtask fixtures rust_predicate_appended_str_arm_truth_assert` runs the
public diff analyzer.

## Then

Both findings read `exposed`. The truthiness assertion on the bool owner
pins its whole result, as for the integer twin `assert!(is_small(2))`.

## Must Not

- Read a string literal compared with a parameter that is not a plain string
  type (`&str`, `String`, `&String`): a newtype may carry its own
  `PartialEq<&str>`.
- Read an escaped, raw or byte string literal.
