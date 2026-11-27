<!-- section: Fixed -->
- A TypeScript test that reads an untouched field through a postfix non-null
  assertion (`f()!.timeout`) or a prefix negation (`!f().enabled`) no longer
  reads `exposed` when the edit moves two other values of the returned
  literal: the untouched-field guard now accepts a plain call/member-chain
  grammar (including a balanced generic call such as `f<string>()`) in
  addition to every base the earlier operator check accepted (#14928). A compound base such
  as `f() ?? defaults.config` keeps its credit. A generic call is not yet
  resolved as an owner call by the reader, so it already reads
  `weakly_exposed` end to end.
