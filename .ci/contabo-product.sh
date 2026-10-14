#!/usr/bin/env bash
set -euo pipefail
# EM CI requires a Bash entrypoint; product orchestration remains Rust-owned.
cargo run --locked -p xtask --example contabo_product_proof
