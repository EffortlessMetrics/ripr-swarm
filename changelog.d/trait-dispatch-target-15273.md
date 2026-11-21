<!-- section: Fixed -->
- A test whose every call of a trait default method's name runs an overriding impl, or whose every method call is taken by an inherent method of the same name, is no longer linked to the changed method by a direct call; it keeps a name-only link (#15273).
