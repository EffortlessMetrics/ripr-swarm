# Fixture: rust_copy_builder_field_write

Spec: RIPR-SPEC-0228

## Given

Production code changes the by-value builder `Opts::retries(mut self, n)` from
`self.retries = n` to `self.retries = n * 2`. `Opts` is `Copy`, so the call
writes a moved copy. The related test calls `o.retries(5)`, discards the
returned value, and asserts the field on its own unmoved binding:

```rust
let o = Opts { retries: 3 };
o.retries(5);
assert_eq!(o.retries, 3);
```

The test's `o.retries` keeps its old value whether or not the write exists, so
the assertion reads a value the changed code never reached.

## When

```bash
ripr check --root fixtures/rust_copy_builder_field_write/input \
           --diff fixtures/rust_copy_builder_field_write/diff.patch --mode fast
```

## Then

The `retries` field-write finding must not be `exposed`. A by-value receiver
writes a moved copy, so a read of the caller's binding confirms nothing; the
write stays unjudged (#12061, #11398). The positive partner is a borrowed
receiver (`&mut self`), where the same exact read confirms the write, pinned by
`field_write_seam::borrowed_receiver_read_of_the_written_field_is_still_a_check`.

## Must Not

- Promote the finding to `exposed` from a read of a binding the write never
  reached.
- Use mutation-runtime outcome vocabulary.
