#!/bin/bash
set -eu
r=/workspace/7817-repair-evidence/run.sh
"$r" v5-public cargo test --locked -p ripr --test sibling_effect_oracle
"$r" v5-reveal cargo test --locked -p ripr --lib analysis::classify::reveal::tests
"$r" v5-expansion cargo test --locked -p ripr --lib analysis::classify::implicit_effect::tests
"$r" v5-transitive cargo test --locked -p ripr --lib analysis::classify::transitive_reach::tests
"$r" v5-native-roots cargo test --locked -p ripr --test native_path_roots
"$r" v5-cli-rerun cargo test --locked -p ripr --test cli_smoke rerun
"$r" v5-goldens cargo xtask goldens check
"$r" v5-honesty cargo xtask check-evidence-promotion-honesty
"$r" v5-dogfood cargo xtask dogfood
"$r" v5-check-fast cargo xtask check-fast
"$r" v5-precommit cargo xtask precommit
