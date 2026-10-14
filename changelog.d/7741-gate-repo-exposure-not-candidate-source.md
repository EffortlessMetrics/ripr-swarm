<!-- section: Fixed -->
- `ripr gate evaluate --repo-exposure` without `--pr-guidance` or
  `--gap-ledger` is now refused up front: `--repo-exposure` is advisory
  context, not a candidate source, so naming it as a satisfying input only
  led to a `config_error` report after writing decision files. The refusal
  names the received flag and the inputs that actually feed candidates
  (#7741).
