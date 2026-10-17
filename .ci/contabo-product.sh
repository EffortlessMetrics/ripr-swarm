#!/usr/bin/env bash
set -euo pipefail
# EM CI requires a Bash entrypoint; product orchestration remains Rust-owned.
# Cargo forces linker/build-script temp paths into this workspace-local root.
mkdir -p target
cargo run --locked -p xtask --example contabo_product_proof
