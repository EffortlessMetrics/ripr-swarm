# Fixture: macro_passthrough_sibling_item_macro_binding

Spec: RIPR-SPEC-0117

## Given

`gate` changes from `input > 10` to `input >= 10`. The only test sits inside a
same-file identity item macro (`suite!`), and a sibling identity macro
(`second!`) emits a local `fn gate` in the same test module that shadows the
glob-imported owner.

## When

`ripr check --diff diff.patch` classifies the predicate probe.

## Then

The wrapped test is refused (its expansion is summarised alone and cannot see
the sibling binding), so the finding is `no_static_path` with no related test
and no oracle credit (#8999 item 5).

## Must Not

The wrapped test must not be promoted to `exposed`: that would credit the
real `gate` with an assertion that actually runs against the shadowing local
function.
