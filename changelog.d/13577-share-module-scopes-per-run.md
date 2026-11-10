<!-- section: Changed -->

- A diff that touches a large file no longer re-masks and rescans that whole file for every candidate same-file helper of every probe: the module-nesting table behind the same-file helper and const-reader scope checks is built once per distinct source text per run, and the other-caller scan names the owner before it compares related-test paths. Output is unchanged (#13577).
