<!-- section: Changed -->
- A diff that changes no analyzable Rust file no longer walks or indexes the Rust workspace under Instant, Draft, or Fast; Deep and Ready still select the whole workspace, generated-file skip counts stay on the changed paths, and a real Rust edit still walks (#8547, #12552).
