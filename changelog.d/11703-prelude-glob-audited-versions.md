<!-- section: Fixed -->
- Verification-prelude globs (`creusot_std::prelude::*`, `vstd::prelude::*`, `prusti_contracts::*`) keep assertion credit only when `Cargo.lock` pins an audited version of that crate (creusot-std 0.14.0, vstd 0.0.0-2026-10-25-0158, prusti-contracts 0.3.3); an unlocked, newer, unknown, git or path version refuses credit as before (RIPR-SPEC-0369, #11703).
