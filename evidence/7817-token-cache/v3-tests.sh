#!/bin/bash
set -eu
r=/workspace/7817-repair-evidence/run.sh
"$r" v3-public cargo test --locked -p ripr --test sibling_effect_oracle
"$r" v3-reveal cargo test --locked -p ripr --lib analysis::classify::reveal::tests
"$r" v3-expansion cargo test --locked -p ripr --lib analysis::classify::implicit_effect::tests
"$r" v3-transitive cargo test --locked -p ripr --lib analysis::classify::transitive_reach::tests
"$r" v3-native-roots cargo test --locked -p ripr --test native_path_roots
"$r" v3-cli-rerun cargo test --locked -p ripr --test cli_smoke rerun
"$r" v3-goldens cargo xtask goldens check
"$r" v3-honesty cargo xtask check-evidence-promotion-honesty
"$r" v3-dogfood cargo xtask dogfood
"$r" v3-check-fast cargo xtask check-fast
"$r" v3-precommit cargo xtask precommit
