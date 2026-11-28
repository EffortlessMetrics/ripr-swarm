# Fixture: rust_async_future_unpolled

Spec: RIPR-SPEC-0240

## Given

`async fn fetch_len(s)` changes its return from `s.len()` to `s.len() + 1`.
A synchronous `#[test]` calls `fetch_len("abc")` and binds the future to `_f` without `.await`, an executor or an async test attribute, so the async body never runs. This is the `rust/async-fn` repair-start benchmark case.

## When

`cargo xtask fixtures rust_async_future_unpolled` runs the public analyzer and compares JSON and
human output.

## Then

Exactly one `return_value` finding on line 2 reads `static_unknown` with `static_limit_kind` `rust_future_never_polled`. Reach reads `unknown` ("future created but never polled") instead of `yes`, infection and propagation are not established, and the canonical next action is `unsupported_or_limited`. No repair start is offered and no runtime is chosen.

## Must Not

- Claim reach, infection or a `reachable_unrevealed` gap for a body that never runs.
- Choose an async runtime or start a repair.
- Claim a runtime mutation outcome from static evidence.
