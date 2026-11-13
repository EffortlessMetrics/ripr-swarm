<!-- section: Internal / CI -->
- `cargo xtask mutation-spot-check` records selected-repository limitations and
  per-language committed versus artifact-only evidence, while the TypeScript
  workflow preserves malformed Stryker reports and still writes a receipt (#12901).
- The mutation spot-check corpus drops `glob`, `bitflags`, and `rust-shlex`,
  whose hosted runners stopped before producing a completable leg (#12901).
