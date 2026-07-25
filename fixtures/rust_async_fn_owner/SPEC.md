# Fixture: rust_async_fn_owner

Spec: RIPR-SPEC-0001

## Given

Production code changes the predicate boundary in an `async fn` from `>=` to
`>`. The related test is a `#[tokio::test]` that calls `.await` on the async
function and asserts the exact return value with `assert_eq!`.

## When

ripr analyzes the diff against the workspace.

## Then

The analyzer resolves the `async fn` owner (`fetch_limit`), reaches it from
the `#[tokio::test]` oracle, and identifies the strong `exact_value` oracle.
Classification is `propagation_unknown` because the analyzer cannot statically
trace `.await` return propagation to the assertion.

## Must Not

- Must not over-credit as `exposed` — `.await` propagation is unknown, not
  established.
- Must not fail to resolve the async owner or miss the test.
