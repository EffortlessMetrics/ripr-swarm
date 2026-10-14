#!/bin/bash
set -eu
r=/workspace/7817-repair-evidence/run.sh
"$r" final-honesty cargo xtask check-evidence-promotion-honesty
"$r" final-dogfood cargo xtask dogfood
"$r" final-check-fast cargo xtask check-fast
"$r" final-precommit cargo xtask precommit
