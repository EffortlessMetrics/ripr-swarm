<!-- section: Changed -->
- The `unsafe block (unchecked assumption)` label now resolves the `unreachable_unchecked` / `assert_unchecked` call through the file's `use` bindings (bare, renamed, aliased `hint` module, `std::hint::*` glob, `intrinsics` path). A bare name nothing imports, a project function of that name, or another module's import keeps the plain `unsafe block` label (#12196).
