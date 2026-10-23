# Fixture: inherent_receiver_operand_item_shadow

Spec: RIPR-SPEC-0005

## Given

A private inherent `JsonValue::as_object` returns the object map or `None`. A
blanket foreign trait has the same method. The assertion operand declares a unit
struct named `value`, shadowing the outer `let value = JsonValue::Null`; its call
uses the foreign trait. The assertion passes even when the private object branch
returns `None`.

## When

Run `cargo xtask fixtures inherent_receiver_operand_item_shadow` or analyze the
fixture's diff. The source also compiles as a native Rust test without dependencies.

## Then

Both private-owner findings remain `weakly_exposed`. The retained decoy has a
strong whole-object-equality oracle but a low-confidence `weak_token_substring`
relation. The honesty corpus independently checks the match-arm subject, expected
row identity and refused direct-owner relation.

## Must Not

- Claim `direct_owner_call` or `helper_owner_call` from an operand-local item
- Drop the decoy row, changed owner or missing discriminator
- Infer exact Rust type or Cargo-target resolution from this bounded refusal
