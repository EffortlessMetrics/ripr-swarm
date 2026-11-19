<!-- section: Fixed -->
- `ripr explain` no longer prints a "To change the verdict" test recipe for base-side findings (deleted or moved code), whose Next step already recommends no test change. An ambiguous `file:line` refusal now marks which listed ids are base-side, and the `call_deletion` change type reads "a call changed" instead of "a call was removed" (#15271).
