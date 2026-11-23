#!/usr/bin/env bash
# Fails unless src/trusted.rs is the only library file that opts out of the
# crate-wide `deny(unsafe_code)`.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
allowed="$(grep -rlE 'allow\((.*,)?\s*unsafe_code' src | sort | tr '\n' ' ')"
if [[ "$allowed" != "src/trusted.rs " ]]; then
  echo "unsafe_code may only be allowed in src/trusted.rs, found: ${allowed:-none}" >&2
  exit 1
fi
grep -q '^unsafe_code = "deny"$' Cargo.toml
grep -q '^#!\[deny(unsafe_code)\]$' src/lib.rs
echo "unsafe is confined to src/trusted.rs"
