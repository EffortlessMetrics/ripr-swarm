<!-- section: Fixed -->
- Python static limits read `getattr`, `setattr` and `type` as a call of that bare name, with optional spaces before `(`: `getattr (o, n)` and `type (x)` now abstain as `dynamic_dispatch`/`metaprogramming`, and `my_getattr(x)` no longer does (RIPR-SPEC-0238 rule 8, examples 28 and 33; #10617).
