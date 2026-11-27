<!-- section: Fixed -->
- `ripr --help --all`, `ripr help --all` and `ripr help --json` given an
  extra argument now write the `cli_refusal` JSON document to stdout when
  JSON was requested, naming the extra argument, as the other stray-argument
  refusals already did. The exit code and the stderr usage line are
  unchanged (#16072).
