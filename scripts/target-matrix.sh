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

installed_targets="$(rustup target list --installed)"

for target in "${targets[@]}"; do
  if grep -qx "$target" <<<"$installed_targets"; then
    echo "==> cargo check --target $target"
    cargo check --target "$target"
  else
    echo "==> skipping $target (target not installed)"
  fi
done
