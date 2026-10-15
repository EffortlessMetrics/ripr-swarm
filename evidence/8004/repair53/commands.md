# Commands and identity boundaries

All Cargo commands use `/workspace/run8004` (PATH/toolchain/cache, jobs1, incremental0, dev/test debug0). Source cwd `/workspace/ripr-swarm`; composition cwd `/workspace/repair8004-composition`, with `CARGO_TARGET_DIR=/workspace/ripr-swarm/target`, serialized.

Exact source53 commands:
- cargo test --locked -p ripr --lib analysis::test_grip_evidence::tests::
- compiled default libtest binary, analysis::classify::related_tests::tests:: selection
- compiled default libtest binary, cli::rerun::tests:: selection
- cargo xtask verdict-corpus check-all
- cargo xtask goldens check
- cargo xtask check-fast
- cargo xtask precommit

Precommit source bytes before whitespace-only control cleanup/commit:
- cargo test --locked -p ripr --test private_inherent_receiver --test native_path_roots
- cargo test --locked -p ripr --test cli_smoke rerun_
- native rustc --test original/mutant controls (raw argv/results retained in native JSON)

Final CLI: clean source53 binary explicitly frozen; `check --root fixtures/<case>/input --mode ready --format json --diff fixtures/<case>/diff.patch` and same check with `--format repo-exposure-json` (no diff). Complete output and stderr retained.

See logs for compiler/tool invocation lines. A missing --locked on an xtask internal cargo build is not represented as a locked invocation. No required hosted or whole workspace test claim.
