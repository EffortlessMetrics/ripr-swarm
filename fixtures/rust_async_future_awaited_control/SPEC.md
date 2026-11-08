# Fixture: rust_async_future_awaited_control

Spec: RIPR-SPEC-0240

## Given

`async fn fetch_len(s)` changes its return from `s.len()` to `s.len() + 1`.
Discriminating negative control for `rust_async_future_unpolled`: the same owner change, but the test is a `#[tokio::test]` that awaits `fetch_len("abc")`.

## When

`cargo xtask fixtures rust_async_future_awaited_control` runs the public analyzer and compares JSON and
human output.

## Then

The finding keeps its existing reading (`reachable_unrevealed`, reach `yes`, no assertion observes the value); it never carries `rust_future_never_polled`.

## Must Not

- Name the never-polled limitation for an awaited call or an async test attribute.
- Credit an assertion the test does not make.
