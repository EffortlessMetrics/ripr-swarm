# Fixture: rust_test_written_expected_file

Spec: RIPR-SPEC-0197

## Given

The return of `render(x)` changes from `x * 2` to `x * 3`. The only test writes `tests/w.txt` from `render(4)` and then compares `render(4)` against parsing that file back.

## When

`cargo xtask fixtures rust_test_written_expected_file` runs the public analyzer.

## Then

The return-value finding must not read `exposed`: the test rewrites its own expected file from the owner before comparing, so every mutant of `render` survives. It reads `weakly_exposed`.

## Must Not

- Credit a comparison whose expected value the test itself wrote.
