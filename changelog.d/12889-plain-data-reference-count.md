Shared owner-reference counting admits plain enum, struct and union declarations
that do not shadow primitive type names, retaining visible function-pointer,
alias and recursive uses in the count. Implementations, type aliases, derives,
unsupported imports and opaque macros remain unresolved. This is the shared
authority portion of #12889; slice-counter integration and derived error types
remain outside this change.
