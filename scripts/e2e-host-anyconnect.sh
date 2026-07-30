#!/usr/bin/env sh
# Host-side AnyConnect protocol E2E (links anyconnect-rs / OpenConnect).
# Does not require a Harmony device.
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "==> unit tests: hopenconnect_core (platform/dry-run paths)"
cargo test -p hopenconnect_core --lib

echo "==> unit tests: hopenconnect_core + native-anyconnect"
cargo test -p hopenconnect_core --features native-anyconnect --lib

echo "==> crates.io anyconnect integration smoke complete"

if [ -n "${HOPEN_E2E_SERVER:-}" ]; then
  echo "==> live obtain_cookie against HOPEN_E2E_SERVER=$HOPEN_E2E_SERVER"
  cargo test -p hopenconnect_core --features native-anyconnect --test live_connect -- --ignored --nocapture
else
  echo "skip live connect (set HOPEN_E2E_SERVER / HOPEN_E2E_USER / HOPEN_E2E_PASSWORD to enable)"
fi

echo "host anyconnect E2E OK"
