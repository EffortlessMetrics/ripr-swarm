<!-- section: Fixed -->
- Repair the process-policy allowlist for `ripr first-pr --run-chain`: #11924 widened the `std::process` import in `run_chain.rs`, orphaning its `use std::process::Command` entry and failing `check-process-policy` on main (#11720).
