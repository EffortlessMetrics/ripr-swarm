<!-- section: Internal / CI -->
- A macro-generated Rust test pinned to its macro invocation line now takes the name scopes (imports, module path) visible at that line instead of having none when no `fn` sits on the line; the six `macro_export` verdict-corpus rows are unchanged and stay `limited` until call identity consumes the invocation-site scope (Advances #12320, #12550).
