# Fixture: whole_tail_match_macro_scalar

Spec: RIPR-SPEC-0229

## Given

An external module defines an exported macro that introduces a struct named u64
with PartialEq<i32> that always succeeds. A newly added whole-tail match returns
that custom type; an assertion compares the result to a literal. An opaque input
helper keeps individual-arm selection separate from the header-only regression.

## When

The public analyzer checks the complete added-owner diff.

## Then

The retained match header must not receive whole-tail return-observation credit.
The original and selected/other/both-arm-zero mutants all compile and pass.

## Must Not

Infer primitive identity from a return spelling supplied by an unexpanded
module-level macro. RIPR-SPEC-0108 retains this false-promotion refusal.

## Ancestor derive-prelude control

`ancestor-control` retains an out-of-line owner whose ancestor uses an external
Debug derive. Compile opaque.rs as proc-macro crate opaque, then compile the
root's tests with `--extern opaque=<libopaque.so>`. The original and selected
arm-zero variant both pass. Cargo can supply the same explicit context through
RUSTFLAGS and RUSTDOCFLAGS; a default Cargo dependency version is a different,
already-refused metadata control. This native control makes no default-manifest
compilation claim. Qualified std::assert_eq proves only the assertion macro,
not the owner file's derive prelude. The ordinary primitive variant removes
that prelude and returns numeric literals, retaining supported observation.
