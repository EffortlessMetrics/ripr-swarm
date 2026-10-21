<!-- section: Fixed -->
- `ripr doctor` no longer prints the PATH-mismatch warning when the running binary is a Cargo build output (`target/<profile>/ripr`) beside a different installed `ripr` on PATH, the normal contributor setup; a genuinely different installed binary still warns, and the identity fields stay truthful about both paths (#9097).
