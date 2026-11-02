<!-- section: Fixed -->
- `ripr check --perl-facts` on an empty Rust diff no longer fails outcome validation with `NoScope requires every count to be zero` when the supplied Perl fact packet projects findings; the run now completes with `complete_with_findings` so the RIPR-SPEC-0108 corpus regeneration command can succeed on a clean checkout (#6606).
