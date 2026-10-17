# Fixture: rust_committed_golden_expected_file

Spec: RIPR-SPEC-0197

## Given

The return of `render(x)` changes from `x * 2` to `x * 3`. The only test compares `render(4)` against a committed `tests/w.txt` golden (`12`) and never writes a file.

## When

`cargo xtask fixtures rust_committed_golden_expected_file` runs the public analyzer.

## Then

Exactly one return-value finding reads `exposed`: the committed golden is independent of the owner, so a wrong `render` fails the test.

## Must Not

- Demote a genuine committed-golden comparison merely because the expected operand reads a file.
