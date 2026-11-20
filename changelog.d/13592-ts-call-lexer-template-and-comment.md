<!-- section: Fixed -->
- TypeScript: a call inside a template substitution (`` `${render(x)}` ``) and a call after a `/*` written in a `//` comment or a string are now seen as calls. Both were hidden before, so code reached through them read `no_static_path`; on a replay of 13 unjs packages, 314 of the 4,027 mutants that read only `no_static_path` now get a different verdict (#13592).
