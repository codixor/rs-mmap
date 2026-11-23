#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

targets=(
  "wasm32-unknown-unknown"
  "x86_64-pc-windows-msvc"
  "aarch64-apple-ios"
  "x86_64-apple-ios"
  "aarch64-linux-android"
  "x86_64-linux-android"
)

rustup target add "${targets[@]}"
for target in "${targets[@]}"; do
  cargo check --locked --lib --all-features --target "$target"
done
