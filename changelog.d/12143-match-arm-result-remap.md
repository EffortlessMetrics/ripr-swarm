<!-- section: Fixed -->
- A changed `match` arm is no longer read as `exposed` when the `match` value is consumed before the owner returns it (a method call on the `match`, or a `let` binding used again), which can remap the arm's result; a `let` binding returned bare still credits (RIPR-SPEC-0229, #12143).
