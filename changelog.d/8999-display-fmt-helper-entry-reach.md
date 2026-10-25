<!-- section: Fixed -->
- A changed helper whose one caller is a `Display`/`Debug` `fmt`
  (`writeln!(f, "{}", sanitize_label(l))`) is now reached by a test that formats a
  value of that type, such as a multi-line `assert_eq!(merge.to_string(), ..)`.
  Such a test is related as `helper_owner_call` instead of "no test is seen
  calling the helper", and the finding reads `propagation_unknown` rather than
  `weakly_exposed`; it never reads `exposed`. The receiver must be a struct
  literal or a `let` annotated with the type; `let merge = Merge::new(..)` stays
  unrelated (#8999).
