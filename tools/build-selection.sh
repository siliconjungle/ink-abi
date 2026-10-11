#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --locked --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/ink_abi.wasm selection.wasm
sha256sum src/selection.rs Cargo.lock Cargo.toml src/selection.mjs > selection.sources.sha256
