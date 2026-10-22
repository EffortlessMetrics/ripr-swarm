<!-- section: Fixed -->
- A helper `fn` declared inside a `#[test]` function is no longer probed as production code, so a diff that only adds such a helper reads no finding instead of `no_static_path` (#9177).
