<!-- section: Fixed -->
- An `async fn` test under `#[actix_rt::test]` or `#[actix_web::test]` is now read as a test. Its exact-value assertions pin a changed owner as they do under `#[tokio::test]`, where the owner previously read `no_static_path`. The attribute must be the crate-qualified path with no arguments, and the crate name must not be rebound in the test's module (#14073).
