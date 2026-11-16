<!-- section: Fixed -->
- A Rust path qualifier spelled like the changed owner (`thr::internal::thr(50)`)
  is no longer read as the owner passed as a function value, so the indirect-owner
  limit stops withholding a real weak-oracle gap on a direct call (#10604).
