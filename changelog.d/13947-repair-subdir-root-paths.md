<!-- section: Fixed -->
- `ripr repair` matches its packet and operational-store rules against dirty paths respelled relative to `--root`, so a subdirectory root exempts its own tracked `target/ripr` files, no longer admits a repository-level namesake of a root-relative target, and names refusal pathspecs that resolve from inside the root (#13947).
