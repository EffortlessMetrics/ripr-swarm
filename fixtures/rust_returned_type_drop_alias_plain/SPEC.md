# Fixture: rust_returned_type_drop_alias_plain

Spec: RIPR-SPEC-0197

## Given

The control for `rust_returned_type_drop_alias_exit`. The same files, re-export, owner, diff and test, but `Cfg`'s `Drop` impl has an empty body.

## When

`cargo xtask fixtures rust_returned_type_drop_alias_plain` runs the public analyzer.

## Then

The return_value finding reads `exposed`: following the rename to a plain `Drop` impl does not withhold the pin (#15107). The field_construction finding reads `weakly_exposed`, as it already did on main before #15107, because the field pin is not lent through a re-exported name. That is a conservative miss outside this fixture's claim.
