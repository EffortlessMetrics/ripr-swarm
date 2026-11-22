<!-- section: Fixed -->
- `ripr agent start` without `--seam-id` and `ripr agent brief` without an
  input now say where the id comes from (`ripr check` or `ripr pilot`), and
  `agent brief` names `--base <rev>` as the usual input. `ripr ""` says the
  command name is empty instead of `unknown command ""`, and its `--json`
  refusal no longer carries an empty `token`. `ripr --version` and `ripr -V`
  followed by a word exit 2 and name the word instead of printing the version
  and discarding it (#15985).
