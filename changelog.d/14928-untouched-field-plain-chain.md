<!-- section: Fixed -->
- A TypeScript test that reads an untouched field through a postfix non-null
  assertion (`f()!.timeout`) or a prefix negation (`!f().enabled`) no longer
  reads `exposed` when the edit moves two other values of the returned
  literal: the untouched-field guard now accepts a plain call/member-chain
  grammar (including a balanced generic call such as `f<string>()`) instead of
  rejecting any base that holds `<`, `>` or `!` (#14928). A compound base such
  as `f() ?? defaults.config` keeps its credit. A generic call is not yet
  resolved as an owner call by the reader, so it already reads
  `weakly_exposed` end to end.
