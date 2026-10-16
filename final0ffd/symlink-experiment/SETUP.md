# Exact-head root-alias experiment

Executed binary: /workspace/ripr-swarm/target/debug/ripr after the locked final-head CLI build; identity is in ../artifact-observation.json. The retained physical fixture contains Cargo.toml, ripr.toml and src/lib.rs; input.diff is adjacent to real/. Before the first three processes, every file below real/ was regular.

Setup: create directory symlink `alias -> real` outside the selected root. Run the exact three argv arrays in receipt.json, with CARGO_HOME=/workspace/.cargo, RUSTUP_HOME=/workspace/.rustup and /workspace/.cargo/bin prepended to PATH. Physical fanout exited0; aliased fanout exited2; aliased standalone JSON exited0. Raw stdout/stderr and manifests are retained. The alias symlink itself is omitted from the Git packet; this explicit relationship preserves its setup.

After those processes, create `real/src/linked.rs -> ../../external.rs` (absolute target used in execution), then run inroot-link-receipt.json's argv. It exited2 with requires-regular-input-files before analysis. This is a separate negative control; the link was absent during the original three processes. The source symlink is omitted from the Git packet; external.rs and the link target description are retained.
