#!/bin/bash
set -eu
r=/workspace/7817-repair-evidence/run.sh
"$r" v4-public cargo test --locked -p ripr --test sibling_effect_oracle
"$r" v4-reveal cargo test --locked -p ripr --lib analysis::classify::reveal::tests
"$r" v4-expansion cargo test --locked -p ripr --lib analysis::classify::implicit_effect::tests
"$r" v4-transitive cargo test --locked -p ripr --lib analysis::classify::transitive_reach::tests
"$r" v4-native-roots cargo test --locked -p ripr --test native_path_roots
"$r" v4-cli-rerun cargo test --locked -p ripr --test cli_smoke rerun
"$r" v4-goldens cargo xtask goldens check
"$r" v4-honesty cargo xtask check-evidence-promotion-honesty
"$r" v4-dogfood cargo xtask dogfood
"$r" v4-check-fast cargo xtask check-fast
"$r" v4-precommit cargo xtask precommit
