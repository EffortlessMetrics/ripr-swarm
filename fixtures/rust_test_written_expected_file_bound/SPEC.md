# Fixture: rust_test_written_expected_file_bound

Spec: RIPR-SPEC-0197

## Given

The return of `render(x)` changes from `x * 2` to `x * 3`. The only test writes `tests/w.txt` from `render(4)`, binds the read-back contents to `want`, and compares `render(4)` against parsing `want`.

## When

`cargo xtask fixtures rust_test_written_expected_file_bound` runs the public analyzer.

## Then

The return-value finding must not read `exposed`: the expected value is bound from a file the test itself wrote, so every mutant of `render` survives. It reads `weakly_exposed`.

## Must Not

- Credit a comparison whose expected value reaches the assertion through a plain binding to a test-written file.
