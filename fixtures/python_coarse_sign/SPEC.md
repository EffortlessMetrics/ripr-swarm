# Fixture: python_coarse_sign

Spec: RIPR-SPEC-0238

## Given

The unchanged `authored-py-pytest-coarse` verdict-corpus subject, with
`self.assertEqual(sign_fee(2) > 0, True)` observing only a coarse projection of the owner result.
The source case is `fixtures/python-verdict-corpus/cases/py-coarse-sign.json`.

## When

The original corpus diff changes the selected fee from `x * 2` to `x * 3`.
The fixture input is the after-edit tree; its retained tests are unchanged.

## Then

The changed owner has one `weakly_exposed` finding. The corpus runtime truth
is `not_discriminated`: both implementations satisfy this coarse assertion,
although the returned values differ. The exact-value sibling remains a control.

## Must Not

Promote the changed owner to `exposed`, report a clean result, or borrow the
exact-value sibling's oracle. RIPR-SPEC-0108 independently enforces the finding
count, weak class and `must_not_promote`, even if generated goldens are re-blessed.
