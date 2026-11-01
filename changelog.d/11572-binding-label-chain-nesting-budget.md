<!-- section: Fixed -->
- `ripr check` now refuses, instead of aborting with a stack overflow, a Rust file with a long left-nested `x @ y @ ..` binding chain or a `break 'a break 'a ..` label run, and postfix call/index counting applies only to a closed group (`f()()`, `x[0][0]`), so a flat macro token tree such as `m!(foo() foo() ..)` is no longer refused as a call chain (#11572).
