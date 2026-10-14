#!/bin/bash
set -eu
r=/workspace/7817-repair-evidence/run.sh
"$r" exact-public cargo test --locked -p ripr --test sibling_effect_oracle
"$r" exact-reveal cargo test --locked -p ripr --lib analysis::classify::reveal::tests
"$r" exact-expansion cargo test --locked -p ripr --lib analysis::classify::implicit_effect::tests
"$r" exact-transitive cargo test --locked -p ripr --lib analysis::classify::transitive_reach::tests
"$r" exact-native-roots cargo test --locked -p ripr --test native_path_roots
"$r" exact-cli-rerun cargo test --locked -p ripr --test cli rerun
"$r" exact-goldens cargo xtask goldens check
"$r" exact-honesty cargo xtask check-evidence-promotion-honesty
"$r" exact-dogfood cargo xtask dogfood
"$r" exact-check-fast cargo xtask check-fast
"$r" exact-precommit cargo xtask precommit
