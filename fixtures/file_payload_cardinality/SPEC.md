# Fixture: file_payload_cardinality

Spec: RIPR-SPEC-0001

## Given

A reduced reconstruction of UB-review #1419's file-mediated cardinality witness:
iterator-built payloads with 128 and 129 distinct notes are written through
`fs::write`, read by `parse_file`, and compared with literal expected retained
and rejected counts. An unrelated same-file test has an exact assertion but
never calls the parser. The changed predicate is the parser's entry cap.

## When

Run `cargo xtask fixtures file_payload_cardinality`, or run the input crate's
Cargo tests. Replacing `entries.len() + 1 > ENTRY_CAP` with
`entries.len() + 1 > usize::MAX` compiles and fails the 129-note assertion:
actual `(129, 0)`, expected `(128, 1)`. The unrelated test still passes.

## Then

Current source reports `infection_unknown`, no missing discriminator, and an
explicit unresolved computed/local operand. The real caller is related by
`direct_owner_call`; the unrelated test remains only `same_test_file` proximity.
The public-API test pins these independent invariants in addition to goldens.

## Must Not

- Infer exact file-payload cardinality flow from nearby literals or proximity.
- Promote this runtime mutant rejection into static exposure or released-tool
  qualification.
- Equate this reconstructed fixture row with historical or latest UB row IDs.
- Claim a missing boundary input when current source cannot map that input.

This freezes a support boundary, not a parser or attribution capability change.
The released 0.10.0 analyzer and UB's required pin are unchanged.
