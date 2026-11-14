<!-- section: Fixed -->
- The language server no longer publishes a diagnostic, hover repair text, code
  action or code lens for a finding that `ripr check` withholds because its
  changed owner carries `#[mutants::skip]`. Workspace status discloses the
  withheld count and the `owner_mutants_skip` reason, and removing the
  attribute brings the findings back on the next refresh (#14136).
