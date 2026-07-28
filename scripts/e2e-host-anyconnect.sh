#!/usr/bin/env sh
# Host-side AnyConnect protocol E2E (links anyconnect-rs / OpenConnect).
# Does not require a Harmony device.
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "==> unit tests: hanyconnect_core (platform/dry-run paths)"
cargo test -p hanyconnect_core --lib

echo "==> unit tests: hanyconnect_core + native-anyconnect"
cargo test -p hanyconnect_core --features native-anyconnect --lib

echo "==> anyconnect-rs protocol smoke"
(
  cd "$ROOT_DIR/../anyconnect-rs"
  cargo test -p anyconnect --tests
)

if [ -n "${HANY_E2E_SERVER:-}" ]; then
  echo "==> live obtain_cookie against HANY_E2E_SERVER=$HANY_E2E_SERVER"
  cargo test -p hanyconnect_core --features native-anyconnect --test live_connect -- --ignored --nocapture
else
  echo "skip live connect (set HANY_E2E_SERVER / HANY_E2E_USER / HANY_E2E_PASSWORD to enable)"
fi

echo "host anyconnect E2E OK"
