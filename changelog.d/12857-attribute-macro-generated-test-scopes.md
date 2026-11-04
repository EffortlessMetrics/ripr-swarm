<!-- section: Fixed -->
- A macro-generated test carrying an unrecognised attribute macro (for example `#[helpers::inject]`) no longer borrows the invocation-site name scopes, because the attribute can rewrite its body. Known harnesses and body-preserving built-ins (`#[ignore]`, `#[should_panic]`, `#[allow]`, `#[cfg]`) keep the credit (#12857).
