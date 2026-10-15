#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --locked --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/ink_abi.wasm selection.wasm
set -- src/selection.rs src/joint_domain.rs src/adaptive.rs src/objective.rs src/usage.rs src/lib.rs Cargo.lock Cargo.toml src/selection.mjs
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "$@" > selection.sources.sha256
else
  shasum -a 256 "$@" > selection.sources.sha256
fi
