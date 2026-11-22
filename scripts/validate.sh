#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "==> cargo test"
cargo test

echo "==> cargo test --examples"
cargo test --examples

echo "==> cargo clippy --all-targets --all-features -- -D warnings"
cargo clippy --all-targets --all-features -- -D warnings

echo "==> cargo doc --no-deps"
cargo doc --no-deps

echo "==> cargo check --benches"
cargo check --benches
