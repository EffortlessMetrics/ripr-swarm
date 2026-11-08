<!-- section: Changed -->

- A diff that touches several free functions in one file no longer rescans every workspace source once per overlapping reach index: the identifier and free-function naming tallies behind unseen-reach, and the call-making-closure check of the helper-credit pass, are computed once per distinct source text per run and reused by each later index build. Output is unchanged (#13577).
