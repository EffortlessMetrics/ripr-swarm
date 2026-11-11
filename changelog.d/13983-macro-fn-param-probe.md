<!-- section: Fixed -->
- `ripr check` no longer probes the parameter lines of a `fn` declared directly inside a macro invocation body as `field_construction`; they read as declarations, like struct fields in a macro, while the fn body and genuine record initializers keep their probes (#13983).
