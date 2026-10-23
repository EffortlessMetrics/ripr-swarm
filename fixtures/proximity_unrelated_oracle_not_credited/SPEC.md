# Fixture: proximity_unrelated_oracle_not_credited

Spec: RIPR-SPEC-0240

## Given

`make` is called by a test that asserts nothing. A second test in the same file
asserts an exact value of a different function and never calls `make` or
anything that reaches it.

## When

The returned expression of `make` changes and the public analyzer reads the
fixture.

## Then

The test that calls `make` reaches the change but nothing observes it, so the
finding is `reachable_unrevealed`. The unrelated same-file assertion stays
listed as an uncredited `no_call_path` row and supplies no oracle (#9167).

## Must Not

- Credit a same-file assertion that cannot reach the owner as the finding's
  oracle while another test reaches the owner.
- Report `weakly_exposed` or a strong oracle for the changed return value.
