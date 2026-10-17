# Stub quality corpus

34 small Rust crates for judging the tests `ripr` suggests. Each case under
`cases/<group>_<name>/` has a `base/` crate, a `head/` crate that differs by
one behavior-changing line the existing tests do not catch (`cargo test`
passes in both), and `NOTES.md` with the change, the assertion a good test
would make, and the owner shape. Groups: A pricing and quotas, B parsers and
validators, C stateful structures and workspaces, D awkward shapes (generics,
async, lifetimes, trait defaults, macros, cfg, I/O).

Run one case by hand (the crates are not workspace members; copy one out):

```bash
cp -r benchmarks/stub_quality/cases/A_token_bucket/base /tmp/case && cd /tmp/case
git init -q . && git add -A && git commit -qm base
cp -r <repo>/benchmarks/stub_quality/cases/A_token_bucket/head/. . && git add -A && git commit -qm head
git diff HEAD~1 HEAD > /tmp/change.diff
ripr check --root . --diff /tmp/change.diff          # prints the stub command or the refusal
ripr agent stub --root . --at src/lib.rs:LINE --kind predicate --write
cargo test                                            # the stub must compile and stop at its own todo!()
```

Judge each stub on three axes: it compiles, it stops only at labelled
`todo!()` values, and the inputs it names separate the changed behavior from
its neighbor (for `>` versus `>=`, equal operands).

## Baseline (main at 6928ad1b, debug build)

16 of 34 cases offer a stub; 15 compiled and stopped at their own `todo!()`,
and `D_dyn_closure` did not compile (fixed in #8371). Three cases read
`exposed` although the changed code passes every test (tracked in #8361).

| case | static exposure | stub route |
|---|---|---|
| `A_bulk_price` | unknown | no route, no reason |
| `A_discount_codes` | unknown | no route, no reason |
| `A_invoice_summary` | weak | stub offered |
| `A_milli_rounding` | weak | no route, no reason |
| `A_sales_tax` | weak | stub offered |
| `A_seat_plans` | weak | stub offered |
| `A_token_bucket` | unknown | stub offered |
| `A_usage_overage` | unknown | no route, no reason |
| `B_arg_parse` | weak | no route, no reason |
| `B_http_status` | none | no route, no reason |
| `B_kv_config` | weak | stub offered |
| `B_name_validator` | unknown | stub offered |
| `B_path_utils` | unknown | no route, no reason |
| `B_semver_lite` | weak | stub offered |
| `B_str_case` | unknown | refused: the owner has type parameters the stub cannot choo |
| `B_tok_lexer` | unknown | stub offered |
| `C_inventory_ledger` | weak | stub offered |
| `C_lru_cache` | weak | no route, no reason |
| `C_request_builder` | exposed | no route (exposed) |
| `C_retry_backoff` | exposed | no route (exposed) |
| `C_ring_window` | weak | no route, no reason |
| `C_scheduler` | weak | no route, no reason |
| `C_state_machine` | weak | stub offered |
| `C_workspace_pricing` | weak | stub offered |
| `D_async_fn` | weak | refused: the owner is async and needs a runtime the stub ca |
| `D_dyn_closure` | unknown | stub offered |
| `D_generic_bounds` | unknown | refused: the owner has type parameters the stub cannot choo |
| `D_generic_impl` | unknown | refused: the owner's impl has type or const generics |
| `D_impl_iterator` | unknown | refused: the owner returns `impl Trait` |
| `D_io_time` | exposed | no route (exposed) |
| `D_lifetimes` | unknown | stub offered |
| `D_macro_cfg` | weak | no route, no reason |
| `D_trait_defaults` | unknown | refused: the owner is a trait default method or nested func |
| `D_unsafe_const` | unknown | stub offered |

This table is a snapshot from one run, not a gate. Re-run after stub changes
and update it in the same PR.
