<!-- section: Fixed -->

- `ripr explain --from` and `ripr context --from`: a `file:line` miss on a replayed artifact no longer says the scope or file has no findings; the artifact does not record whether its run was complete, so the hint says none were reported and a finding there may be missing (#14440).
