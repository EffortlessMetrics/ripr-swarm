<!-- section: Fixed -->
- Human and JSON findings: the weakly_exposed `Next step` no longer
  presumes an assertion shape the related test does not have. When the
  strongest related oracle is already exact (for example the only test is an
  `assert_eq!` on the success value), `error_path` and `return_value` probes
  now name the input that is actually missing — an input that executes the
  changed error arm, or one where the changed branch behaves differently —
  with the concrete missing discriminator when the Evidence block holds one,
  instead of advising the user to replace `is_err()` or broad assertions they
  do not own (#7393).
