#!/bin/bash
set -eu
r=/workspace/7817-repair-evidence/composition-run.sh
"$r" composition-public cargo test --locked -p ripr --test sibling_effect_oracle
"$r" composition-reveal cargo test --locked -p ripr --lib analysis::classify::reveal::tests
"$r" composition-expansion cargo test --locked -p ripr --lib analysis::classify::implicit_effect::tests
"$r" composition-transitive cargo test --locked -p ripr --lib analysis::classify::transitive_reach::tests
"$r" composition-native-roots cargo test --locked -p ripr --test native_path_roots
"$r" composition-cli-rerun cargo test --locked -p ripr --test cli_smoke rerun
"$r" composition-goldens cargo xtask goldens check
"$r" composition-honesty cargo xtask check-evidence-promotion-honesty
