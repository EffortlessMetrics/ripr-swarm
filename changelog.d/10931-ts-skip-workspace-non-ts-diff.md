<!-- section: Fixed -->
- A diff that changes no TypeScript or JavaScript file the adapter would analyze (docs, lockfiles, CI, Rust-only edits) no longer walks, reads and parses the whole TypeScript workspace, and no longer reports workspace-wide limitations for files it never consulted. A valibot docs-only commit with the adapter enabled drops from about 60 s to well under a second (#10931).
