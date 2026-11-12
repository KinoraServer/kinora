#!/bin/sh
# Rebuild test_module.wasm from src/lib.rs.
set -e
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/kinora_test_module.wasm test_module.wasm
ls -l test_module.wasm
