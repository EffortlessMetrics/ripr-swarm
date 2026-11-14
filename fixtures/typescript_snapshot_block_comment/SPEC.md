# Fixture: typescript_snapshot_block_comment

Spec: RIPR-SPEC-0258
Spec: RIPR-SPEC-0108

## Given

The single-call control's changed return and valid Jest snapshot header, but the only matching entry is inside a block comment.

## When

```bash
cargo xtask fixtures typescript_snapshot_block_comment
```

## Then

The assertion remains a medium snapshot oracle and the return is `weakly_exposed`.

## Must Not

- Credit text inside a block comment as an entry.
