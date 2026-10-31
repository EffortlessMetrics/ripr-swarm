<!-- section: Fixed -->
- The wall-clock growth guards for the twin check on deep shared-start chains and the added-run start map now compare cache-resident sizes (1,000 against 2,000) with the same 3x bound, and `testing/scale.rs` gains a real-clock quadratic stand-in that must still fail the guard, so hosted-runner memory effects at 10,000 against 20,000 no longer fail unrelated PRs (#11238).
