# Fixture: rust_async_future_block_on_control

Spec: RIPR-SPEC-0240

## Given

`async fn fetch_len(s)` changes its return from `s.len()` to `s.len() + 1`.
Discriminating negative control for `rust_async_future_unpolled`: the same owner change, but the synchronous test drives the future with `futures::executor::block_on(fetch_len("abc"))`.

## When

`cargo xtask fixtures rust_async_future_block_on_control` runs the public analyzer and compares JSON and
human output.

## Then

The finding keeps its existing reading (`reachable_unrevealed`, reach `yes`, no assertion observes the value); it never carries `rust_future_never_polled`.

## Must Not

- Name the never-polled limitation for a future an executor drives.
- Credit an assertion the test does not make.
