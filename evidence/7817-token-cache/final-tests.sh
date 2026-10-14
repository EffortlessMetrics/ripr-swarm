#!/bin/bash
set -eu
r=/workspace/7817-repair-evidence/run.sh
"$r" final-public cargo test --locked -p ripr --test sibling_effect_oracle
"$r" final-reveal cargo test --locked -p ripr --lib analysis::classify::reveal::tests
"$r" final-expansion cargo test --locked -p ripr --lib analysis::classify::implicit_effect::tests
"$r" final-transitive cargo test --locked -p ripr --lib analysis::classify::transitive_reach::tests
"$r" final-native-roots cargo test --locked -p ripr --test native_path_roots
"$r" final-cli-rerun cargo test --locked -p ripr --test cli_smoke rerun
"$r" final-goldens cargo xtask goldens check
"$r" final-honesty cargo xtask check-evidence-promotion-honesty
"$r" final-dogfood cargo xtask dogfood
"$r" final-check-fast cargo xtask check-fast
"$r" final-precommit cargo xtask precommit
